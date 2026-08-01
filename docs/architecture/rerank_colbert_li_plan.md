# ColBERT Late Interaction + CE Guard Plan

## Objective
Introduce a late-interaction reranking backend to reduce reranking latency, with an optional
cross-encoder guard path to preserve quality on ambiguous queries.

## Scope
- Add backend abstraction for reranking (`cross_encoder` default, `colbert_li` planned).
- Add index-time storage for token-level chunk embeddings.
- Add query-time MaxSim scoring path.
- Add optional cross-encoder guard for uncertain cases.

## Proposed interfaces
- `reranking.backend = "cross_encoder" | "colbert_li"` (default: `cross_encoder`)
- `reranking.ce_guard_enabled = bool` (default: `false`)
- `reranking.ce_guard_top_k = usize` (default: `8`)
- `reranking.ce_guard_margin_threshold = f32` (default: `0.02`)

## Data flow
1. Index-time:
   - tokenize chunk text into model tokens
   - compute token embeddings
   - compress/store per-chunk token matrix (int8 or fp16)
2. Query-time:
   - encode query once
   - score candidates via MaxSim against stored chunk token matrices
   - if guard condition triggers, run cross-encoder on top-K and replace final order

## Storage considerations
- Size is approximately:
  - `num_chunks * avg_tokens_per_chunk * dim * bytes_per_value`
- Must support:
  - per-chunk retrieval by chunk id
  - versioning by model/hash
  - invalidation on index rebuild/model change

## Failure modes and fallbacks
- Missing/invalid token-embedding store:
  - fallback to current cross-encoder path
- Guard model unavailable/timeout:
  - return pure ColBERT ranking
- Model/version mismatch:
  - hard-disable `colbert_li` and log warning

## Quality and performance gates
- vs `jina_tn30_heur_off`:
  - `ΔHit@1 >= -0.01`
  - `ΔMRR@10 >= -0.01`
  - `ΔnDCG@10 >= -0.01`
  - timeout count not increased
- Performance target:
  - warm `p95` improvement >= 15%

## Benchmark matrix
- `jina_tn30_heur_off` (reference)
- `colbert_li_tn30`
- `colbert_li_tn30_ce_guard`

Protocol:
- 30-query set
- `cold-runs=1`, `warm-runs=2`, `limit=10`
- same qrels and timeout threshold
