# `colbert_li_tn30`

## Goal
Replace expensive cross-encoder rerank with late interaction scoring for large latency gains.

## Runtime model
- Index-time: store token-level document/chunk embeddings.
- Query-time: encode query once.
- Score: MaxSim(query tokens, doc tokens).

## Expected trade-off
- Much lower rerank compute cost.
- Higher storage/memory footprint.
- Quality typically below cross-encoder, above simple bi-encoder ranking.

## Evaluation position
- Compare against `jina_tn30_heur_off`.
- Primary metrics: Hit@1, MRR@10, nDCG@10, warm p95.
