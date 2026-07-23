"""
PE OPSEC helpers for the Kassandra builder.

Two jobs:
1. Sanitize build-time PE metadata that must not leak (timestamps, stale checksum).
2. Audit the finished payload for remaining OPSEC issues (PE-OopsSec-style checks).

Timestamp zeroing is pure PE layout — no third-party deps. Audit uses only the stdlib.
IAT "dangerous API" findings are reported but do not fail the build by default: those
require agent-side dynamic resolution, not a post-link patch.
"""

from __future__ import annotations

import math
import struct
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Iterable


# IMAGE_DIRECTORY_ENTRY_*
_DIR_EXPORT = 0
_DIR_RESOURCE = 2
_DIR_DEBUG = 6
_DIR_LOAD_CONFIG = 10

# Subsystem
_SUBSYSTEM_GUI = 2
_SUBSYSTEM_CUI = 3

# PECheck-style "suspicious" import names (informational for now)
_SUSPICIOUS_APIS = frozenset({
    "CreateRemoteThread", "WriteProcessMemory", "VirtualAllocEx", "VirtualProtectEx",
    "NtCreateSection", "NtMapViewOfSection", "QueueUserAPC", "RtlCreateUserThread",
    "SetThreadContext", "ResumeThread", "SuspendThread",
    "IsDebuggerPresent", "CheckRemoteDebuggerPresent",
    "OutputDebugStringA", "OutputDebugStringW",
    "GetTickCount", "QueryPerformanceCounter",
    "FindWindowA", "FindWindowW", "BlockInput", "VirtualProtect",
    "WinExec", "ShellExecuteA", "ShellExecuteW", "CreateProcessA", "CreateProcessW",
    "OpenProcess", "GetProcAddress", "LoadLibraryA", "LoadLibraryW",
    "RegCreateKeyExA", "RegCreateKeyExW", "RegSetValueExA", "RegSetValueExW",
})


@dataclass
class OpsecFinding:
    severity: str  # "error" | "warn" | "info"
    code: str
    message: str


@dataclass
class OpsecReport:
    findings: list[OpsecFinding] = field(default_factory=list)

    def add(self, severity: str, code: str, message: str) -> None:
        self.findings.append(OpsecFinding(severity, code, message))

    @property
    def errors(self) -> list[OpsecFinding]:
        return [f for f in self.findings if f.severity == "error"]

    def summary(self) -> str:
        lines = []
        for f in self.findings:
            lines.append(f"[{f.severity.upper()}] {f.code}: {f.message}")
        if not lines:
            return "No OPSEC findings."
        return "\n".join(lines)


class PeError(Exception):
    pass


def _u16(data: bytes, off: int) -> int:
    return struct.unpack_from("<H", data, off)[0]


def _u32(data: bytes, off: int) -> int:
    return struct.unpack_from("<I", data, off)[0]


def _set_u32(buf: bytearray, off: int, value: int) -> None:
    struct.pack_into("<I", buf, off, value)


def _pe_layout(data: bytes) -> dict:
    if len(data) < 0x40 or data[:2] != b"MZ":
        raise PeError("not a PE (missing MZ)")
    e_lfanew = _u32(data, 0x3C)
    if e_lfanew + 24 > len(data) or data[e_lfanew : e_lfanew + 4] != b"PE\0\0":
        raise PeError("invalid PE signature / e_lfanew")
    coff = e_lfanew + 4
    opt = coff + 20
    magic = _u16(data, opt)
    if magic == 0x10B:  # PE32
        num_rva_off = opt + 92
        data_dirs = opt + 96
    elif magic == 0x20B:  # PE32+
        num_rva_off = opt + 108
        data_dirs = opt + 112
    else:
        raise PeError(f"unsupported optional header magic {magic:#x}")
    number_of_rva = _u32(data, num_rva_off)
    return {
        "e_lfanew": e_lfanew,
        "coff": coff,
        "opt": opt,
        "magic": magic,
        "timestamp_off": coff + 4,  # after Machine+NumberOfSections
        "checksum_off": opt + 64,
        "subsystem_off": opt + (68 if magic == 0x10B else 68),
        "num_rva": number_of_rva,
        "data_dirs": data_dirs,
        "size_of_headers": _u32(data, opt + 60),
        "number_of_sections": _u16(data, coff + 2),
        "section_table": opt + (96 if magic == 0x10B else 112) + number_of_rva * 8,
    }


def _dir_entry(data: bytes, layout: dict, index: int) -> tuple[int, int]:
    if index >= layout["num_rva"]:
        return 0, 0
    off = layout["data_dirs"] + index * 8
    rva = _u32(data, off)
    size = _u32(data, off + 4)
    return rva, size


def _rva_to_offset(data: bytes, layout: dict, rva: int) -> int | None:
    if rva == 0:
        return None
    # Section headers: 40 bytes each
    sec = layout["section_table"]
    for i in range(layout["number_of_sections"]):
        s = sec + i * 40
        if s + 40 > len(data):
            break
        virt_size = _u32(data, s + 8)
        virt_addr = _u32(data, s + 12)
        raw_size = _u32(data, s + 16)
        raw_ptr = _u32(data, s + 20)
        size = max(virt_size, raw_size)
        if virt_addr <= rva < virt_addr + size:
            return raw_ptr + (rva - virt_addr)
    # Fallback: header region
    if rva < layout["size_of_headers"]:
        return rva
    return None


def sanitize_pe_timestamps(path: str | Path, timestamp: int = 0) -> list[str]:
    """
    Zero (or set) PE TimeDateStamp fields that leak real build time.

    Touches:
    - COFF FileHeader.TimeDateStamp
    - Export directory TimeDateStamp (if present)
    - Debug directory entry TimeDateStamp(s) (if present)
    - Resource root directory TimeDateStamp (if present)
    - OptionalHeader.CheckSum zeroed (stale checksum is worse than zero)

    Returns a list of human-readable actions taken.
    """
    path = Path(path)
    data = bytearray(path.read_bytes())
    layout = _pe_layout(data)
    actions: list[str] = []

    old_ts = _u32(data, layout["timestamp_off"])
    if old_ts != timestamp:
        _set_u32(data, layout["timestamp_off"], timestamp)
        actions.append(f"COFF TimeDateStamp {old_ts:#010x} -> {timestamp:#010x}")

    # Export dir: TimeDateStamp at +4
    exp_rva, exp_size = _dir_entry(data, layout, _DIR_EXPORT)
    if exp_rva and exp_size >= 8:
        exp_off = _rva_to_offset(data, layout, exp_rva)
        if exp_off is not None and exp_off + 8 <= len(data):
            old = _u32(data, exp_off + 4)
            if old != timestamp:
                _set_u32(data, exp_off + 4, timestamp)
                actions.append(f"Export TimeDateStamp {old:#010x} -> {timestamp:#010x}")

    # Debug dir: array of IMAGE_DEBUG_DIRECTORY (28 bytes), TimeDateStamp at +4
    dbg_rva, dbg_size = _dir_entry(data, layout, _DIR_DEBUG)
    if dbg_rva and dbg_size >= 28:
        dbg_off = _rva_to_offset(data, layout, dbg_rva)
        if dbg_off is not None:
            n = dbg_size // 28
            for i in range(n):
                ent = dbg_off + i * 28
                if ent + 8 > len(data):
                    break
                old = _u32(data, ent + 4)
                if old != timestamp:
                    _set_u32(data, ent + 4, timestamp)
                    actions.append(f"Debug[{i}] TimeDateStamp {old:#010x} -> {timestamp:#010x}")

    # Resource root: TimeDateStamp at +4 of IMAGE_RESOURCE_DIRECTORY
    res_rva, res_size = _dir_entry(data, layout, _DIR_RESOURCE)
    if res_rva and res_size >= 8:
        res_off = _rva_to_offset(data, layout, res_rva)
        if res_off is not None and res_off + 8 <= len(data):
            old = _u32(data, res_off + 4)
            if old != timestamp:
                _set_u32(data, res_off + 4, timestamp)
                actions.append(f"Resource TimeDateStamp {old:#010x} -> {timestamp:#010x}")

    # Zero optional checksum so tools don't trust a pre-stamp value
    old_cs = _u32(data, layout["checksum_off"])
    if old_cs != 0:
        _set_u32(data, layout["checksum_off"], 0)
        actions.append(f"OptionalHeader.CheckSum {old_cs:#010x} -> 0")

    if actions:
        path.write_bytes(data)
    return actions


def _read_cstring(data: bytes, off: int, limit: int = 512) -> str:
    end = data.find(b"\x00", off, off + limit)
    if end < 0:
        end = off + limit
    return data[off:end].decode("utf-8", errors="replace")


def _import_names(data: bytes, layout: dict) -> dict[str, list[str]]:
    """Best-effort parse of import DLL -> function names (PE32+ only fully supported)."""
    result: dict[str, list[str]] = {}
    imp_rva, imp_size = _dir_entry(data, layout, 1)  # IMPORT
    if not imp_rva:
        return result
    imp_off = _rva_to_offset(data, layout, imp_rva)
    if imp_off is None:
        return result

    # IMAGE_IMPORT_DESCRIPTOR is 20 bytes
    idx = 0
    while True:
        base = imp_off + idx * 20
        if base + 20 > len(data):
            break
        oft = _u32(data, base + 0)
        name_rva = _u32(data, base + 12)
        ft = _u32(data, base + 16)
        if oft == 0 and name_rva == 0 and ft == 0:
            break
        name_off = _rva_to_offset(data, layout, name_rva) if name_rva else None
        dll = _read_cstring(data, name_off).upper() if name_off is not None else f"UNK_{idx}"
        thunk_rva = oft or ft
        funcs: list[str] = []
        if thunk_rva:
            thunk_off = _rva_to_offset(data, layout, thunk_rva)
            if thunk_off is not None:
                # PE32+ thunks are 8 bytes; PE32 are 4
                entry_size = 8 if layout["magic"] == 0x20B else 4
                t = 0
                while True:
                    eoff = thunk_off + t * entry_size
                    if eoff + entry_size > len(data):
                        break
                    if entry_size == 8:
                        val = struct.unpack_from("<Q", data, eoff)[0]
                        ordinal_flag = 1 << 63
                    else:
                        val = _u32(data, eoff)
                        ordinal_flag = 1 << 31
                    if val == 0:
                        break
                    if val & ordinal_flag:
                        funcs.append(f"Ordinal_{val & 0xFFFF}")
                    else:
                        hint_rva = val & 0x7FFFFFFF if entry_size == 4 else val & 0x7FFFFFFFFFFFFFFF
                        hint_off = _rva_to_offset(data, layout, hint_rva)
                        if hint_off is not None and hint_off + 2 < len(data):
                            funcs.append(_read_cstring(data, hint_off + 2))
                    t += 1
                    if t > 4096:
                        break
        result[dll] = funcs
        idx += 1
        if idx > 512:
            break
    return result


def _debug_pdb_path(data: bytes, layout: dict) -> str | None:
    dbg_rva, dbg_size = _dir_entry(data, layout, _DIR_DEBUG)
    if not dbg_rva or dbg_size < 28:
        return None
    dbg_off = _rva_to_offset(data, layout, dbg_rva)
    if dbg_off is None:
        return None
    n = dbg_size // 28
    for i in range(n):
        ent = dbg_off + i * 28
        if ent + 28 > len(data):
            break
        dtype = _u32(data, ent + 12)
        # IMAGE_DEBUG_TYPE_CODEVIEW = 2
        if dtype != 2:
            continue
        addr = _u32(data, ent + 20)  # PointerToRawData preferred
        size = _u32(data, ent + 16)
        if addr == 0 or size < 24 or addr + size > len(data):
            # try AddressOfRawData as RVA
            rva = _u32(data, ent + 8)
            off = _rva_to_offset(data, layout, rva)
            if off is None:
                continue
            addr, size = off, _u32(data, ent + 16)
        if addr + 4 > len(data):
            continue
        magic = data[addr : addr + 4]
        if magic == b"RSDS" and addr + 24 < len(data):
            return _read_cstring(data, addr + 24, limit=1024)
        if magic == b"NB10" and addr + 16 < len(data):
            return _read_cstring(data, addr + 16, limit=1024)
    return None


def _section_entropy(data: bytes, layout: dict, name_prefix: bytes = b".text") -> float | None:
    sec = layout["section_table"]
    for i in range(layout["number_of_sections"]):
        s = sec + i * 40
        if s + 40 > len(data):
            break
        name = data[s : s + 8].split(b"\x00", 1)[0]
        if not name.startswith(name_prefix) and name.lower() not in (b"code", b".code"):
            # also accept CNT_CODE characteristic
            chars = _u32(data, s + 36)
            if not (chars & 0x20):
                continue
        raw_size = _u32(data, s + 16)
        raw_ptr = _u32(data, s + 20)
        if raw_size == 0 or raw_ptr + raw_size > len(data):
            continue
        blob = data[raw_ptr : raw_ptr + raw_size]
        return _shannon(blob)
    return None


def _shannon(blob: bytes) -> float:
    if not blob:
        return 0.0
    counts = [0] * 256
    for b in blob:
        counts[b] += 1
    ent = 0.0
    n = len(blob)
    for c in counts:
        if c:
            p = c / n
            ent -= p * math.log2(p)
    return ent


def audit_pe(path: str | Path, *, require_gui: bool = True) -> OpsecReport:
    """
    PE-OopsSec-inspired static audit. Failures (severity=error) are for issues the
    *build pipeline* is responsible for. IAT / CRT findings are warnings.
    """
    path = Path(path)
    data = path.read_bytes()
    report = OpsecReport()
    try:
        layout = _pe_layout(data)
    except PeError as e:
        report.add("error", "not_pe", str(e))
        return report

    # Subsystem
    subsystem = _u16(data, layout["subsystem_off"])
    if subsystem == _SUBSYSTEM_GUI:
        report.add("info", "subsystem", "GUI (windows)")
    elif subsystem == _SUBSYSTEM_CUI:
        sev = "error" if require_gui else "warn"
        report.add(sev, "subsystem", "CONSOLE — set no_console=true for production")
    else:
        report.add("warn", "subsystem", f"unexpected subsystem={subsystem}")

    # Timestamp
    ts = _u32(data, layout["timestamp_off"])
    if ts in (0, 0xFFFFFFFF):
        report.add("info", "timestamp", f"TimeDateStamp={ts:#010x} (neutral)")
    else:
        try:
            human = datetime.fromtimestamp(ts, tz=timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
            if 1985 <= datetime.fromtimestamp(ts, tz=timezone.utc).year <= 2035:
                report.add(
                    "error",
                    "timestamp",
                    f"real compile timestamp {human} ({ts:#010x}) — must be zeroed at build",
                )
            else:
                report.add("info", "timestamp", f"non-epoch/hash-like TimeDateStamp {ts:#010x}")
        except (OverflowError, OSError, ValueError):
            report.add("info", "timestamp", f"non-calendar TimeDateStamp {ts:#010x}")

    # PDB / CodeView
    pdb = _debug_pdb_path(data, layout)
    if pdb:
        report.add("error", "pdb", f"debug symbol path present: {pdb}")
    else:
        report.add("info", "pdb", "no CodeView PDB path")

    # Rich header (MSVC only) — presence is a warn for mingw-built agents it should be absent
    if b"Rich" in data[: min(len(data), layout["e_lfanew"] + 0x200)]:
        # only between DOS and PE
        rich_idx = data.find(b"Rich", 0, layout["e_lfanew"])
        if rich_idx != -1:
            report.add("warn", "rich_header", f"MSVC Rich header marker at offset {rich_idx}")
        else:
            report.add("info", "rich_header", "no Rich header before PE signature")
    else:
        report.add("info", "rich_header", "no Rich header")

    # Imports
    imports = _import_names(data, layout)
    flagged: list[str] = []
    has_msvcrt = False
    for dll, funcs in imports.items():
        if "msvcr" in dll.lower() or "ucrtbase" in dll.lower() or "vcruntime" in dll.lower() or dll.lower() == "msvcrt.dll":
            has_msvcrt = True
        for fn in funcs:
            if fn in _SUSPICIOUS_APIS:
                flagged.append(f"{dll}!{fn}")
    if has_msvcrt:
        # PECheck labels this /MD; for mingw+msvcrt it's expected — warn only
        report.add(
            "warn",
            "crt",
            "msvcrt/ucrt dynamic CRT present (PECheck would flag /MD; normal for x86_64-pc-windows-gnu)",
        )
    else:
        report.add("info", "crt", "no MSVC/UCRT dynamic CRT DLLs in IAT")

    if flagged:
        report.add(
            "warn",
            "iat_suspicious",
            f"{len(flagged)} high-signal imports (agent-level fix, not post-link): "
            + ", ".join(flagged[:12])
            + ("…" if len(flagged) > 12 else ""),
        )
    else:
        report.add("info", "iat_suspicious", "no PECheck-listed high-signal APIs in IAT")

    ent = _section_entropy(data, layout)
    if ent is not None:
        if ent > 7.2:
            report.add("warn", "entropy", f".text entropy {ent:.3f} (high — packed/encrypted look)")
        elif ent >= 6.0:
            report.add("info", "entropy", f".text entropy {ent:.3f} (moderate)")
        else:
            report.add("info", "entropy", f".text entropy {ent:.3f} (low/plaintext-like)")

    return report


def pe_opsec_link_rustflags(existing: str | None = None) -> str:
    """RUSTFLAGS fragment: neutral COFF timestamp at link time (mingw ld)."""
    extra = "-C link-arg=-Wl,--no-insert-timestamp"
    if not existing:
        return extra
    if "--no-insert-timestamp" in existing:
        return existing
    return f"{existing} {extra}"
