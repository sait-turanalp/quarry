# Test A New Reranker Model

This guide is for testing a new reranker model with the existing benchmark system.

## What stays fixed

- Same query set: `queries.v1.jsonl`
- Same qrels: `qrels.v1.jsonl`
- Same benchmark command and metrics
- Auto baseline: `baseline_current_default` is included automatically

## 1) Prepare model path

Model must be loadable as:

```text
custom:/absolute/path/to/model-dir
```

Model dir must include:

- `model.onnx` (or `onnx/model.onnx`)
- `tokenizer.json`
- `config.json`
- `special_tokens_map.json`
- `tokenizer_config.json`

## 2) Create profile file

Example: `/tmp/profiles.newmodel.toml`

```toml
[defaults]
model = "custom:/absolute/path/to/model-dir"
timeout_ms = 15000
max_length = 1024
rerank_score_normalization = "none"
top_k_vector = 100
top_k_bm25 = 100
top_k_fused = 50

[[profiles]]
name = "newmodel_tn20_heur_off"
top_n = 20
post_rerank_heuristics_enabled = false

[[profiles]]
name = "newmodel_tn30_heur_off"
top_n = 30
post_rerank_heuristics_enabled = false

[[profiles]]
name = "newmodel_tn40_heur_off"
top_n = 40
post_rerank_heuristics_enabled = false

[[profiles]]
name = "newmodel_tn30_heur_on"
top_n = 30
post_rerank_heuristics_enabled = true

[[profiles]]
name = "newmodel_tn40_heur_on"
top_n = 40
post_rerank_heuristics_enabled = true
```

## 3) Validate qrels

```bash
python3 benchmarks/rerank/validate_qrels.py \
  --queries /tmp/quarry-rerank-realdata/queries.v1.jsonl \
  --qrels /tmp/quarry-rerank-realdata/qrels.v1.jsonl
```

Expected: `status: OK`

## 4) Run benchmark

```bash
quarry -c /path/to/settings.toml benchmark-rerank \
  --queries /tmp/quarry-rerank-realdata/queries.v1.jsonl \
  --qrels /tmp/quarry-rerank-realdata/qrels.v1.jsonl \
  --profiles /tmp/profiles.newmodel.toml \
  --out /tmp/quarry-rerank-newmodel \
  --cold-runs 1 \
  --warm-runs 3 \
  --limit 10 \
  --query-timeout-ms 12000 \
  --checkpoint-every 1 \
  --skip-warm-on-timeout true
```

## 5) Read results

Main outputs:

- `/tmp/quarry-rerank-newmodel/summary_table.md`
- `/tmp/quarry-rerank-newmodel/summary.json`
- `/tmp/quarry-rerank-newmodel/per_query.json`
- `/tmp/quarry-rerank-newmodel/decision.md`

## 6) Decision rule (recommended)

Use this order:

1. No timeouts / init failures
2. Quality: `nDCG@10`, `MRR@10`, `Hit@1`, `Recall@10`
3. Latency: `warm p95`, then `warm p50`

If quality is close, pick lower `warm p95`.
If quality gap is large, keep better quality profile.
