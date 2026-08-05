#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  run_static_int8_pipeline.sh [options]

Options:
  --mode <fast|prod>                    Calibration preset (default: fast)
  --calib-size <N>                      Override calibration sample size
  --model-src <DIR>                     Source model dir (default: auto-resolve Jina v1 FP32 snapshot cache)
  --config <PATH>                       settings.toml for benchmark (default: /tmp/quarry-rerank-realdata/settings.bench.toml)
  --queries <PATH>                      queries.v1.jsonl (default: /tmp/quarry-rerank-realdata/queries.v1.jsonl)
  --qrels <PATH>                        qrels.v1.jsonl (default: /tmp/quarry-rerank-realdata/qrels.v1.jsonl)
  --calibration-jsonl <PATH>            candidates.raw.jsonl (default: /tmp/quarry-rerank-realdata/candidates.raw.jsonl)
  --out-root <DIR>                      Output root dir (default: /tmp/quarry-static-int8)
  --quarry-bin <PATH>                  quarry binary (default: /Users/sait/Documents/lut-app/quarry/target/release/quarry)
  --workspace-cwd <DIR>                 Working directory for smoke mcp call (default: /Users/sait/Documents/gemini-cli)
  --bootstrap-pydeps <true|false>       Create venv + install Python deps (default: true)
  --calibration-method <name>           percentile|entropy|minmax (default: percentile)
  --quant-max-length <N>                Max length used while calibration encoding (default: 1024)
  --mixed-precision <none|jina_v1_mixed|jina_v1_mixed_v2>
                                         Exclude sensitive nodes from quantization (default: none)
  --exclude-node-patterns <CSV_REGEX>   Additional node-name regex patterns to exclude
  --prefer-format <qoperator|qdq>       Quant format priority (default: qoperator)
  --dry-run                             Print resolved plan and exit
  -h, --help                            Show this help
EOF
}

MODE="fast"
CALIB_SIZE=""
MODEL_SRC=""
CFG="/tmp/quarry-rerank-realdata/settings.bench.toml"
QUERIES="/tmp/quarry-rerank-realdata/queries.v1.jsonl"
QRELS="/tmp/quarry-rerank-realdata/qrels.v1.jsonl"
CALIB_JSONL="/tmp/quarry-rerank-realdata/candidates.raw.jsonl"
OUT_ROOT="/tmp/quarry-static-int8"
QUARRY_BIN="/Users/sait/Documents/lut-app/quarry/target/release/quarry"
WORKSPACE_CWD="/Users/sait/Documents/gemini-cli"
BOOTSTRAP_PYDEPS="true"
CALIB_METHOD="percentile"
QUANT_MAX_LENGTH="1024"
MIXED_PRECISION="none"
EXCLUDE_NODE_PATTERNS=""
DRY_RUN="false"
PREFER_FORMAT="qoperator"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode) MODE="${2:-}"; shift 2 ;;
    --calib-size) CALIB_SIZE="${2:-}"; shift 2 ;;
    --model-src) MODEL_SRC="${2:-}"; shift 2 ;;
    --config) CFG="${2:-}"; shift 2 ;;
    --queries) QUERIES="${2:-}"; shift 2 ;;
    --qrels) QRELS="${2:-}"; shift 2 ;;
    --calibration-jsonl) CALIB_JSONL="${2:-}"; shift 2 ;;
    --out-root) OUT_ROOT="${2:-}"; shift 2 ;;
    --quarry-bin) QUARRY_BIN="${2:-}"; shift 2 ;;
    --workspace-cwd) WORKSPACE_CWD="${2:-}"; shift 2 ;;
    --bootstrap-pydeps) BOOTSTRAP_PYDEPS="${2:-}"; shift 2 ;;
    --calibration-method) CALIB_METHOD="${2:-}"; shift 2 ;;
    --quant-max-length) QUANT_MAX_LENGTH="${2:-}"; shift 2 ;;
    --mixed-precision) MIXED_PRECISION="${2:-}"; shift 2 ;;
    --exclude-node-patterns) EXCLUDE_NODE_PATTERNS="${2:-}"; shift 2 ;;
    --prefer-format) PREFER_FORMAT="${2:-}"; shift 2 ;;
    --dry-run) DRY_RUN="true"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage; exit 1 ;;
  esac
done

resolve_default_model_src() {
  local snapshots_root="$HOME/.quarry/models/models--jinaai--jina-reranker-v1-turbo-en/snapshots"
  if [[ -d "$snapshots_root" ]]; then
    local snap
    snap="$(find "$snapshots_root" -mindepth 1 -maxdepth 1 -type d | head -n 1 || true)"
    if [[ -n "${snap:-}" && -f "$snap/onnx/model.onnx" ]]; then
      echo "$snap"
      return 0
    fi
  fi

  # Fallback to local dynamic INT8 copy if snapshot cache is unavailable.
  local fallback="$HOME/.quarry/models/jina-reranker-v1-turbo-en-int8"
  if [[ -d "$fallback" ]]; then
    echo "$fallback"
    return 0
  fi
  return 1
}

if [[ -z "$MODEL_SRC" ]]; then
  MODEL_SRC="$(resolve_default_model_src || true)"
fi

if [[ -z "$MODEL_SRC" ]]; then
  echo "Could not resolve default --model-src. Please pass it explicitly." >&2
  exit 1
fi

case "$MODE" in
  fast) DEFAULT_CALIB=500 ;;
  prod) DEFAULT_CALIB=1000 ;;
  *) echo "--mode must be fast|prod (got: $MODE)" >&2; exit 1 ;;
esac

if [[ -z "$CALIB_SIZE" ]]; then
  CALIB_SIZE="$DEFAULT_CALIB"
fi

for p in "$MODEL_SRC" "$CFG" "$QUERIES" "$QRELS" "$CALIB_JSONL" "$QUARRY_BIN"; do
  if [[ ! -e "$p" ]]; then
    echo "Missing required path: $p" >&2
    exit 1
  fi
done

if ! [[ "$QUANT_MAX_LENGTH" =~ ^[0-9]+$ ]] || [[ "$QUANT_MAX_LENGTH" -le 0 ]]; then
  echo "--quant-max-length must be a positive integer (got: $QUANT_MAX_LENGTH)" >&2
  exit 1
fi

case "$CALIB_METHOD" in
  percentile|entropy|minmax) ;;
  *)
    echo "--calibration-method must be one of: percentile|entropy|minmax (got: $CALIB_METHOD)" >&2
    exit 1
    ;;
esac

case "$MIXED_PRECISION" in
  none|jina_v1_mixed|jina_v1_mixed_v2) ;;
  *)
    echo "--mixed-precision must be one of: none|jina_v1_mixed|jina_v1_mixed_v2 (got: $MIXED_PRECISION)" >&2
    exit 1
    ;;
esac

case "$PREFER_FORMAT" in
  qoperator|qdq) ;;
  *)
    echo "--prefer-format must be qoperator|qdq (got: $PREFER_FORMAT)" >&2
    exit 1
    ;;
esac

TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$OUT_ROOT/run-$TS"
MODEL_OUT_DIR="$RUN_DIR/model_static_int8"
BENCH_OUT_DIR="$RUN_DIR/benchmark"
PROFILE_TOML="$RUN_DIR/profiles.static_vs_jina_v1.toml"
SMOKE_CFG="$RUN_DIR/settings.smoke.static.toml"
VENV_DIR="$RUN_DIR/.venv"
PY_BIN="${PY_BIN:-python3}"

mkdir -p "$RUN_DIR"

if [[ "$DRY_RUN" == "true" ]]; then
  cat <<EOF
[dry-run] resolved configuration:
  mode: $MODE
  calib_size: $CALIB_SIZE
  model_src: $MODEL_SRC
  cfg: $CFG
  queries: $QUERIES
  qrels: $QRELS
  calibration_jsonl: $CALIB_JSONL
  out_root: $OUT_ROOT
  run_dir: $RUN_DIR
  model_out_dir: $MODEL_OUT_DIR
  benchmark_out_dir: $BENCH_OUT_DIR
  quarry_bin: $QUARRY_BIN
  workspace_cwd: $WORKSPACE_CWD
  bootstrap_pydeps: $BOOTSTRAP_PYDEPS
  calibration_method: $CALIB_METHOD
  quant_max_length: $QUANT_MAX_LENGTH
  mixed_precision: $MIXED_PRECISION
  exclude_node_patterns: $EXCLUDE_NODE_PATTERNS
  prefer_format: $PREFER_FORMAT
EOF
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
QUANT_SCRIPT="$SCRIPT_DIR/quantize_static_onnx.py"
VALIDATE_SCRIPT="$SCRIPT_DIR/validate_qrels.py"

if [[ ! -f "$QUANT_SCRIPT" ]]; then
  echo "Missing script: $QUANT_SCRIPT" >&2
  exit 1
fi

if [[ "$BOOTSTRAP_PYDEPS" == "true" ]]; then
  "$PY_BIN" -m venv "$VENV_DIR"
  source "$VENV_DIR/bin/activate"
  pip install --quiet --upgrade pip
  pip install --quiet "onnx>=1.15" "onnxruntime>=1.19" "numpy>=1.24" "transformers>=4.40"
  PY_RUN="$VENV_DIR/bin/python"
else
  PY_RUN="$PY_BIN"
fi

"$PY_RUN" "$VALIDATE_SCRIPT" --queries "$QUERIES" --qrels "$QRELS"

"$PY_RUN" "$QUANT_SCRIPT" \
  --model-dir "$MODEL_SRC" \
  --calibration-jsonl "$CALIB_JSONL" \
  --out-dir "$MODEL_OUT_DIR" \
  --sample-size "$CALIB_SIZE" \
  --max-length "$QUANT_MAX_LENGTH" \
  --calibration-method "$CALIB_METHOD" \
  --exclude-preset "$MIXED_PRECISION" \
  --exclude-node-patterns "$EXCLUDE_NODE_PATTERNS" \
  --prefer-format "$PREFER_FORMAT"

cp "$CFG" "$SMOKE_CFG"
perl -0pi -e \
  's#(^\[reranking\][\s\S]*?\nmodel\s*=\s*)"[^"]+"#$1"custom:'"$MODEL_OUT_DIR"'"#m; s/^top_n\s*=\s*\d+/top_n = 30/m; s/^timeout_ms\s*=\s*\d+/timeout_ms = 15000/m; s/^max_length\s*=\s*\d+/max_length = 1024/m; s/^post_rerank_heuristics_enabled\s*=\s*(true|false)/post_rerank_heuristics_enabled = false/m' \
  "$SMOKE_CFG"

echo "[smoke] static INT8 model check"
(
  cd "$WORKSPACE_CWD"
  "$QUARRY_BIN" -c "$SMOKE_CFG" mcp semantic_search_chunks query:"edit tool" limit:8 >/tmp/static_int8_smoke.out 2>/tmp/static_int8_smoke.err || true
)
if rg -q "Reranker init failed|Failed to load custom reranker|Error:" /tmp/static_int8_smoke.err /tmp/static_int8_smoke.out; then
  echo "Smoke test failed. See:" >&2
  echo "  /tmp/static_int8_smoke.out" >&2
  echo "  /tmp/static_int8_smoke.err" >&2
  exit 1
fi

cat >"$PROFILE_TOML" <<EOF
[defaults]
timeout_ms = 15000
max_length = 1024
rerank_score_normalization = "none"
top_k_vector = 100
top_k_bm25 = 100
top_k_fused = 50

[[profiles]]
name = "jina_v1_tn30_heur_off"
model = "JINARerankerV1TurboEn"
top_n = 30
post_rerank_heuristics_enabled = false

[[profiles]]
name = "jina_v1_static_int8_tn30_heur_off"
model = "custom:$MODEL_OUT_DIR"
top_n = 30
post_rerank_heuristics_enabled = false
EOF

"$QUARRY_BIN" -c "$CFG" benchmark-rerank \
  --queries "$QUERIES" \
  --qrels "$QRELS" \
  --profiles "$PROFILE_TOML" \
  --out "$BENCH_OUT_DIR" \
  --cold-runs 1 \
  --warm-runs 3 \
  --limit 10 \
  --query-timeout-ms 12000 \
  --checkpoint-every 1 \
  --skip-warm-on-timeout true

"$PY_RUN" - <<'PY' "$BENCH_OUT_DIR/summary.json" "$RUN_DIR/decision_gate.md"
import json, sys
from pathlib import Path

summary_path = Path(sys.argv[1])
out_path = Path(sys.argv[2])
d = json.loads(summary_path.read_text())
profiles = {p["profile"]: p for p in d["profiles"]}
base = profiles.get("jina_v1_tn30_heur_off")
new = profiles.get("jina_v1_static_int8_tn30_heur_off")
if not base or not new:
    out_path.write_text("Missing expected profiles in summary.json\n")
    print(f"Wrote: {out_path}")
    raise SystemExit(0)

def delta(a, b, key):
    return round(b[key] - a[key], 3)

dhit = delta(base, new, "hit_at_1")
dmrr = delta(base, new, "mrr_at_10")
dndcg = delta(base, new, "ndcg_at_10")
speed_gain = 0.0
if base["warm_p95_ms"] > 0:
    speed_gain = round((base["warm_p95_ms"] - new["warm_p95_ms"]) / base["warm_p95_ms"], 3)
timeout_ok = new["timeout_queries"] <= base["timeout_queries"]

quality_ok = (dhit >= -0.01) and (dmrr >= -0.02) and (dndcg >= -0.02)
latency_ok = speed_gain >= 0.20
decision = "PROMOTE_STATIC" if (quality_ok and latency_ok and timeout_ok) else "KEEP_DYNAMIC"

lines = [
    "# Static INT8 Decision Gate",
    "",
    f"- Decision: **{decision}**",
    f"- Delta Hit@1: {dhit}",
    f"- Delta MRR@10: {dmrr}",
    f"- Delta nDCG@10: {dndcg}",
    f"- Warm p95 gain: {speed_gain * 100:.1f}%",
    f"- Timeout guard ok: {timeout_ok}",
    "",
    "## Thresholds",
    "",
    "- Hit@1 drop <= 0.01",
    "- MRR@10 drop <= 0.02",
    "- nDCG@10 drop <= 0.02",
    "- Warm p95 improvement >= 20%",
    "- Timeout queries must not increase",
]
out_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"Wrote: {out_path}")
PY

echo
echo "Pipeline completed."
echo "Run dir: $RUN_DIR"
echo "Model: $MODEL_OUT_DIR"
echo "Benchmark: $BENCH_OUT_DIR"
echo "Summary table: $BENCH_OUT_DIR/summary_table.md"
echo "Decision gate: $RUN_DIR/decision_gate.md"
