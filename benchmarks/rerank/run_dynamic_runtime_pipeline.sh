#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  run_dynamic_runtime_pipeline.sh [options]

Options:
  --bin <PATH>           codanna binary
                         (default: /Users/sait/Documents/lut-app/codanna/target/release/codanna)
  --config <PATH>        settings.toml used for benchmark
                         (default: /tmp/codanna-rerank-realdata/settings.bench.toml)
  --queries <PATH>       query jsonl
                         (default: /tmp/codanna-rerank-realdata/queries.v1.jsonl)
  --qrels <PATH>         qrels jsonl
                         (default: /tmp/codanna-rerank-realdata/qrels.v1.jsonl)
  --profiles <PATH>      profile matrix TOML
                         (default: benchmarks/rerank/profiles.dynamic_runtime_sweep.toml)
  --out <DIR>            output directory (default: /tmp/codanna-dynamic-runtime)
  --cold-runs <N>        cold runs per query (default: 1)
  --warm-runs <N>        warm runs per query (default: 3)
  --limit <N>            search limit (default: 10)
  --query-timeout-ms <N> timeout classification threshold (default: 12000)
  --checkpoint-every <N> partial checkpoint interval (default: 1)
  --dry-run              print resolved command and exit
  -h, --help             show help
EOF
}

BIN="/Users/sait/Documents/lut-app/codanna/target/release/codanna"
CFG="/tmp/codanna-rerank-realdata/settings.bench.toml"
QUERIES="/tmp/codanna-rerank-realdata/queries.v1.jsonl"
QRELS="/tmp/codanna-rerank-realdata/qrels.v1.jsonl"
PROFILES="/Users/sait/Documents/lut-app/codanna/benchmarks/rerank/profiles.dynamic_runtime_sweep.toml"
OUT="/tmp/codanna-dynamic-runtime"
COLD_RUNS=1
WARM_RUNS=3
LIMIT=10
QUERY_TIMEOUT_MS=12000
CHECKPOINT_EVERY=1
DRY_RUN=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin) BIN="${2:-}"; shift 2 ;;
    --config) CFG="${2:-}"; shift 2 ;;
    --queries) QUERIES="${2:-}"; shift 2 ;;
    --qrels) QRELS="${2:-}"; shift 2 ;;
    --profiles) PROFILES="${2:-}"; shift 2 ;;
    --out) OUT="${2:-}"; shift 2 ;;
    --cold-runs) COLD_RUNS="${2:-}"; shift 2 ;;
    --warm-runs) WARM_RUNS="${2:-}"; shift 2 ;;
    --limit) LIMIT="${2:-}"; shift 2 ;;
    --query-timeout-ms) QUERY_TIMEOUT_MS="${2:-}"; shift 2 ;;
    --checkpoint-every) CHECKPOINT_EVERY="${2:-}"; shift 2 ;;
    --dry-run) DRY_RUN=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
VALIDATE_SCRIPT="$SCRIPT_DIR/validate_qrels.py"

for p in "$BIN" "$CFG" "$QUERIES" "$QRELS" "$PROFILES" "$VALIDATE_SCRIPT"; do
  if [[ ! -e "$p" ]]; then
    echo "Missing required path: $p" >&2
    exit 1
  fi
done

python3 "$VALIDATE_SCRIPT" --queries "$QUERIES" --qrels "$QRELS"

CMD=(
  "$BIN" -c "$CFG" benchmark-rerank
  --queries "$QUERIES"
  --qrels "$QRELS"
  --profiles "$PROFILES"
  --out "$OUT"
  --cold-runs "$COLD_RUNS"
  --warm-runs "$WARM_RUNS"
  --limit "$LIMIT"
  --query-timeout-ms "$QUERY_TIMEOUT_MS"
  --checkpoint-every "$CHECKPOINT_EVERY"
  --skip-warm-on-timeout true
)

echo "Resolved command:"
printf ' %q' "${CMD[@]}"
echo

if [[ "$DRY_RUN" == "true" ]]; then
  exit 0
fi

"${CMD[@]}"
echo "Done. Outputs at: $OUT"
