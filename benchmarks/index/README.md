# Indexing A/B Benchmark (Force Rebuild)

This benchmark compares indexing runtime between two profiles:

- `index_force_baseline`: legacy behavior
- `index_force_opt_v1`: enables:
  - `indexing.chunk_incremental_rebuild_enabled = true`
  - `indexing.semantic_single_save_mode = true`
  - `chunk_search.rebuild_logging_verbose = true`

## Run

```bash
benchmarks/index/run_force_index_ab.sh \
  --bin /Users/sait/Documents/lut-app/codanna/target/release/codanna \
  --repo /Users/sait/Documents/opencode-dev \
  --config /Users/sait/Documents/opencode-dev/.codanna/settings.toml \
  --out /tmp/codanna-index-force-ab
```

Dry-run:

```bash
benchmarks/index/run_force_index_ab.sh \
  --bin /Users/sait/Documents/lut-app/codanna/target/release/codanna \
  --repo /Users/sait/Documents/opencode-dev \
  --config /Users/sait/Documents/opencode-dev/.codanna/settings.toml \
  --dry-run
```

## Outputs

- `summary.json`: machine-readable metrics per profile
- `summary_table.md`: markdown table with deltas vs baseline
- `<profile>.log`: raw command logs

## Metrics Collected

- `wall_s`: end-to-end command duration
- `pipeline_total_s`: Pipeline TRACE total (if available)
- `semantic_save_calls`: number of `index/semantic` save calls
- `chunk_semantic_save_s`: estimated code-chunk semantic save duration
- `chunk_rebuild_span_s`: estimated span from chunk semantic save start to chunk rebuild completion
