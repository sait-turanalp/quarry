#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 5 ]]; then
  echo "Usage: $0 <codanna_bin> <settings_toml> <queries_jsonl> <qrels_jsonl> <out_dir> [profiles_toml]"
  exit 1
fi

BIN="$1"
CFG="$2"
QUERIES="$3"
QRELS="$4"
OUT="$5"
PROFILES="${6:-/Users/sait/Documents/lut-app/codanna/benchmarks/rerank/profiles.stage1.toml}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

python3 "$SCRIPT_DIR/validate_qrels.py" --queries "$QUERIES" --qrels "$QRELS"

CMD=(
  "$BIN" -c "$CFG" benchmark-rerank
  --queries "$QUERIES"
  --qrels "$QRELS"
  --profiles "$PROFILES"
  --out "$OUT"
  --cold-runs 1
  --warm-runs 3
  --limit 10
  --query-timeout-ms 12000
  --checkpoint-every 1
)

echo "Ready to run benchmark:"
printf ' %q' "${CMD[@]}"
echo

echo -n "Type RUN to start benchmark (anything else cancels): "
read -r ANSWER
if [[ "$ANSWER" != "RUN" ]]; then
  echo "Cancelled."
  exit 0
fi

"${CMD[@]}"

echo "Done. Outputs at: $OUT"
