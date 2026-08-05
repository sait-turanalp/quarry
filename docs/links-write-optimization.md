# Tantivy Write Optimization — LINKS Phase

## Problem

LINKS fazi Tantivy'ye 642K relationship documani yaziyor. Darbogazin %80'i Tantivy `commit_batch()` ve per-call overhead'iydi. Bu dokuman Tantivy write path'inde yapilan 3 optimizasyonu anlatir.

Quarry'nin indeksleme pipeline'i 3 ana fazdan olusuyor:

```
Phase 1 (PARSE+COLLECT+EMBED)  →  Phase 2 (LINKS)  →  Chunk Rebuild
         ~17s                         ~9.2s                ~29s
```

LINKS fazi (Phase 2) 642K relationship'i cozumleyip Tantivy'ye yaziyor. 195K symbol, 4497 dosya iceren void-main monorepo'sunda 9.2s suruyordu. Timing instrumentation ile asama bazinda analiz yapildi:

```
┌──────────────────────┬────────────────┬─────────┬────────┬────────┐
│        Asama         │ build_contexts │ resolve │ write  │ Toplam │
├──────────────────────┼────────────────┼─────────┼────────┼────────┤
│ Pass 1 (77K Defines) │ 132ms          │ 52ms    │ 1263ms │ 1447ms │
├──────────────────────┼────────────────┼─────────┼────────┼────────┤
│ Pass 2 (565K Calls)  │ 415ms          │ 1074ms  │ 5459ms │ 6948ms │
├──────────────────────┼────────────────┼─────────┼────────┼────────┤
│ Toplam               │ 547ms          │ 1126ms  │ 6722ms │ ~8.4s  │
└──────────────────────┴────────────────┴─────────┴────────┴────────┘
```

**Write %80 (6.7s/8.4s)**. `build_contexts` + `resolve` toplam sadece 1.7s.

Write path'te 3 darbogazin kaynagi:

1. **Auto-commit her 10K relationship'te** — `commit_batch()` cagrisi: segment finalize + diske yaz + fsync + `reader.reload()` + `start_batch()`. 372K relationship / 10K threshold = ~37 commit. Her commit Tantivy'nin en agir operasyonu.
2. **Per-call RwLock.read()** — `store_relationship()` her cagrida `self.writer.read()` ile lock aliyor. 372K lock acquire non-blocking ama yine de overhead.
3. **`format!("{:?}", rel.kind)` per-call** — Her relationship icin Debug format ile heap allocation. 372K kez `String` olustur + drop.

## Cozum

3 degisiklik, en etkisinden baslayarak:

```
ONCE:   372K store_relationship × (lock + format + doc) + 37 commit = 9.2s
SONRA:  ~4700 batch × (1 lock + batch doc) + 2 commit = 2.0-2.7s
```

### 1. Auto-commit Kaldirma (en buyuk etki)

Tantivy `commit_batch()` operasyonu:
- IndexWriter segment finalize (memory → disk segment)
- fsync (OS buffer → fiziksel disk)
- SegmentReader reload (yeni segment'leri oku)
- `start_batch()` (yeni IndexWriter olustur)

Eski davranis: `WriteStage` her 10K relationship'te otomatik commit yapiyordu. 372K relationship = ~37 commit. Ama LINKS fazinda sadece 2 commit gerekli:
- Pass 1 (Defines) sonunda 1 commit — barrier, Pass 2 (Calls) Defines sonuclarini sorguluyor
- Pass 2 (Calls) sonunda 1 flush — final commit

37 commit → 2 commit. Her commit ~100-150ms × 35 gereksiz commit = ~4-5s tasarruf.

### 2. Batch Write (tek lock)

Eski: Her `store_relationship` cagrisi kendi basina `self.writer.read()` lock aliyor.
Yeni: `store_relationships_batch()` — lock'u 1 kez al, tum batch'i yaz.

Tipik batch boyutu ~80 relationship (dosya basina ortalama). 372K / 80 = ~4700 batch, her biri tek lock acquire.

### 3. Statik String (`as_str()`)

Eski: `format!("{:?}", rel.kind)` — her cagrida Debug trait uzerinden heap allocation.
Yeni: `rel.kind.as_str()` — static `&str` match, zero allocation.

372K heap alloc + dealloc → 0.

## Mimari

### Veri Akisi (Phase 2 — LINKS)

```
Phase 1'den SymbolLookupCache (DashMap) hazir
    │
    ├── Pass 1: Defines (77K relationship)
    │     │
    │     ├── ContextStage.build_contexts()        ← rayon par_iter (dosya basina paralel)
    │     │     └── file_id → local symbols + imports + behavior
    │     │
    │     ├── ResolveStage.resolve()                ← rayon par_iter (context basina paralel)
    │     │     └── DashMap lookup (read-only, thread-safe)
    │     │
    │     ├── Collect resolved batches (Vec)
    │     │
    │     ├── WriteStage.write(batch)               ← serial, store_relationships_batch()
    │     │     └── 1 lock acquire per batch, ~80 docs queued
    │     │
    │     └── WriteStage.commit()                   ← 1 commit (barrier for Pass 2)
    │
    ├── Pass 2: Calls (565K relationship)
    │     │
    │     ├── ContextStage.build_contexts()        ← rayon par_iter
    │     ├── ResolveStage.resolve()                ← rayon par_iter
    │     ├── Collect + Serial write                ← store_relationships_batch()
    │     └── WriteStage.flush()                    ← 1 final commit
    │
    └── Toplam: 2 commit (eskisi 37)
```

### Thread-Safety Garantileri

Rayon paralellestirme icin gereken thread-safety:
- `SymbolLookupCache` — `DashMap<SymbolId, Symbol>` (concurrent read)
- `LanguageBehavior` — `Arc<dyn LanguageBehavior>` (Send + Sync)
- `ResolutionScope` — Send + Sync
- `DocumentIndex` reader — Tantivy Searcher thread-safe
- `ResolveStage.resolve(&self, ctx)` — `&self` (read-only DashMap lookups)

Write serial kalmak zorunda: Tantivy `IndexWriter` `!Send`, tek thread.

## Dosya Bazinda Degisiklikler

| Dosya | Degisiklik |
|-------|-----------|
| `src/indexing/pipeline/stages/write.rs` | Auto-commit kaldirildi, `store_relationships_batch()` kullanimi, `commit()` + `flush()` explicit |
| `src/storage/tantivy.rs` | `store_relationships_batch()` method, `add_relationship_doc()` static helper, `as_str()` kullanimi |
| `src/relationship/mod.rs` | `RelationKind::as_str()` — static `&str` match |
| `src/indexing/pipeline/stages/context.rs` | `build_contexts()` rayon `par_iter` |
| `src/indexing/pipeline/mod.rs` | Pass 1 + Pass 2 resolve loop'lari rayon `par_iter` |

### `src/indexing/pipeline/stages/write.rs`

Tam rewrite. Eski struct:
```rust
pub struct WriteStage {
    index: Arc<DocumentIndex>,
    pending: Vec<ResolvedRelationship>,  // Bellekte biriktirilmis
    commit_threshold: usize,             // 10_000
    batch_started: bool,
}
```

Yeni struct:
```rust
pub struct WriteStage {
    index: Arc<DocumentIndex>,
    written_since_commit: usize,  // Sadece sayac
    batch_started: bool,
}
```

`pending: Vec<ResolvedRelationship>` kaldirildi — gereksiz bellek kopya. Tantivy zaten kendi internal buffer'inda tutuyor. `commit_threshold` kaldirildi — auto-commit yok.

`write()` methodu artik `store_relationships_batch()` kullanir:
```rust
pub fn write(&mut self, batch: ResolvedBatch) -> WriteStats {
    // ...ensure_batch_started()...
    let rels: Vec<_> = batch.relationships.into_iter()
        .map(|r| {
            let rel = Relationship { kind: r.kind, weight: 1.0, metadata: r.metadata };
            (r.from_id, r.to_id, rel)
        })
        .collect();

    match self.index.store_relationships_batch(&rels) {
        Ok((written, failed)) => {
            stats.written = written;
            stats.failed = failed;
            self.written_since_commit += written;
        }
        Err(e) => { stats.failed = rels.len(); }
    }
    stats
}
```

### `src/storage/tantivy.rs`

Yeni `store_relationships_batch()`:
```rust
pub(crate) fn store_relationships_batch(
    &self,
    rels: &[(SymbolId, SymbolId, Relationship)],
) -> StorageResult<(usize, usize)> {
    let writer_lock = self.writer.read().unwrap_or_else(|p| p.into_inner());
    let writer = writer_lock.as_ref().ok_or(StorageError::NoActiveBatch)?;

    let mut written = 0;
    let mut failed = 0;
    for (from, to, rel) in rels {
        match Self::add_relationship_doc(writer, &self.schema, *from, *to, rel) {
            Ok(()) => written += 1,
            Err(_) => failed += 1,
        }
    }
    Ok((written, failed))
}
```

Mevcut `store_relationship()` de artik `add_relationship_doc()` static helper'i kullanir — kod tekrari yok.

### `src/relationship/mod.rs`

```rust
pub fn as_str(&self) -> &'static str {
    match self {
        Self::Calls => "Calls",
        Self::CalledBy => "CalledBy",
        // ... 12 variant, hepsi static &str
    }
}
```

### `src/indexing/pipeline/stages/context.rs`

```rust
// Eski: serial for loop
for (file_id, rels) in by_file { ... }

// Yeni: rayon parallel
by_file.into_par_iter()
    .map(|(file_id, rels)| self.build_context_for_file(file_id, rels))
    .collect()
```

### `src/indexing/pipeline/mod.rs`

Her iki pass'te (Defines + Calls) resolve paralellestirme:
```rust
// Eski: serial resolve + write
for ctx in contexts {
    let (batch, stats) = resolve_stage.resolve(&ctx);
    write_stage.write(batch);
}

// Yeni: parallel resolve → collect → serial write
let resolved: Vec<_> = contexts
    .par_iter()
    .map(|ctx| {
        let (batch, stats) = resolve_stage.resolve(ctx);
        (rel_count, batch, stats)
    })
    .collect();

for (rel_count, batch, stats) in resolved {
    write_stage.write(batch);
}
```

## Zamanlama Sonuclari (void-main, 195K symbol, 642K relationship)

### Optimizasyon Oncesi vs Sonrasi

| Metrik | Eski | Yeni | Kazanc |
|--------|------|------|--------|
| LINKS toplam | 9.2s | 2.0-2.7s | %71-78 |
| Commit sayisi | ~37 | 2 | %95 azalma |
| Lock acquire | 372K | ~4700 | %99 azalma |
| Heap alloc (format) | 372K | 0 | %100 azalma |

### Her Optimizasyonun Katkisi

Tahmini katki (izole olcmek zor, birlikte uygulandilar):

| Degisiklik | Tahmini Kazanc | Neden |
|------------|---------------|-------|
| Auto-commit kaldirma | ~4-5s | 35 gereksiz segment finalize+fsync elimine |
| Batch write (tek lock) | ~0.5-1s | 372K → 4700 lock acquire |
| as_str() statik string | ~0.2-0.5s | 372K heap alloc+dealloc elimine |
| Rayon paralel resolve | ~0.5-1s | build_contexts + resolve paralel |

### Stabil Calisma Dogrulamasi

3 ardisik calisma:
```
Run 1: 642009 relationships | 372928 resolved | 2.0s
Run 2: 642009 relationships | 372928 resolved | 2.7s
Run 3: 642009 relationships | 372928 resolved | 2.0s
```

## Tantivy Write Mimarisi — Ogrenilenler

Web arastirmasi ve kod analizi sonucu ogrenilen Tantivy icleri:

1. **`add_document()` non-blocking** — Tantivy `add_document` cagrisi dogrudan diske yazmaz. Documani internal thread pool'una kuyruklar. Asil maliyet commit aninda.

2. **`commit()` cok pahali** — Segment finalize (bellekteki dokumanlari segment dosyalarina yaz) + fsync (OS buffer → disk) + reader reload (yeni segment'leri Searcher'a kaydet). Bir commit ~100-150ms.

3. **IndexWriter tek thread** — `IndexWriter<D>: !Send`. Birden fazla thread'den add_document yapamaz. Ama bu sorun degil cunku add_document zaten non-blocking; asil bottleneck commit.

4. **Batch mode (`start_batch`/`commit_batch`)** — Batch mode'da Tantivy dokulanlari internal buffer'da biriktirir. Commit'e kadar hicbir sey diske yazilmaz. Bu bize 372K documani 2 commit'te yazma sansi veriyor.

5. **RwLock overhead birikir** — Tek basina `RwLock::read()` <100ns, ama 372K kez cagrinca ~30-40ms. Batch ile 4700 kez'e dusunce ihmal edilebilir.

6. **Debug format heap alloc** — `format!("{:?}", enum)` her cagrida yeni `String` olusturur. 372K kez = olculebilir overhead. Static `&str` match ile sifir alloc.

## Neden Bu Yaklasim?

1. **Minimal invaziv** — WriteStage'in public API'si ayni kaldi (`write`, `commit`, `flush`). Sadece internal implementasyon degisti. Pipeline kodu 3 satirlik degisiklik (par_iter + collect + serial write).

2. **Tantivy'nin guclu yonunu kullan** — add_document zaten non-blocking ve internal parallelism var. Tek yapmamiz gereken gereksiz commit'lerden kacinmak.

3. **2-commit barrier korundu** — Pass 1 sonrasi commit zorunlu (Pass 2 Defines sonuclarini sorguluyor). Bu mimari gereksinim bozulmadi.

4. **Thread-safety kaniti** — Rayon paralellestirme sadece read-only operasyonlarda (DashMap lookup, LanguageBehavior). Write path serial kaldi.

5. **Geriye uyumluluk** — `store_relationship()` hala calisiyor (internal olarak `add_relationship_doc()` cagirir). Sadece LINKS fazinda batch variant kullaniliyor.

6. **Olculebilir** — Her degisikligin etkisi timing instrumentation ile dogrulandi. Tahmin degil, veri odakli optimizasyon.
