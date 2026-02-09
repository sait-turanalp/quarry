# `colbert_li_tn30_ce_guard`

## Goal
Keep most of ColBERT latency benefits while protecting quality on hard/ambiguous queries.

## Runtime model
- Primary scorer: ColBERT late interaction.
- Guard: if uncertainty condition is met, run cross-encoder on small top-K set.
- Final ranking uses guarded result when guard triggers.

## Guard control surface
- `ce_guard_enabled`
- `ce_guard_top_k`
- `ce_guard_margin_threshold`
- optional confidence features (score spread, dual-source, etc.)

## Evaluation position
- Compare against:
  - `jina_tn30_heur_off` (quality baseline)
  - `colbert_li_tn30` (latency baseline)
- Promote only if quality gate passes and warm p95 remains meaningfully improved.
