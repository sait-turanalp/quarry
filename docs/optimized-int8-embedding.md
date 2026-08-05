# Optimized Int8 Static Embedding Engine

## Problem

Quarry bir CLI tool — kullanicinin makinesinde IDE, tarayici ve diger uygulamalarla birlikte calisiyor. Embedding modeli (`potion-retrieval-32M-int8`) diskten 32MB int8 veri yukleniyor, ama eskiden kullandigimiz `model2vec-rs` crate'i bu veriyi **yukleme aninda f32'ye ceviriyordu**. Sonuc: bellekte 123MB'lik bir tablo, 4x gereksiz bellek kullanimi, ve daha kotu CPU cache locality.

## Cozum

Int8 veriyi int8 olarak bellekte tutmak. Hesaplama aninda sadece kullanilan token satirlarini i32'ye genisletip, sonunda f32'ye donusturmek. Sonuc ayni (cosine > 0.999), bellek 4x az.

```
Eski (model2vec-rs):  disk (i8) → yukle → f32'ye cevir → 123MB bellekte tut → f32 arithmetic
Yeni (bizim motor):   disk (i8) → yukle → i8 olarak tut → 31MB bellekte tut → i8→i32→f32 (sadece sonuc)
```

`model2vec-rs` dependency tamamen kaldirildi. `OptimizedStaticModel` artik model2vec backend'inin tek motoru.

## Dosya Bazinda Degisiklikler

### `src/semantic/static_model.rs` (ana motor)

Tum is burada yapiliyor. ~345 satir.

**Struct:**
```rust
pub struct OptimizedStaticModel {
    tokenizer: Tokenizer,        // WordPiece tokenizer (63091 vocab)
    embeddings_i8: Vec<i8>,      // Flat embedding tablo: [vocab_size * dim]
    dim: usize,                  // 512
    vocab_size: usize,           // 63091
    normalize: bool,             // L2 normalize (true)
    median_token_length: usize,  // Char-level truncation icin
    unk_token_id: Option<u32>,   // Bilinmeyen token ID (1)
}
```

**`resolve_model_path(model_path)`** — Model dizin cozumleme:
- Oncelik sirasi:
  1. `model_path` zaten `model.safetensors` iceren bir dizinse → dogrudan kullan
  2. `~/.quarry/models/{name}-int8/` dene (tercih edilen int8 variant)
  3. `~/.quarry/models/{name}/` dene
- HuggingFace identifiers icin (`minishlab/potion-retrieval-32M`), repo adini (`potion-retrieval-32M`) cikarir
- `model2vec:` prefix'i otomatik olarak strip edilir

**`from_local(model_path)`** — Model yukleme:
- `resolve_model_path()` ile dizin cozumler
- `model.safetensors` dosyasindaki `embeddings` tensor'u I8 dtype olarak dogrudan `Vec<i8>`'e kopyalar (f32'ye cevirme yok)
- `tokenizer.json`'dan WordPiece tokenizer yukler
- `config.json`'dan `normalize` flag'i okur
- Sonuc: 31MB embedding tablo (123MB yerine)

**`encode_single(text)`** — Tek text encode:
1. Char-level truncation (median_token_length * max_tokens)
2. `tokenizer.encode_fast()` ile tokenize
3. `pool_ids_i8()` ile embedding hesapla

**`encode_batch(texts, max_length, batch_size)`** — Sequential batch:
- Batch tokenization (`encode_batch_fast`) + sequential pooling
- Kucuk batch'ler icin veya rayon uygun degilse kullanilir

**`encode_batch_parallel(texts, max_length)`** — Rayon paralel batch:
- Her text bagimsiz olarak rayon thread'lerinde tokenize + pool edilir
- `pool.rs`'deki `embed_parallel` ve `chunks/mod.rs`'deki `encode()` bu methodu cagiriyor
- batch_100 benchmark'ta 3x hizlanma (eski model2vec-rs'e gore)

**`pool_ids_i8(ids)` — Hot path (performans-kritik):**
```
Her token ID icin:
  embeddings_i8[row * 512 .. (row+1) * 512] satirini al (i8)
  512 boyutlu i32 accumulator'a topla (overflow-safe, SIMD-friendly)

Son adim:
  i32 → f32 donusumu (mean division)
  L2 normalization
```
Bu loop LLVM tarafindan otomatik olarak NEON (Apple Silicon) veya SSE/AVX (x86) instruction'larina vektorize ediliyor. 512 sabit boyutlu loop, derleyici icin ideal.

### `src/semantic/mod.rs` — Modul export

```rust
pub mod static_model;
pub use static_model::OptimizedStaticModel;
```

### `src/semantic/pool.rs` — Embedding pool entegrasyonu

`ModelBackendInstance` enum'daki variant'lar:
```rust
enum ModelBackendInstance {
    Fastembed(TextEmbedding),
    Optimized(OptimizedStaticModel),
}
```

**Mantik:** Pool, embedding model instance'larini yonetir. Model2vec backend icin pool_size=1 (thread-safe). `embed_parallel` methodu batch'leri islerkken `Optimized` variant icin `encode_batch_parallel()` cagirir — rayon ile paralel.

### `src/semantic/simple.rs` — Query-time embedding

`QueryEmbeddingModel` enum'da `Optimized(OptimizedStaticModel)` variant. Kullanicinin MCP uzerinden semantic search yaptiginda query metninin embed edilmesi icin kullanilir.

**Mantik:** Kullanici `semantic_search_with_context query:"parse json"` yaptiginda, "parse json" metni bu model ile embed edilir ve vector DB'de benzer symbol'ler aranir.

### `src/chunks/mod.rs` — Document recall backend

`ActiveModel` enum'da `Optimized(OptimizedStaticModel)` variant. Dokuman chunk'larinin embed edilmesi icin kullanilir (RAG pipeline). `encode()` methodu `encode_batch_parallel()` cagirir.

### `src/indexing/facade.rs` — Facade API

`EmbeddingPool::new()`, `ActiveRecallBackend::new()`, ve `SimpleSemanticSearch::from_model_name()` — hepsi Model2vec backend icin `OptimizedStaticModel` kullanir, flag gerekmiyor.

### `Cargo.toml` — Dependencies

```toml
safetensors = "0.5"   # Safetensors dosya okuma
tokenizers = "0.21"   # HuggingFace tokenizer
```

`model2vec-rs` dependency kaldirildi. `safetensors` ve `tokenizers` dogrudan dependency olarak eklendi.

### `benches/static_embed_bench.rs` — Benchmark

Criterion benchmark ile OptimizedStaticModel performans olcumu:

| Benchmark | Sonuc |
|-----------|-------|
| single_short (20 char) | ~6.5 us |
| single_long (2000 char) | ~358 us |
| batch_100_sequential | ~700 us |
| batch_100_rayon | ~497 us |

Eski model2vec-rs ile karsilastirma (referans):

| Benchmark | model2vec-rs | Optimized+Rayon | Hizlanma |
|-----------|-------------|-----------------|----------|
| single_short | 8.27 us | 6.47 us | 1.28x |
| single_long | 511.9 us | 358.5 us | 1.43x |
| batch_100 | 1.508 ms | 497.3 us | 3.03x |

Bellek karsilastirmasi:
- model2vec-rs RSS: 199.9 MB
- Optimized RSS: 106.4 MB
- Tasarruf: 93.5 MB (%47 azalma)

## Throughput

| Motor | Symbol/saniye (batch_100) |
|-------|--------------------------|
| model2vec-rs (eski) | ~66,300 |
| Optimized+Rayon (guncel) | ~201,000 |

## Model Path Resolution

HuggingFace model isimleri (`minishlab/potion-retrieval-32M`) dogrudan dizin path'i degildir. `resolve_model_path()` bu isimleri lokal dizinlere cozumler:

```
"minishlab/potion-retrieval-32M"
  → rsplit('/') → "potion-retrieval-32M"
  → ~/.quarry/models/potion-retrieval-32M-int8/  (varsa, tercih)
  → ~/.quarry/models/potion-retrieval-32M/        (fallback)
```

Default model config'de `minishlab/potion-retrieval-32M` olarak tanimli. Bu otomatik olarak `~/.quarry/models/potion-retrieval-32M-int8/` dizinine cozumlenir.

## Neden Bu Yaklasim?

1. **CLI tool icin bellek onemli** — kullanicinin makinesinde diger uygulamalarla birlikte calisiyor, 93MB tasarruf gercek dunya etkisi yapar
2. **Kalite kaybi yok** — ayni model, ayni sonuc (cosine > 0.999), sadece runtime temsil farki
3. **Autovectorization yeterli** — explicit SIMD intrinsics yazmadan LLVM'in autovectorize etmesine guveniyoruz, platform bagimsiz
4. **Rayon batch sadece gerekli yerde** — single query'de overhead, batch indeksleme'de kazanc
5. **model2vec-rs kaldirildi** — tek motor, tek code path, bakim kolayligi
