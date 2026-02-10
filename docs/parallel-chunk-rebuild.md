# Parallel Chunk Rebuild Pipeline

## Problem

Codanna'nin indeksleme pipeline'i 3 ana fazdan olusuyor:

```
Phase 1 (PARSE+COLLECT+EMBED)  →  Phase 2 (LINKS)  →  Chunk Rebuild  →  Save
         ~17s                         ~9s                  ~48s           ~0.1s
```

Chunk rebuild, pipeline bittikten sonra **tamamen sirali** calisiyordu. Ayni verileri tekrar okuyup, ayni embedding modelini tekrar baslatip, sonra 195K symbol icin snippet cikarip 420K chunk embed ediyordu. Toplam ~74s.

Sorunlar:
1. **Tantivy re-read:** 195K symbol'u Tantivy'den tekrar oku (~2-3s)
2. **Model re-init:** Embedding modelini sifirdan baslat (~1s)
3. **Kaynak dosya re-read:** 4497 dosyayi diskten tekrar oku (~1.5s, OS cache'ten)
4. **Sirali calisma:** Phase 2 (LINKS) 9s, chunk rebuild 48s — sirayla = 57s, ama overlap olabilir

## Cozum

Chunk rebuild'i Phase 2 ile **paralel** calistirmak. Pipeline'daki mevcut verileri (SymbolLookupCache, EmbeddingPool) yeniden kullanarak gereksiz I/O'yu elimine etmek.

```
ONCE:   Phase 1 (17s) → Phase 2 (9s) → Chunk Rebuild (48s) → Save = ~74s
SONRA:  Phase 1 (17s) → [Phase 2 (9s) || Chunk Rebuild (29s)] → Save = ~46s
```

## Mimari

### Veri Akisi

```
Pipeline Phase 1
    │
    ├── SymbolLookupCache (DashMap<SymbolId, Symbol>)
    │       │
    │       ├──→ Phase 2 (LINKS) thread    ← mevcut, degismedi
    │       │
    │       └──→ Chunk Rebuild thread       ← YENi, paralel
    │               │
    │               ├── cache.all_symbols() → Vec<Symbol> (0.0s, Tantivy re-read yok)
    │               ├── PoolRecallAdapter(Arc<EmbeddingPool>) (model re-init yok)
    │               ├── build_chunk_record (source_cache ile dosya oku)
    │               ├── append_mixed_chunks (module comments, gaps, config/doc)
    │               ├── append_flow_chunks (tree-sitter re-parse)
    │               ├── token_budget + dedup
    │               ├── embed (420K chunk, rayon paralel)
    │               └── write (compact JSON + Tantivy chunk index)
    │
    ├── Join Phase 2
    ├── Join Chunk Rebuild
    └── Save
```

### Yeni Bilesenler

**`ChunkRebuildConfig`** (`src/indexing/pipeline/mod.rs`):
```rust
pub struct ChunkRebuildConfig {
    pub index_base: PathBuf,
    pub settings: Arc<Settings>,
    pub workspace_root: Option<PathBuf>,
    pub embedding_pool: Arc<EmbeddingPool>,
    pub indexed_paths: Vec<PathBuf>,
}
```

Pipeline method'larina `chunk_config: Option<ChunkRebuildConfig>` parametresi eklendi. `Some` geldiginde paralel thread spawn edilir, `None` geldiginde eski sirali davranisa fallback.

**`PoolRecallAdapter`** (`src/chunks/mod.rs`):
```rust
pub struct PoolRecallAdapter {
    pool: Arc<EmbeddingPool>,
}

impl RecallBackend for PoolRecallAdapter {
    fn model_name(&self) -> &str { self.pool.model_name() }
    fn dimensions(&self) -> usize { self.pool.dimensions() }
    fn encode(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, IndexError> {
        Ok(self.pool.encode_texts(texts))
    }
}
```

Pipeline'in `EmbeddingPool`'unu `RecallBackend` trait'ine adapt eder. Yeni model instance olusturmak yerine mevcut pool'u yeniden kullanir.

**`SymbolLookupCache::all_symbols()`** (`src/indexing/pipeline/types.rs`):
```rust
pub fn all_symbols(&self) -> Vec<Symbol> {
    self.by_id.iter().map(|r| r.value().clone()).collect()
}
```

DashMap'ten tum symbol'leri cikarir. Tantivy'den tekrar okuma yerine in-memory cache'ten 0.0s'de 195K symbol.

**`EmbeddingPool::encode_texts()`** (`src/semantic/pool.rs`):
```rust
pub fn encode_texts(&self, texts: &[String]) -> Vec<Vec<f32>> {
    // SymbolId bookkeeping olmadan dogrudan text→embedding
    // Optimized backend icin encode_batch_parallel() kullanir
}
```

## Dosya Bazinda Degisiklikler

| Dosya | Degisiklik |
|-------|-----------|
| `src/indexing/pipeline/types.rs` | `all_symbols()` method |
| `src/semantic/pool.rs` | `encode_texts()` method |
| `src/chunks/mod.rs` | `PoolRecallAdapter` struct + compact JSON write + `split_text_file_chunks` bug fix |
| `src/indexing/pipeline/mod.rs` | `ChunkRebuildConfig` + `chunk_config` param (4 method) + paralel spawn |
| `src/indexing/facade.rs` | `build_chunk_config()` + pipeline'a config aktarimi |

## Bulunan Bug: `split_text_file_chunks` Sonsuz Dongu

Paralel rebuild'i void-main'de test ederken, `append_config_doc_chunks` asamasinda proses 300s+ takildiktan sonra asla bitmiyordu. Adim adim daraltma ile sorun `split_text_file_chunks` fonksiyonunda tespit edildi.

**Sebep:** `overlap_lines > 0` durumunda, eger overlap chunk boyutuna esit veya buyukse `start` asla ileri gidemiyordu. Eger ayni zamanda snippet `min_chunk_chars`'i karsilamiyorsa `out` buyumuyordu → sonsuz dongu.

```
Senaryo: 17 satirlik kucuk JSON dosyasi, overlap_lines=2

Iterasyon 1: start=0, end=17, snippet kisa → pushed, start = 17-2 = 15
Iterasyon 2: start=15, end=17 (2 satir), snippet < min_chunk_chars → NOT pushed
             start = 17 - min(2, 2) = 15  → AYNI START → sonsuz dongu
```

**Fix:** Tek satir, her iterasyonda `start` en az 1 ilerlemeli:
```rust
let prev_start = start;
start = end.saturating_sub(overlap_lines.min(end.saturating_sub(start)));
// Guarantee forward progress
if start <= prev_start {
    start = prev_start + 1;
}
```

Bu bug paralel mimariden bagimsiz — eski sirali path'te de vardi, ama void-main'in dosya dagilimiyla tetiklenmiyordu (sirali path farkli `indexed_paths` geciyordu). Paralel path'te walkdir ile bulunan kucuk JSON test fixture dosyalari bug'i tetikledi.

## Compact JSON Write

Chunk metadata yazimi `to_string_pretty` → `to_string` + `BufWriter` olarak degistirildi:

```rust
// Eski: ~5.8s (pretty-print 420K chunk)
let json = serde_json::to_string_pretty(&chunks)?;
std::fs::write(&path, json)?;

// Yeni: ~1.5s (compact + buffered)
let json = serde_json::to_string(&chunks)?;
let mut writer = std::io::BufWriter::new(std::fs::File::create(&path)?);
std::io::Write::write_all(&mut writer, json.as_bytes())?;
```

## Zamanlama Sonuclari (void-main, 195K symbol, 4497 dosya)

### Chunk Rebuild Asama Detaylari

| Asama | Sure |
|-------|------|
| Symbol extract (cache → Vec) | 0.0s |
| build_chunk_record (195K sym, 4497 dosya) | 1.5s |
| module_comment + inter-symbol gaps | 0.2s |
| config_doc_chunks (1335 dosya walkdir) | 0.5s |
| append_flow_chunks (tree-sitter re-parse) | 7.1s |
| token_budget + dedup (438K → 420K) | 0.5s |
| embed (420K chunk, rayon) | 10.2s (41K chunk/s) |
| save + write (semantic + JSON + Tantivy) | 7.3s |
| **Chunk Rebuild Toplam** | **29.1s** |

### Pipeline Toplam Karsilastirma

| Metrik | Eski (Sirali) | Yeni (Paralel) | Kazanc |
|--------|--------------|----------------|--------|
| Phase 1 (PARSE+EMBED) | ~17s | ~17s | — |
| Phase 2 (LINKS) | ~9s | ~9s | — |
| Chunk Rebuild | ~48s | ~29s | -19s (pool reuse + compact JSON) |
| LINKS + Chunks (sirali vs paralel) | 9 + 48 = 57s | max(9, 29) = 29s | -28s |
| **Toplam** | **~74s** | **~46s** | **~28s (%38)** |

### Kucuk/Orta Repo Sonuclari

| Repo | Symbol | Dosya | Chunk Rebuild | Not |
|------|--------|-------|---------------|-----|
| landing (14 dosya) | ~200 | 14 | 0.4s | Anlık |
| crush (Go, 1180 dosya) | ~8K | ~300 | 1.0s | LINKS (0.5s) ile paralel |
| void-main (monorepo) | 195K | 4497 | 29.1s | LINKS (9s) ile paralel |

## Neden Bu Yaklasim?

1. **Mevcut verileri yeniden kullan** — SymbolLookupCache ve EmbeddingPool pipeline'da zaten var. Tantivy re-read ve model re-init elimine edildi.
2. **Minimal invaziv** — Pipeline method'larina sadece `Option<ChunkRebuildConfig>` parametresi eklendi. `None` gecildiginde davranis degismez. Incremental/watcher path'ler etkilenmez.
3. **Thread-safe tasarim** — `Arc<SymbolLookupCache>` ve `Arc<EmbeddingPool>` ile paylasim. EmbeddingPool'un crossbeam channel pool'u zaten thread-safe (pool_size=1 Model2vec icin yeterli — rayon parallelism model icinde).
4. **Fallback garanti** — Chunk config olusturulamazsa (pool yok, chunk search disabled) eski sirali path otomatik calisir.
5. **Compact JSON** — Pretty-print gereksiz, chunk metadata insanlar tarafindan okunmuyor. %74 write hizlanmasi.
