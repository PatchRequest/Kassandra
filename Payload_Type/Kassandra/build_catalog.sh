#!/usr/bin/env bash
# Orchestrates building of the built-in BOF / .NET catalog for Kassandra.
# Runs inside the catalog-builder Docker stage.
#
# Expects:
#   /src/tsec, /src/outflank, /src/sharp   (upstream repos cloned, pinned)
#   mingw-w64 + dotnet-sdk installed
#
# Produces:
#   /catalog/bof/<prefix>_<name>.x64.o
#   /catalog/dotnet/<prefix>_<name>.exe
#   /catalog/licenses/<collection>.LICENSE
#   /catalog/manifest.json
#   /catalog/build_errors.log

set -uo pipefail

CATALOG=/catalog
ERRLOG=$CATALOG/build_errors.log
mkdir -p "$CATALOG/bof" "$CATALOG/dotnet" "$CATALOG/licenses"
: > "$ERRLOG"

log_err() { echo "[$(date -u +%H:%M:%SZ)] $*" >> "$ERRLOG"; }

sanitize() {
    echo "$1" \
      | sed -E 's/\.(x64\.)?o$//' \
      | sed -E 's/\.exe$//' \
      | tr 'A-Z' 'a-z' \
      | sed -E 's/[^a-z0-9]+/_/g' \
      | sed -E 's/^_+|_+$//g'
}

########## TrustedSec CS-SA-BOF ##########
echo "==> Building TSEC CS-SA-BOF"
cp /src/tsec/LICENSE "$CATALOG/licenses/tsec.LICENSE" 2>/dev/null || log_err "tsec license missing"

# Makefile per tool lives in /src/tsec/src/SA/<tool>/ but `make` moves .o files
# to /src/tsec/SA/<tool>/ (note: different path, sometimes different casing).
for dir in /src/tsec/src/SA/*/; do
    tool=$(basename "$dir")
    echo "  -> tsec/$tool (build)"
    (
        cd "$dir"
        make clean >/dev/null 2>&1 || true
        if ! make >>"$ERRLOG" 2>&1; then
            log_err "tsec/$tool: make failed"
            exit 1
        fi
    ) || continue
done

# Collect from the relocated output tree
for o in /src/tsec/SA/*/*.x64.o; do
    [ -f "$o" ] || continue
    name=$(basename "$o" .x64.o)
    cp "$o" "$CATALOG/bof/tsec_$(sanitize "$name").x64.o"
done

########## Outflank C2-Tool-Collection ##########
echo "==> Building Outflank C2-Tool-Collection"
cp /src/outflank/LICENSE "$CATALOG/licenses/outflank.LICENSE" 2>/dev/null || log_err "outflank license missing"

# BOFs: Makefile at BOF/<name>/SOURCE/Makefile writes output one level up to
# BOF/<name>/<bof>.x64.o. A single tool folder can yield multiple BOFs.
for dir in /src/outflank/BOF/*/SOURCE/; do
    tool=$(basename "$(dirname "$dir")")
    echo "  -> outflank/$tool (build)"
    (
        cd "$dir"
        make clean >/dev/null 2>&1 || true
        if ! make >>"$ERRLOG" 2>&1; then
            log_err "outflank/$tool: make failed"
            exit 1
        fi
    ) || continue
done

for o in /src/outflank/BOF/*/*.x64.o; do
    [ -f "$o" ] || continue
    name=$(basename "$o" .x64.o)
    cp "$o" "$CATALOG/bof/outflank_$(sanitize "$name").x64.o"
done

# .NET under Other/
for dir in /src/outflank/Other/*/; do
    tool=$(basename "$dir")
    proj=$(find "$dir" -maxdepth 4 \( -name '*.csproj' -o -name '*.sln' \) -print -quit 2>/dev/null)
    if [ -z "$proj" ]; then
        echo "  -> outflank/$tool: no .NET project found, skipping"
        continue
    fi
    echo "  -> outflank/$tool (.NET $proj)"
    if ! dotnet build "$proj" -c Release -o "$dir/out" >>"$ERRLOG" 2>&1; then
        log_err "outflank/$tool: dotnet build failed"
        continue
    fi
    picked=""
    for exe in "$dir/out/"*.exe; do [ -f "$exe" ] && { picked="$exe"; break; }; done
    if [ -z "$picked" ]; then
        log_err "outflank/$tool: no .exe produced"
        continue
    fi
    cp "$picked" "$CATALOG/dotnet/outflank_$(sanitize "$tool").exe"
done

########## Flangvik SharpCollection (precompiled) ##########
echo "==> Copying SharpCollection (precompiled)"
cp /src/sharp/LICENSE "$CATALOG/licenses/sharp.LICENSE" 2>/dev/null || log_err "sharp license missing"

SHARP_DIR=/src/sharp/NetFramework_4.7_x64
if [ ! -d "$SHARP_DIR" ]; then
    log_err "SharpCollection: $SHARP_DIR not present; layout may have changed"
else
    for exe in "$SHARP_DIR"/*.exe; do
        [ -f "$exe" ] || continue
        name=$(basename "$exe" .exe)
        cp "$exe" "$CATALOG/dotnet/sharp_$(sanitize "$name").exe"
    done
fi

########## Manifest ##########
echo "==> Generating manifest.json"
python3 - <<'PY'
import json, os
cat = "/catalog"
entries = []
for kind, sub in (("bof", "bof"), ("dotnet", "dotnet")):
    d = os.path.join(cat, sub)
    for fname in sorted(os.listdir(d)):
        full = os.path.join(d, fname)
        if not os.path.isfile(full):
            continue
        stem = fname
        for ext in (".x64.o", ".exe", ".o"):
            if stem.endswith(ext):
                stem = stem[: -len(ext)]
                break
        source = stem.split("_", 1)[0] if "_" in stem else "unknown"
        entries.append({
            "name": stem,
            "type": kind,
            "source": source,
            "filename": fname,
            "size": os.path.getsize(full),
        })
with open(os.path.join(cat, "manifest.json"), "w") as f:
    json.dump(entries, f, indent=2)
print(f"  Wrote {len(entries)} catalog entries")
PY

echo "==> Summary"
echo "    BOFs:      $(ls "$CATALOG/bof/" 2>/dev/null | wc -l)"
echo "    .NET:      $(ls "$CATALOG/dotnet/" 2>/dev/null | wc -l)"
echo "    Error log: $(wc -l < "$ERRLOG") lines (see $ERRLOG)"
