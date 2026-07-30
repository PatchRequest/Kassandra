# Kassandra — Rust Mythic Agent

**Kassandra** is a custom [Mythic](https://github.com/its-a-feature/Mythic) C2 agent written in **Rust**, packaged as a **Python payload-type container**. It targets **Windows x86_64**, cross-compiled from Linux via `x86_64-pc-windows-gnu`. Build as **EXE**, **DLL**, or **shellcode** (Donut).

> This public release does not include every private obfuscation / defense-evasion technique. Some components are simplified intentionally. The full private build remains controlled red-team use only.

<p align="center">
  <img src="./KassandraLogo.png" width="400" />
</p>

## Installation

From the Mythic install directory:

```bash
cd /path/to/Mythic
sudo ./mythic-cli install github https://github.com/PatchRequest/Kassandra
```

Or from a local folder:

```bash
sudo ./mythic-cli install folder /path/to/Kassandra
```

**Case note:** `mythic-cli` may create `InstalledServices/Kassandra` while Docker Compose expects lowercase `kassandra`. Rename if needed:

```bash
sudo mv InstalledServices/Kassandra InstalledServices/kassandra
sudo docker compose build kassandra && sudo docker compose up -d kassandra
```

---

## Features

### BusyWork evasion ([BusyWork](https://github.com/PatchRequest/BusyWork))

Replaces fixed-cadence `sleep()` with **real, varied work** (compute, memory, WinAPI, registry, crypto, …) so callback intervals do not look like pure idle-then-act beacons.

| API | Role |
|-----|------|
| `idle()` | **One** full-intensity burst between tasking rounds + short jittered yield. Main callback-interval surface. |
| `churn()` | Always **Low**, **COMPUTE \| MEMORY** only — light noise at feature boundaries (not on every HTTP POST). |
| `startup_delay()` | One burst at configured intensity before first check-in. |

- Operator-selectable levels: **`off` / `low` / `medium` / `high` / `ultra`**
- Real program data is fed in via `feed()` (UUID, host, paths, outputs, …)
- **C2 transport path does not run BusyWork** — chunked uploads/downloads issue many POSTs; heavy work stays in `idle()` so Medium/High remain usable without starving tasking
- Intensity ladder lives in BusyWork (`bump/windows-0.61`); levels must stay a real volume ladder (no flattening `.min(N)` caps)

### Indirect syscalls ([CallGhost](https://github.com/PatchRequest/CallGhost))

Hell’s Hall + NASM trampoline was **replaced** with CallGhost:

```rust
syscall!(indirect, NtFoo, /* args */);
```

Halo’s Gate SSN resolution, SSN caching, used across:

- Check-in (host / user / PID)
- Process list & selfclone
- Reflective loader / mem wipe / local Nt helpers
- Self-delete
- BOF loader injection primitives

### Process hardening (`selfprotect`)

Sets a restrictive process DACL (deny Everyone generic-all; allow System + owner) so casual handle opens against the implant process fail. Failures are silent (no `eprintln`).

### In-memory BOF / .NET execution

Loader code is **not** linked into the agent binary:

1. Standalone `bof_loader.dll` / `dot_loader.dll` built in Docker → `/opt/loaders/`
2. Agent downloads loaders from C2, stores them **XOR-encrypted** in memory (`loader_cache`)
3. Reflective load → `execute_bof` / `execute_dot` → wipe (`mem_wipe`)

| Piece | Notes |
|-------|--------|
| BOF | Forked/renamed coffee-ldr style loader |
| .NET | `clroxide`-based `dot-loader` |
| Python | Still subprocess worker (`--worker-py`) |
| `loadLoader` | Pre-stage loaders (temporal separation from execution) |

### Built-in BOF / .NET catalog

Docker **catalog-builder** stage compiles tools from pinned upstream commits into `/opt/kassandra_catalog` + `manifest.json`.

| Source | Type | Prefix |
|--------|------|--------|
| [TrustedSec CS-SA-BOF](https://github.com/trustedsec/CS-Situational-Awareness-BOF) | BOF | `tsec_` |
| [Outflank C2-Tool-Collection](https://github.com/outflanknl/C2-Tool-Collection) | BOF | `outflank_` |
| [Flangvik SharpCollection](https://github.com/Flangvik/SharpCollection) | .NET | `sharp_` |

- **`listRemote [filter]`** — server-side only (no agent round-trip)
- **`executeRemote -tool_name <name> [-parameters …]`** — container registers the file and rewrites the task to `executeBOF` / `executeDOT`

```text
listRemote kerb
executeRemote -tool_name tsec_whoami
executeRemote -tool_name sharp_seatbelt -parameters "-group=system"
```

### C2 transports

Priority dispatch in `transport.rs`:

1. **Tailscale** (feature) — embedded tsnet / WireGuard via Go FFI; HTTP or raw TCP inside the tunnel; optional DoH
2. **S3** — SigV4, bootstrap → per-execution IAM creds, optional AES-256-CBC + HMAC-SHA256 (EKE)
3. **HTTP** — Mythic HTTP profile; body `base64(uuid ‖ json)`

Translation container **`KassandraTranslator`** is a JSON pass-through (`mythic_encrypts = false`).

### Proxy & pivot

- SOCKS via Mythic
- Pivot listeners (`start_pivot` / `stop_pivot` / `list_pivot`)

### Core agent ops

Filesystem (ls/rm/mkdir/mv/cp/touch/pwd), upload/download, process list (`ps` / `psw`), screenshot, selfdelete, selfclone, ping, exit.

**`selfclone`** (default **earlybird**): PPID-spoof a sacrificial host (`CREATE_SUSPENDED` under e.g. `explorer.exe`), inject Donut shellcode of this payload via `NtQueueApcThread`, resume. Process tree looks like `explorer → RuntimeBroker` (not agent → child). Mode `process` keeps the legacy “spawn own EXE under spoofed PPID” behavior.

### Shellcode output ([Donut](https://github.com/TheWover/donut))

Build `output=shellcode` and the agent is compiled as a normal EXE, then converted to position-independent shellcode with Donut (same idea as Mythic Apollo).

| Setting | Options | Default |
|---------|---------|---------|
| `shellcode_format` | Binary, Base64, C, Ruby, Python, Powershell, C#, Hex | Binary |
| `shellcode_bypass` | None / Abort on fail / Continue on fail | Continue on fail |

Flow: **cargo EXE → PE OPSEC audit → Donut (`-x3 -k2 -f… -b…`) → `.bin` (or chosen format)**. Authenticode is skipped for shellcode (would only bloat Donut input). Use any standard shellcode runner / injector on target.

---

## Build parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `output` | **exe** / dll / **shellcode** | exe | Payload format |
| `shellcode_format` | see above | Binary | Donut format (only if `output=shellcode`) |
| `shellcode_bypass` | see above | Continue on fail | Donut AMSI/WLDP/ETW bypass (only if `output=shellcode`) |
| `adjust_filename` | bool | **true** | Auto extension (`.exe` / `.dll` / `.bin` / …) |
| `chunk_size` | string | 4096 | Upload/download chunk size |
| `busywork_intensity` | off / low / medium / high / ultra | **medium** | BusyWork intensity for `idle` / startup |
| `no_console` | bool | **true** | `windows_subsystem = windows` |
| `debug_log` | bool | **false** | Lab only: lifecycle log → `%TEMP%\kassandra_debug.log` |
| `tailscale_protocol` | http / tcp | http | Protocol inside WireGuard |
| `doh` | off / cloudflare / google / custom | off | DoH for Tailscale DNS |
| `doh_url` | string | — | Custom DoH URL when `doh=custom` |

**Production defaults:** `no_console=true`, `busywork=medium`, `debug_log=false`.  
**Lab:** prefer `debug_log=true` and `busywork=off` or `low` while debugging tasking.

### Cargo features

| Feature | Effect |
|---------|--------|
| `tailscale` | Tailscale transport + link Go FFI |
| `no_console` | Hide console window |
| `debug_log` | Enable `dlog!` file logging (otherwise compile-time no-op) |

---

## Architecture (short)

```text
checkin (CallGhost syscalls for host/user/pid)
    → main loop:
         get_tasking  →  handleTask  →  idle() BusyWork
```

Reflective path:

```text
loadLoader / on-demand download
    → XOR cache → reflective_loader → execute_bof / execute_dot → mem wipe
```

Related repos:

- [BusyWork](https://github.com/PatchRequest/BusyWork) — intensity work engine (`bump/windows-0.61`)
- [CallGhost](https://github.com/PatchRequest/CallGhost) — indirect syscalls

---

## Repository layout

```text
Payload_Type/Kassandra/
  Dockerfile                 # catalog-builder + runtime (+ Donut)
  build_catalog.sh
  main.py
  translator/translator.py
  Kassandra/
    agent_functions/         # Mythic commands + builder.py
    agent_code/kassandra/    # Rust implant
      src/                   # main, tasking, transport, features, …
      loaders/               # bof-loader, dot-loader (cdylib)
      tailscale_ffi/         # Go tsnet wrapper
```

---

## Design notes (recent)

Ideas baked into the current public tree:

1. **Evasion without starving C2** — heavy BusyWork only between rounds (`idle`); light `churn` at boundaries; never on raw HTTP I/O.
2. **Loaders out of the implant** — reflective DLLs staged from C2; optional `loadLoader` for OPSEC timing.
3. **Catalog as container content** — operators run tools without manual uploads; agent only sees normal BOF/DOT tasks.
4. **Syscalls via a maintained crate** — CallGhost instead of a bespoke Hell’s Hall + ASM stack.
5. **Shellcode via Donut** — optional PIC output for inject workflows without a separate packing pipeline.
6. **Silent production build** — no debug file/stderr unless `debug_log` is explicitly enabled at build time.
7. **Self-protect by default** — DACL lockdown on EXE and DLL entry (lab builds should not permanently disable this).

---

## Blog posts

Series on [patchi.fyi](https://patchi.fyi):

- [Architecture Overview](https://patchi.fyi/blog/kassandra-architecture-overview/)
- [Hell's Hall Syscalls](https://patchi.fyi/blog/kassandra-hells-hall-syscalls/) *(historical; agent now uses CallGhost)*
- [S3 Transport](https://patchi.fyi/blog/kassandra-s3-transport/)
- [In-Memory Execution](https://patchi.fyi/blog/kassandra-in-memory-execution/) *(historical for BOF/.NET; see reflective loading)*
- [Process Hardening](https://patchi.fyi/blog/kassandra-process-hardening/)
- [BusyWork Integration](https://patchi.fyi/blog/kassandra-busywork-integration/)
- [Reflective Loading](https://patchi.fyi/blog/kassandra-reflective-loading/)

---

## Disclaimer

Educational and authorized red-team use only. Do not use without proper authorization.

---

Thanks to [@Yeeb1](https://github.com/Yeeb1) for the [awss3](https://github.com/Yeeb1/awss3) S3 C2 profile, [Tailscale C2](https://github.com/Yeeb1/mythic_tailscale), and agent improvements.

Thanks to MalDevAcademy, VX-Underground, @ZkClown, and Ze_Asimovitch for training, archives, and inspiration in the red-team community.
