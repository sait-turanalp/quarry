# Chunk RAG V2 — Uygulama Planı ve Gerçekleşen Durum

Tarih: 2026-02-08  
Durum: Uygulandı (core)

## Kapsam
Bu çalışma, mevcut symbol-level retrieval hattını bozmadan chunk-level retrieval kalitesini artırmak için şu üç ana hedefi uygular:

1. AST-aware flow chunk üretimi
2. Mixed chunk katmanını olgunlaştırma (module comment, inter-symbol gap, doc/config chunk)
3. model2vec runtime clamp değerini 4096'a çıkarma

## Net Kararlar
- Flow chunk dili: `typescript`, `javascript`, `python`, `rust`
- model2vec clamp üst sınırı: `4096`
- Embedding backend coupling yok: chunk indexleme `RecallBackend` trait'i üzerinden backend-agnostic

## Public Interface Değişiklikleri

### Parser API
`LanguageParser` trait:
```rust
fn find_flow_blocks(&mut self, _code: &str) -> Vec<FlowBlock> { Vec::new() }
```

Yeni tipler:
- `FlowKind`: `IfElse`, `TryCatch`, `Switch`, `Loop`, `CallChain`, `ErrorPath`
- `FlowBlock`: `kind`, `range`, `label`, `parent_symbol_name`

### Chunk Type Genişlemesi
Yeni `chunk_type` değerleri:
- `flow_if_else`
- `flow_try_catch`
- `flow_switch`
- `flow_loop`
- `flow_call_chain`
- `flow_error_path`

### Config (chunk_search)
Eklenen alanlar:
- `flow_chunk_enabled = true`
- `flow_chunk_languages = ["typescript","javascript","python","rust"]`
- `flow_chunk_max_per_symbol = 6`
- `chunk_token_target = 800`
- `chunk_token_max = 4096`
- `chunk_token_overlap = 96`

### Runtime Clamp
`effective_semantic_pool_config` içinde model2vec clamp:
- `max_chunk_tokens` üst sınırı `1024 -> 4096`

## Uygulanan Mimarî

### Faz 1 — Flow Extraction
- TS/JS/Python/Rust parser'larında `find_flow_blocks` implement edildi.
- `if/loop/try/switch/call-chain/error-path` blokları AST'den çıkarılıyor.

### Faz 2 — Chunk Builder Entegrasyonu
- `CodeChunkIndexer::rebuild_from_symbols` imzası genişletildi.
- Flow chunk üretimi, mixed chunk katmanı ve token budget akışı aynı rebuild içinde çalışıyor.
- `file + line_start + line_end + chunk_type` ile dedup eklendi.

### Faz 3 — Token-Aware Split/Merge
- `chunk_token_max` aşılırsa line-bazlı parçalara bölünüyor.
- `chunk_token_overlap` ile overlap korunuyor.
- Çok küçük segmentler hedefe uygun şekilde merge ediliyor.

### Faz 4 — Retrieval/Rerank Entegrasyonu
- Flow chunk türleri retrieval scoring'de implementation seviyesinde ağırlık alıyor.
- Mevcut tail-pruning/coherence/symbol-aware mantığı korunuyor.

### Faz 5 — model2vec Clamp
- Runtime clamp 4096'a yükseltildi.
- Clamp uygulanınca log üretimi korunuyor.

## Test Kapsamı

Eklenen/güncellenen testler:
- Parser flow extraction testleri (TS/JS/Python/Rust)
- `dedup_chunk_records` testi
- token split/merge davranış testi
- chunk search config varsayılanları testi (yeni alanlar dahil)
- chunk type weight testi (`flow_if_else` dahil)

## Known Limitations — V2
- Flow extraction şu an yalnızca `TS/JS/Python/Rust` için aktif.
- Token sayımı yaklaşık (`split_whitespace`) yöntemle yapılıyor; model-tokenizer tabanlı değil.
- Flow chunk sayısı `flow_chunk_max_per_symbol` ile sınırlandığı için bazı uzun fonksiyonlarda tüm akış parçaları indekslenmeyebilir.

## Not
Önceki V1 limitleri (module comment/config/inter-symbol gap chunk yok) bu çalışmada giderildi; mixed chunk katmanı aktif durumda.
