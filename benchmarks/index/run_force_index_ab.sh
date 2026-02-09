#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  run_force_index_ab.sh [options]

Options:
  --bin <PATH>       codanna binary (required)
  --repo <PATH>      repo path to index (required)
  --config <PATH>    settings.toml path (required)
  --profiles <PATH>  profile matrix TOML
                     (default: benchmarks/index/profiles.force_ab.toml)
  --out <DIR>        output directory (default: /tmp/codanna-index-force-ab)
  --dry-run          print resolved commands and exit
  -h, --help         show help
EOF
}

BIN=""
REPO=""
CONFIG=""
PROFILES="benchmarks/index/profiles.force_ab.toml"
OUT="/tmp/codanna-index-force-ab"
DRY_RUN=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin) BIN="${2:-}"; shift 2 ;;
    --repo) REPO="${2:-}"; shift 2 ;;
    --config) CONFIG="${2:-}"; shift 2 ;;
    --profiles) PROFILES="${2:-}"; shift 2 ;;
    --out) OUT="${2:-}"; shift 2 ;;
    --dry-run) DRY_RUN=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "$BIN" || -z "$REPO" || -z "$CONFIG" ]]; then
  echo "Error: --bin, --repo, and --config are required." >&2
  usage
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PY="$SCRIPT_DIR/run_force_index_ab.py"

CMD=(
  python3 "$PY"
  --bin "$BIN"
  --repo "$REPO"
  --config "$CONFIG"
  --profiles "$PROFILES"
  --out "$OUT"
)
if [[ "$DRY_RUN" == "true" ]]; then
  CMD+=(--dry-run)
fi

"${CMD[@]}"
