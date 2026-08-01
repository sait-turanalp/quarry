# Codanna Embedding Optimization Plan (Final)

## Summary
- Goal: Keep current hybrid retrieval architecture, reduce indexing cost, and keep query quality stable.
- Decision: Add `model2vec` as a **recall backend option** while preserving existing `fastembed` flow.
- Stability: Keep graceful fallback paths (`RRF` output when reranker fails/times out).

## Verified Baseline Facts
- Hybrid search is already implemented: `src/indexing/facade.rs` (`hybrid_search`, `rrf_merge`).
- Lazy reranker is already implemented in `IndexFacade`.
- Semantic indexing persists required artifacts in `SimpleSemanticSearch::save()`:
  - `segment_0.vec`
  - `binary_index.bin`
  - `embedded_hashes.json`
- Existing semantic pipeline already supports worker and in-process embedding modes.

## Implemented Changes
1. Config extensions
- Added semantic backend selector:
  - `semantic_search.backend = "fastembed" | "model2vec"`
- Added reranker timeout:
  - `reranking.timeout_ms` (default `500`)
- Updated config comments generation to be section-aware for semantic/reranking fields.

2. Model2Vec backend integration
- Added dependency: `model2vec-rs = "0.1.4"`.
- Integrated into embedding pool (`src/semantic/pool.rs`) with backend dispatch:
  - `fastembed` path unchanged.
  - `model2vec` path using `StaticModel::from_pretrained(...)`.
- Query-time semantic embedding updated in `src/semantic/simple.rs`:
  - Supports both fastembed and model2vec model initialization.
  - Keeps index/query embedding space aligned by using the same model name.

3. Worker + pipeline compatibility
- Worker config now carries backend selection (`src/semantic/worker.rs`).
- Pipeline worker bootstrap passes backend (`src/indexing/pipeline/mod.rs`).
- Facade pool initialization now passes backend (`src/indexing/facade.rs`).

4. Reranker timeout fallback
- `IndexFacade::hybrid_search()` reranking call now runs in a separate thread and waits with timeout.
- On timeout or failure, returns fused (`RRF`) candidates instead of blocking request path.

## Runtime Behavior
- `semantic_search.backend = "fastembed"`:
  - Existing behavior preserved.
- `semantic_search.backend = "model2vec"`:
  - Uses static embedding model for recall (faster, lower RAM).
  - Current runtime clamps keep resource usage bounded.

## Validation
1. Build/check/tests
- `cargo fmt`
- `cargo check`
- `cargo test --lib config::tests -- --nocapture`
- `cargo test --lib --no-run`

2. End-to-end smoke tests (done)
- Model2Vec backend:
  - `init --force`
  - set backend/model in `.codanna/settings.toml`
  - `index src --force`
  - verified semantic files exist:
    - `binary_index.bin`
    - `embedded_hashes.json`
    - `segment_0.vec`
- FastEmbed backend:
  - same flow with `AllMiniLML6V2`
  - verified semantic artifacts and successful indexing output.

## Notes
- Initial build failed once due disk exhaustion (`errno=28` during link); resolved with `cargo clean`.
- Plan intentionally keeps architecture incremental and backward-compatible while enabling faster backend selection.
