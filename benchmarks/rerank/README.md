# Reranker Benchmark Inputs

This folder contains a complete pipeline for real graded benchmark runs.

## 1) Queries

Create `queries.v1.jsonl` with one object per line:

```json
{"id":"q1","query":"edit tool"}
```

Use `queries.template.jsonl` as a starter.

## 2) Candidate chunk extraction

Generate labeling candidates (`top-30` by default):

```bash
python3 benchmarks/rerank/prepare_candidates.py \
  --bin /path/to/codanna \
  --config /path/to/settings.toml \
  --queries /path/to/queries.v1.jsonl \
  --out /tmp/rerank-labeling \
  --limit 30 \
  --cwd /path/to/workspace
```

Outputs:

- `candidates.raw.jsonl`
- `candidates.jsonl` (deduped by `query_id+chunk_id`)
- `qrels.todo.jsonl` (fill `grade`)

## 3) Qrels labeling

Fill grades in `qrels.todo.jsonl`:

- `2`: primary correct chunk
- `1`: helpful secondary chunk
- `0`: irrelevant

Then export final qrels:

```bash
jq -c 'select(.grade != null) | {query_id,chunk_id,grade}' /tmp/rerank-labeling/qrels.todo.jsonl > /tmp/rerank-labeling/qrels.v1.jsonl
```

Use `qrels.template.jsonl` as a format reference.

## 4) Validate dataset

```bash
python3 benchmarks/rerank/validate_qrels.py \
  --queries /path/to/queries.v1.jsonl \
  --qrels /path/to/qrels.v1.jsonl
```

Validation requires:

- every `query_id` exists
- `grade` in `0..2`
- no duplicate `query_id+chunk_id`
- each query has at least one positive label (`grade>0`)

## 5) Run benchmark (approval-gated)

`run_stage1_benchmark.sh` validates first, then waits for explicit `RUN` confirmation:

```bash
benchmarks/rerank/run_stage1_benchmark.sh \
  /path/to/codanna \
  /path/to/settings.toml \
  /path/to/queries.v1.jsonl \
  /path/to/qrels.v1.jsonl \
  /tmp/codanna-rerank-real \
  benchmarks/rerank/profiles.stage1.toml
```

Default runner flags:

- `--query-timeout-ms 12000`
- `--checkpoint-every 1`
- `--cold-runs 1`
- `--warm-runs 3`
- `--limit 10`

During execution, benchmark writes partial outputs so long runs are observable:

- `summary.partial.json`
- `per_query.partial.json`

## 6) Profile matrix

`profiles.stage1.toml` defines Jina sweep profiles.
`baseline_current_default` is auto-included by `codanna benchmark-rerank`.

## 7) One-command static INT8 pipeline (Jina v1)

Use this when you want to test static INT8 against current `jina_v1_tn30_heur_off`
with the same graded benchmark set.

Script:

- `benchmarks/rerank/run_static_int8_pipeline.sh`

What it does:

1. validates qrels
2. builds static INT8 model from `candidates.raw.jsonl` calibration pairs
3. runs smoke check (`semantic_search_chunks`)
4. benchmarks `jina_v1_tn30_heur_off` vs `jina_v1_static_int8_tn30_heur_off`
5. writes decision gate report

### Fast mode (recommended first)

```bash
benchmarks/rerank/run_static_int8_pipeline.sh --mode fast
```

- uses 500 calibration pairs
- lower runtime, good first signal

### Prod mode

```bash
benchmarks/rerank/run_static_int8_pipeline.sh --mode prod
```

- uses 1000 calibration pairs
- slower, more stable quantization signal

### Useful overrides

```bash
benchmarks/rerank/run_static_int8_pipeline.sh \
  --mode fast \
  --model-src ~/.codanna/models/models--jinaai--jina-reranker-v1-turbo-en/snapshots/<hash> \
  --config /tmp/codanna-rerank-realdata/settings.bench.toml \
  --queries /tmp/codanna-rerank-realdata/queries.v1.jsonl \
  --qrels /tmp/codanna-rerank-realdata/qrels.v1.jsonl \
  --calibration-jsonl /tmp/codanna-rerank-realdata/candidates.raw.jsonl \
  --calibration-method minmax \
  --quant-max-length 512 \
  --out-root /tmp/codanna-static-int8
```

Notes:

- If `--model-src` is omitted, script auto-resolves Jina v1 FP32 snapshot cache first.
- For low-memory environments, start with `--calibration-method minmax --quant-max-length 512`.

Outputs per run:

- `<run_dir>/model_static_int8`
- `<run_dir>/benchmark/summary_table.md`
- `<run_dir>/benchmark/summary.json`
- `<run_dir>/decision_gate.md`

Decision thresholds in `decision_gate.md`:

- Hit@1 drop <= 0.01
- MRR@10 drop <= 0.02
- nDCG@10 drop <= 0.02
- warm p95 improvement >= 20%
- timeout queries must not increase

## 8) Dynamic runtime-tuning sweep (Jina v1)

Use this when you want to tune runtime knobs while keeping the same model:

- `runtime_tuning_enabled`
- `batch_size`
- `warmup_pairs`

Default matrix:

- `benchmarks/rerank/profiles.dynamic_runtime_sweep.toml`

Runner:

```bash
benchmarks/rerank/run_dynamic_runtime_pipeline.sh \
  --out /tmp/codanna-dynamic-runtime
```

Outputs:

- `/tmp/codanna-dynamic-runtime/summary_table.md`
- `/tmp/codanna-dynamic-runtime/summary.json`
- `/tmp/codanna-dynamic-runtime/per_query.json`
- `/tmp/codanna-dynamic-runtime/decision.md`

## 9) Compare track-1 vs track-2

Generate one markdown table comparing:

- base (`jina_v1_tn30_heur_off_base` if present)
- best dynamic runtime-tuned profile
- static INT8 profile

```bash
python3 benchmarks/rerank/compare_track_results.py \
  --dynamic-summary /tmp/codanna-dynamic-runtime/summary.json \
  --static-summary /tmp/codanna-static-int8/run-<ts>/benchmark/summary.json \
  --out /tmp/codanna-rerank-final-decision.md
```

## 10) Prefilter v1 sweep vs current Jina TN30

Use this when you want to reduce reranker cost without changing model:

- keep baseline profile: `jina_tn30_heur_off`
- compare safe/aggressive prefilter settings

Matrix:

- `benchmarks/rerank/profiles.prefilter_v1_vs_base.toml`

Example:

```bash
codanna -c /tmp/codanna-rerank-realdata/settings.bench_baseline_off_tmpindex.toml benchmark-rerank \
  --queries /tmp/codanna-rerank-realdata/queries.v1x2.jsonl \
  --qrels /tmp/codanna-rerank-realdata/qrels.v1x2.jsonl \
  --profiles benchmarks/rerank/profiles.prefilter_v1_vs_base.toml \
  --out /tmp/codanna-prefilter-v1-30q \
  --cold-runs 1 \
  --warm-runs 2 \
  --limit 10 \
  --query-timeout-ms 12000 \
  --checkpoint-every 1 \
  --skip-warm-on-timeout true
```

## 11) ColBERT late-interaction planning docs

Implementation is not in this pass, but design references are ready:

- `docs/architecture/rerank_colbert_li_plan.md`
- `benchmarks/rerank/profiles/colbert_li_tn30.md`
- `benchmarks/rerank/profiles/colbert_li_tn30_ce_guard.md`
