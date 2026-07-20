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
5. **Signing**: Self-signed certificate via openssl, then `osslsigncode` signs the binary.

### Docker stages

- **Stage 1 (catalog-builder)**: Clones TrustedSec CS-SA-BOF, Outflank C2-Tool-Collection, Flangvik SharpCollection at pinned commits. Compiles BOFs with mingw, .NET tools with dotnet SDK. Outputs `/catalog/` with `manifest.json`.
- **Stage 2 (runtime)**: Installs Rust nightly, Go, mingw, osslsigncode. Pre-builds Tailscale FFI static library (`libtailscale_ffi.a`) via Go cross-compilation. Copies catalog from stage 1 to `/opt/kassandra_catalog`.

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

Replaces `thread::sleep` with real computational work via the `busywork` crate. Configurable intensity (low/medium/high/ultra). `idle()` is the sleep replacement used in the main loop. `churn()` is sprinkled throughout for additional behavioral noise (after crypto ops, file downloads, transport calls).

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

## Deploy & Test Workflow

Testing happens on the Mythic lab server (`daniel@mythic`, Tailscale). Always deploy via GitHub:

```bash
# 1. Push changes to GitHub
git push origin main

# 2. Install from GitHub on the server (clones fresh, builds Docker image)
ssh daniel@mythic 'echo daniel | sudo -S bash -c "cd /home/daniel/Mythic && yes | ./mythic-cli install github https://github.com/PatchRequest/Kassandra"'

# 3. IMPORTANT: mythic-cli creates dir as "Kassandra" but Docker Compose needs lowercase "kassandra"
ssh daniel@mythic 'echo daniel | sudo -S mv /home/daniel/Mythic/InstalledServices/Kassandra /home/daniel/Mythic/InstalledServices/kassandra 2>/dev/null'

# 4. Build the Docker image (install from GH doesn't always build)
ssh daniel@mythic 'echo daniel | sudo -S bash -c "cd /home/daniel/Mythic && docker compose build kassandra"'

# 5. Start the container
ssh daniel@mythic 'echo daniel | sudo -S bash -c "cd /home/daniel/Mythic && docker compose up -d kassandra"'
```

**Cache gotcha**: Docker caches build layers. If only loader DLL source changed, Docker may reuse the cached compiled DLL. To force a clean build: `docker builder prune -af` on the server before building, or `docker compose build --no-cache kassandra`.

**Payload rebuild after deploy**: After deploying a new container, create a new payload in the Mythic UI. Old payloads use the old agent code.

**Loader DLL cross-check**: Before pushing, verify loader changes compile:
```bash
cd Payload_Type/Kassandra/Kassandra/agent_code/kassandra/loaders/dot-loader
cargo +nightly-2025-04-30 check --target x86_64-pc-windows-gnu
```

## Development Notes

- Cross-compilation target: `x86_64-pc-windows-gnu` (mingw). Uses Rust nightly (pinned to `nightly-2025-04-30`).
- The `config.rs` file cannot be compiled as-is -- the `%PLACEHOLDER%` strings are not valid Rust. It only compiles after builder.py stamps in real values.
- `lib.rs` is missing the `selfclone` feature module compared to `main.rs` -- DLL builds do not include `selfclone`.
- S3 transport debug prints say `[RECEeved]` (typo is intentional/present in code).
- Adding a new command requires: a Rust module in `features/`, a Python file in `agent_functions/`, registration in `tasking.rs`'s match arm, and `mod` declarations in both `main.rs` and `lib.rs`.
- `clroxide` crate 1.1.1 on crates.io does NOT have `run_no_redirect()` -- that method exists only in the GitHub source (unreleased). The dot-loader depends on the crates.io version.
