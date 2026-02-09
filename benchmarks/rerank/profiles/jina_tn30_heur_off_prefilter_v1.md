# `jina_tn30_heur_off_prefilter_v1`

## Amaç
`jina_tn30_heur_off` kalitesini koruyup reranker latency'sini azaltmak.

## Prefilter Mekanizması
Prefilter, cross-encoder rerank'e girmeden aday listesini kontrol eder:

1. `prefilter_enabled = true` ise aday listesi değerlendirilir.
2. Hedef aday sayısı `prefilter_target_top_n` (safe profilde 24).
3. Eğer `top_score - cutoff_score < prefilter_small_gap_threshold` ise kırpma iptal edilir ve tam liste rerank edilir (`prefilter_fallback_on_small_gap = true`).
4. Kırpma yapılırsa kuyruktan BM25+vector ortak adaylardan `prefilter_dual_source_tail_keep` kadar geri eklenir.

Bu sayede kolay sorgularda daha az pair rerank edilir, belirsiz sorgularda kalite için full sete geri dönülür.

## Profil Farkları (vs `jina_tn30_heur_off`)
Ortak kalanlar:
- model: `JINARerankerV1TurboEn`
- `top_n = 30`
- `max_length = 1024`
- `post_rerank_heuristics_enabled = false`

Fark yaratanlar:
- `jina_tn30_heur_off`: `prefilter_enabled = false`
- `jina_tn30_heur_off_prefilter_safe_v1`:
  - `prefilter_enabled = true`
  - `prefilter_target_top_n = 24`
  - `prefilter_fallback_on_small_gap = true`
  - `prefilter_small_gap_threshold = 0.015`
  - `prefilter_dual_source_tail_keep = 4`
- `jina_tn30_heur_off_prefilter_aggressive_v1`:
  - `prefilter_enabled = true`
  - `prefilter_target_top_n = 18`
  - `prefilter_fallback_on_small_gap = true`
  - `prefilter_small_gap_threshold = 0.010`
  - `prefilter_dual_source_tail_keep = 2`

## 30 Query Benchmark (2026-02-09)
Kaynak:
- `/tmp/codanna-prefilter-v1-30q/summary_table.md`
- `/tmp/codanna-prefilter-v1-30q/summary.json`

| Profil | Hit@1 | MRR@10 | nDCG@10 | Warm p50 | Warm p95 | TimeoutQ |
|---|---:|---:|---:|---:|---:|---:|
| `jina_tn30_heur_off` | 0.200 | 0.228 | 0.197 | 2839 ms | 5596 ms | 0 |
| `jina_tn30_heur_off_prefilter_safe_v1` | 0.200 | 0.228 | 0.197 | 3157 ms | 4426 ms | 0 |
| `jina_tn30_heur_off_prefilter_aggressive_v1` | 0.200 | 0.228 | 0.197 | 3174 ms | 4737 ms | 0 |

Delta vs `jina_tn30_heur_off`:
- `prefilter_safe_v1`: kalite aynı, warm p95 `+20.9%` daha iyi
- `prefilter_aggressive_v1`: kalite aynı, warm p95 `+15.4%` daha iyi
- Her iki profilde timeout artışı yok

## Karar Notu
- Tail latency odaklı en iyi profil: `jina_tn30_heur_off_prefilter_safe_v1`
- Varsayılanı hemen değiştirmeden, bir sonraki geniş benchmark turunda (daha büyük query seti) tekrar doğrulanmalı.
