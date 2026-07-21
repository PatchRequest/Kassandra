# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

Kassandra is a Mythic C2 agent written in Rust, targeting Windows x86_64. It runs as a Mythic Payload Type container: a Python service handles Mythic integration (command definitions, build orchestration, translation) while the actual implant is Rust cross-compiled from Linux via `x86_64-pc-windows-gnu`.

## Repository Layout

```
Payload_Type/Kassandra/
  Dockerfile                     # Two-stage: catalog-builder + runtime
  build_catalog.sh               # Compiles BOF/.NET tools into /catalog
  main.py                        # Mythic container entry point
  translator/translator.py       # Pass-through JSON translator (KassandraTranslator)
  Kassandra/
    agent_functions/              # Python: Mythic command definitions + builder
      builder.py                 # Build orchestration, config stamping, signing
      executeRemote.py            # Server-side catalog lookup, rewrites to executeBOF/executeDOT
      listRemote.py               # Server-side only, lists catalog manifest
      *.py                        # One file per agent command
    agent_code/kassandra/         # Rust: the actual implant
      src/
        main.rs / lib.rs          # EXE vs DLL entry points (builder deletes one)
        config.rs                 # %PLACEHOLDER% values stamped at build time
        transport.rs              # Transport dispatcher (HTTP / S3 / Tailscale)
        s3_transport.rs           # S3 transport with SigV4 signing
        tailscale_transport.rs    # FFI calls into Go static library
        checkin.rs                # Initial checkin via indirect syscalls
        tasking.rs                # Task dispatch loop
        worker.rs                 # Subprocess workers for BOF/DOT/PY execution
        helpers.rs                # BusyWork evasion integration
        crypto.rs                 # AES-256-CBC + HMAC-SHA256 (encrypt-then-MAC)
        nt_mem.rs                 # Local-process memory helpers via CallGhost indirect syscalls
        selfprotect/mod.rs        # DACL manipulation to block process access
        features/*.rs             # One module per command
      tailscale_ffi/              # Go cgo library wrapping tsnet
      coffee-patched/             # Patched fork of coffee-ldr for BOF execution
      build.rs                    # Links tailscale FFI library when feature enabled
```

## Build System

### How a build works (builder.py)

1. **C2 provisioning**: Calls Mythic RPC to get credentials from the selected C2 profile (S3 bootstrap keys, Tailscale pre-auth key, or plain HTTP config).
2. **Config stamping**: Copies agent_code to a temp directory, does string replacement on `config.rs` -- all `%PLACEHOLDER%` values become concrete values (UUID, host, port, S3 keys, Tailscale auth, BusyWork intensity, etc.).
3. **EXE vs DLL**: For DLL output, `main.rs` is deleted and `[lib] crate-type = ["cdylib"]` is appended to Cargo.toml. For EXE, `lib.rs` is deleted.
4. **Cargo build**: `cargo +nightly-2025-04-30 build --release --target x86_64-pc-windows-gnu` with conditional `--features tailscale,no_console`.
5. **Authenticode stamp**: BinaryFiller `stamp-cert` copies a WIN_CERTIFICATE table from bundled goodware (e.g. PuTTY) onto the PE. Static presence only — signature does **not** cryptographically verify.

### Docker stages

- **Stage 1 (catalog-builder)**: Clones TrustedSec CS-SA-BOF, Outflank C2-Tool-Collection, Flangvik SharpCollection at pinned commits. Compiles BOFs with mingw, .NET tools with dotnet SDK. Outputs `/catalog/` with `manifest.json`.
- **Stage 2 (runtime)**: Installs Rust nightly, Go, mingw. Ships BinaryFiller corpus at `/opt/bf-corpus` and pre-builds `binary-filler` CLI for post-link cert stamping. Pre-builds Tailscale FFI static library (`libtailscale_ffi.a`) via Go cross-compilation. Copies catalog from stage 1 to `/opt/kassandra_catalog`.

### Cargo features

- `tailscale` -- enables Tailscale transport module and links Go FFI library
- `no_console` -- sets `#![windows_subsystem = "windows"]` to hide console window

### Build dependencies (build.rs)

- When `tailscale` feature is on, links `/opt/tailscale_ffi/libtailscale_ffi.a` plus Windows system libraries needed by Go runtime

## Architecture

### Agent Lifecycle (main.rs)

1. `selfprotect::set_process_security_descriptor()` -- sets DACL to deny `Everyone` access
2. If Tailscale: `tailscale_transport::init()` -- joins tailnet via embedded tsnet
3. If S3: `s3_transport::register()` -- bootstrap registration, EKE, gets per-execution IAM creds
4. `checkin::checkin()` -- sends host info, gets agent UUID back (replaces payload UUID)
5. Main loop: `tasking::getTasking()` then `helpers::idle()` (BusyWork evasion sleep)

### Transport Layer

`transport.rs` dispatches all communication through a priority chain:
1. **Tailscale** (feature-gated): Embedded WireGuard mesh via Go FFI (`tsnet`). Supports HTTP or raw TCP protocol inside the tunnel. DoH option to avoid DNS logs.
2. **S3**: Agent-to-server via S3 objects. Uses AWS SigV4 signing. Two-phase credential model: bootstrap keys (build-time) -> per-execution keys (runtime). Messages go in `{prefix}/ats/{uuid}.obj`, responses come back at `{prefix}/sta/{uuid}.obj`. Optional AES-256-CBC + HMAC-SHA256 encryption.
3. **HTTP**: Direct POST to callback host. Base64-encoded `{uuid}{payload}` body.

### Indirect Syscalls (CallGhost)

Uses [CallGhost](https://github.com/PatchRequest/CallGhost) (`syscall!(indirect, NtFoo, ...)`) for direct/indirect syscalls with Halo's Gate SSN resolution and SSN caching. The old Hell's Hall + NASM trampoline was removed.

Used for:
- `checkin.rs` — host/user/pid collection (`NtQuerySystemInformation`, `NtQueryInformationProcess`, `NtOpenProcessToken`, `NtQueryInformationToken`)
- `list_processes` / `selfclone` — process enumeration and `NtOpenProcess`
- `reflective_loader` / `mem_wipe` / `nt_mem` — `NtAllocateVirtualMemory`, `NtProtectVirtualMemory`, `NtFreeVirtualMemory`, `NtClose`
- `selfdelete` — `NtCreateFile`, `NtSetInformationFile`, `NtClose`
- `bof-loader` — local/remote memory + injection (`NtAllocateVirtualMemory`, `NtWriteVirtualMemory`, `NtCreateThreadEx`, `NtOpenProcess`)

### BusyWork Evasion (helpers.rs)

Replaces fixed-cadence `sleep` with real work via `busywork` (`PatchRequest/BusyWork` branch `bump/windows-0.61`). Levels: `off` / `low` / `medium` / `high` / `ultra`.

- **`idle()`** — one full-intensity burst between tasking rounds (callback interval). Short jittered yield after. This is the main anti-sleep surface.
- **`churn()`** — always Low, COMPUTE|MEMORY only; used at feature boundaries (not every HTTP POST). Must not starve C2.
- **`startup_delay()`** — one burst at configured intensity before first check-in.
- Transport path does **not** call BusyWork (chunked downloads issue many POSTs).

### Reflective In-Memory Loader (reflective_loader.rs, loader_cache.rs, mem_wipe.rs)

BOF and .NET execution use on-demand reflective DLL loading. Standalone loader DLLs (`loaders/bof-loader/`, `loaders/dot-loader/`) are compiled as cdylib during the Docker build and placed in `/opt/loaders/`. The agent downloads them from C2, caches them XOR-encrypted in memory (`loader_cache.rs`), and reflectively loads them when needed.

Flow: download loader DLL → XOR-decrypt → `reflective_loader::load()` → call exported `execute_bof`/`execute_dot` → wipe all memory (`mem_wipe.rs`).

- **BOF**: `bof-loader` is a forked/renamed coffee-ldr. Single export: `execute_bof(bof, bof_len, args, args_len, out, out_len) -> i32`. Arguments packed by `beacon_pack.rs` in the agent.
- **.NET**: `dot-loader` wraps `clroxide`. Single export: `execute_dot(asm, asm_len, args, args_len, out, out_len) -> i32`. clroxide uses `windows` crate 0.46 (not 0.61 like kassandra).
- **Python**: Still uses subprocess via `worker.rs` (`--worker-py`).
- **loadLoader**: Command to pre-stage loader DLLs before execution (temporal separation).

### Catalog System

The Docker build compiles a catalog of BOFs and .NET assemblies from three upstream repos at pinned commits. `build_catalog.sh` produces `/catalog/manifest.json` with entries like `{name, type, source, filename, size}`.

- `listRemote` -- server-side only (no agent round-trip), reads manifest and displays available tools
- `executeRemote` -- server-side Python looks up the tool in the manifest, registers the file with Mythic via RPC, then rewrites the task as an `executeBOF` or `executeDOT` command. The agent sees a normal `executeBOF`/`executeDOT` task.

### Encryption (crypto.rs)

AES-256-CBC with HMAC-SHA256 (encrypt-then-MAC). Key derivation uses HMAC with domain separation strings (`s3c2-enc`, `s3c2-mac`). Used by S3 transport when a pre-shared key is configured. The EKE (Encrypted Key Exchange) during S3 registration encrypts a random 32-byte session key with the PSK, server verifies by returning its hash.

### Translation Container

`KassandraTranslator` is a pass-through -- it serializes/deserializes JSON without transformation. It exists because Mythic requires a translation container when `mythic_encrypts = False`.

## Agent Commands

`ping`, `exit`, `ls`, `rm`, `mkdir`, `mv`, `cp`, `touch`, `pwd`, `upload`, `download`, `ps`, `psw` (detailed process listing), `screenshot`, `selfdelete`, `selfclone`, `executeBOF`, `executeDOT`, `executePY`, `executeRemote`, `listRemote`, `start_pivot`, `stop_pivot`, `list_pivot`, `socks`.

## Lab Test Setup (ALWAYS use this when working on the agent)

This is the permanent end-to-end lab. Do not invent a different path mid-session.

### Hosts

| Role | Access | Notes |
|------|--------|--------|
| Mythic C2 | `ssh daniel@mythic` (Tailscale) | Mythic root: `/home/daniel/Mythic`. Sudo password is the user password. |
| Windows implant host | `desktop-r9q963g.tail5a21e7.ts.net` | User `daniel`. Password auth via `sshpass` (`SSHPASS` env). |
| C2 callback address | `http://100.124.54.29:80/data` | Mythic HTTP profile on Tailscale IP of the Mythic box. |

Mythic GraphQL/API (from the Mythic host itself):
- Auth: `POST https://127.0.0.1:7443/auth` with `mythic_admin` / lab password
- GraphQL: `http://127.0.0.1:8080/v1/graphql` with `Authorization: Bearer <token>` and `x-hasura-role: mythic_admin`

Windows SSH example:
```bash
export SSHPASS='…'   # lab Windows password
HOST=daniel@desktop-r9q963g.tail5a21e7.ts.net
sshpass -e ssh -o PreferredAuthentications=password -o PubkeyAuthentication=no "$HOST" 'cmd /c …'
```

### Deploy agent container (always via GitHub)

Never edit files only on the Mythic host — push to GitHub first, then install.

```bash
# 1. Push
git push origin main

# 2. Install from GitHub (fresh clone into InstalledServices)
ssh daniel@mythic 'echo daniel | sudo -S bash -c "cd /home/daniel/Mythic && yes | ./mythic-cli install github https://github.com/PatchRequest/Kassandra"'

# 3. CASE: mythic-cli creates "Kassandra" but compose needs lowercase "kassandra"
ssh daniel@mythic 'echo daniel | sudo -S bash -c "
  cd /home/daniel/Mythic/InstalledServices
  if [ -d Kassandra ] && [ ! -d kassandra ]; then mv Kassandra kassandra
  elif [ -d Kassandra ] && [ -d kassandra ]; then rm -rf kassandra && mv Kassandra kassandra
  fi
"'

# 4. Build + start
ssh daniel@mythic 'echo daniel | sudo -S bash -c "cd /home/daniel/Mythic && docker compose build kassandra && docker compose up -d kassandra"'
```

**Cache gotcha**: Docker layer cache can keep old loader DLLs. Force rebuild with `docker compose build --no-cache kassandra` or `docker builder prune -af` on the server.

**Disk / RabbitMQ**: If check-in creates a callback but the agent never gets a response (hangs after HTTP POST), check `df -h` and `docker logs kassandra`. RabbitMQ blocks on low disk (`low on disk`); free space and restart `mythic_rabbitmq`, `kassandra`, `http`, `mythic_server`.

### Build payload for lab

After container is up, create a **new** payload (old payloads keep old agent code).

Recommended lab build parameters:
| Param | Lab value | Production default |
|-------|-----------|--------------------|
| output | `exe` | `exe` |
| C2 | HTTP → Mythic Tailscale IP / port 80 / URI `data` | same pattern |
| `no_console` | `false` if you need a console; else true | **true** |
| `busywork_intensity` | `off` or `low` while debugging tasking | **medium** |
| `debug_log` | **true** (writes `%TEMP%\kassandra_debug.log`) | **false** |

`debug_log` is a cargo feature — without it, all `dlog!` calls are compile-time no-ops (no file, no stderr).

Download the payload (Mythic UI or GraphQL file download) onto the Windows host.

### Windows host prep (Defender)

Without exclusions the agent often dies after check-in / a few tasking rounds.

```powershell
Add-MpPreference -ExclusionPath 'C:\Users\daniel'
Add-MpPreference -ExclusionPath $env:TEMP
Add-MpPreference -ExclusionProcess 'kassandra_lab.exe'
Add-MpPreference -ExclusionProcess 'kassandra.exe'
# Optional lab-only:
# Set-MpPreference -DisableRealtimeMonitoring $true
```

Stable path: copy payload to `C:\Users\daniel\kassandra_lab.exe` (not only `%TEMP%`).

### Launch agent (must fully detach from SSH)

**Broken over SSH** (process dies when the session ends):
- `start /B …`
- bare `Start-Process` without detaching from the SSH tree

**Works** — WMIC create (detached):
```bash
sshpass -e ssh … "$HOST" 'cmd /c "taskkill /F /IM kassandra_lab.exe 2>nul & del %TEMP%\kassandra_debug.log 2>nul & wmic process call create \"C:\\Users\\daniel\\kassandra_lab.exe\""'
```

Or `Start-Process` via a **separate** PowerShell that is itself started detached, then exit.

Verify:
```bash
sshpass -e ssh … "$HOST" 'cmd /c "tasklist | findstr /i kassandra & type %TEMP%\kassandra_debug.log"'
```

With `debug_log=true`, success looks like: `checkin: success agent_id=…` then repeating `getTasking: 0 task(s)` / `tasking round=N ok`.

### Tasking (GraphQL only — do not curl-steal)

**Never** `curl` `get_tasking` against a live agent UUID while testing that agent. Manual get_tasking marks tasks `agent processing` and steals them from the real implant.

Create tasks via Mythic GraphQL `createTask` on the **newest live callback** (match `pid` / `last_checkin` / `agent_callback_id` after launch).

Standard smoke suite (in order):
1. `ping`, `pwd`, `ps` — proves C2 + task loop
2. `loadLoader` with `loader_type` `bof` then `dot` (or `all`) — stages reflective loaders
3. `executeRemote` `tsec_whoami` (BOF) and `sharp_seatbelt` with `-group=system` (DOT)

Without `loadLoader` first, executeBOF/DOT fail with `loader not cached`.

Poll task status until `completed=true`. Response text may be hex-escaped (`\x…`) or base64 depending on Mythic; decode both.

Empty get_tasking body is ~38 bytes: `{"action": "get_tasking", "tasks": []}`. Non-empty tasking returns plain JSON when `mythic_encrypts=false` (translator pass-through).

### Timing expectations

- **`idle()`** = one BusyWork burst at configured intensity + short jittered yield (not 3× full bursts)
- **`churn()`** = always Low + COMPUTE|MEMORY only; never on the raw HTTP transport path
- BusyWork `off`: jittered ~80–280 ms sleep between rounds
- BusyWork `medium`: multi-second gap between rounds is normal; tasks should still leave `submitted` within one idle cycle once the agent polls
- Multi-minute hang after POST with callback already created on Mythic = **infra** (RabbitMQ/disk/translator)
- Never curl `get_tasking` for a live agent UUID — steals tasks into orphaned `agent processing`

BusyWork intensity ladder lives in `PatchRequest/BusyWork` branch `bump/windows-0.61`.

### Minimal end-to-end checklist

1. `git push origin main`
2. Reinstall + rename service dir + `docker compose build/up kassandra`
3. New payload: HTTP C2, `debug_log=true`, `busywork=off` or `low` for fast loops
4. Copy to `C:\Users\daniel\kassandra_lab.exe`, Defender exclusions
5. WMIC launch; wait for check-in (Mythic callback + debug log)
6. GraphQL: ping/pwd/ps → loadLoader → executeRemote whoami + seatbelt
7. Confirm all tasks `completed` with real output

### Loader compile cross-check (before push)

```bash
cd Payload_Type/Kassandra/Kassandra/agent_code/kassandra/loaders/dot-loader
cargo +nightly-2025-04-30 check --target x86_64-pc-windows-gnu
# same for loaders/bof-loader if BOF loader changed
```

## Development Notes

- Cross-compilation target: `x86_64-pc-windows-gnu` (mingw). Uses Rust nightly (pinned to `nightly-2025-04-30`).
- The `config.rs` file cannot be compiled as-is -- the `%PLACEHOLDER%` strings are not valid Rust. It only compiles after builder.py stamps in real values.
- Cargo features: `tailscale`, `no_console`, `debug_log` (lab diagnostics only).
- Production builder defaults: `no_console=true`, `busywork_intensity=medium`, `debug_log=false`.
- Adding a new command requires: a Rust module in `features/`, a Python file in `agent_functions/`, registration in `tasking.rs`'s match arm, and `mod` declarations in both `main.rs` and `lib.rs`.
- `clroxide` crate 1.1.1 on crates.io does NOT have `run_no_redirect()` -- that method exists only in the GitHub source (unreleased). The dot-loader depends on the crates.io version.
- Related repo: BusyWork (`https://github.com/PatchRequest/BusyWork`, branch `bump/windows-0.61`) — intensity ladder must stay real (no flattening `.min(N)` caps on task iterations).
