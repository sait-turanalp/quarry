# Codanna Code Chunk RAG Implementation Plan (Approved)

**Date:** 2026-02-08  
**Status:** APPROVED

## Summary
Bu plan, mevcut symbol-level aramayı bozmadan Codanna'ya code chunk-level RAG hattı ekler.  
Mevcut stack korunur: Tantivy + model2vec/semantic + RRF + reranker + confidence gate.

## Verified Baseline Facts
1. `semantic_search_docs` bugün symbol döndürüyor ve `hybrid_search` çağırıyor.
   - `src/mcp/mod.rs:1190`
   - `src/mcp/mod.rs:1214`
   - `src/indexing/facade.rs:1109`
2. `search_documents` ayrı bir yol ve ayrı `DocumentStore` kullanıyor.
   - `src/mcp/mod.rs:3113`
   - `src/documents/mod.rs:32`
3. `DocumentStore::search` bugün BM25+vector hybrid değil; query metni lexical recall'da kullanılmadan filtre+vector sıralaması yapıyor.
   - `src/documents/store.rs:1123`
   - `src/documents/store.rs:1167`
   - `src/documents/store.rs:1197`
4. `documents` embedding tarafı doğrudan `FastEmbedGenerator` ile bağlı; model2vec doğrudan destekli değil.
   - `src/documents/mod.rs:44`
   - `src/cli/commands/documents.rs:37`
   - `src/vector/embedding.rs:659`
5. `documents` CLI tarafında dimension sabit `384` açılıyor, model boyutu ile mismatch riski var.
   - `src/cli/commands/documents.rs:29`
6. Kod indexing pipeline'ı symbol range/body/scope bilgisini zaten çıkarıyor; ikinci tree-sitter setup zorunlu değil.
   - `src/indexing/pipeline/types.rs:23`
   - `src/indexing/pipeline/stages/parse.rs:175`
7. Reranker + confidence gate bugün symbol `hybrid_search` path'inde çalışıyor.
   - `src/indexing/facade.rs:1153`
   - `src/indexing/facade.rs:1227`

## Final Decisions
1. `semantic_search_docs` mevcut davranışı korunur (backward-compatible).
2. Yeni güçlü code-RAG hattı `semantic_search_chunks` olarak eklenir.
3. V1 chunk birimi: `symbol-range chunk` (AST-aware, mevcut parser verisi reuse).
4. `documents` (md/txt) sistemi ayrı kalır; code chunk index `.codanna/index/code_chunks/` altında tutulur.
5. Chunk retrieval zorunlu pipeline: `BM25 + vector + RRF + reranker + chunk confidence gate`.
6. `rerank_timeout_ms` default değeri `1000` ms olur.

## Implementation Plan
1. Config yüzeyini ekle (`[chunk_search]`).
   - Dosya: `src/config.rs`
   - Alanlar:
     - `enabled`
     - `top_k_bm25`
     - `top_k_vector`
     - `top_k_fused`
     - `rerank_top_n`
     - `rerank_timeout_ms` (default: `1000`)
     - `confidence_gate_enabled`
     - `confidence_gate_min_top1_prob`
     - `confidence_gate_min_rrf`
     - `confidence_gate_require_dual_source`
     - `boost_chunk_type_function`
     - `boost_chunk_type_class`
     - `boost_path_src`
     - `boost_path_test`
   - Default: açık ama muhafazakar.

2. Code chunk metadata index'i ekle.
   - Yeni modüller:
     - `src/chunks/mod.rs`
     - `src/chunks/schema.rs`
     - `src/chunks/store.rs`
   - Schema alanları:
     - `doc_type`
     - `chunk_id`
     - `symbol_id`
     - `file_path`
     - `language`
     - `chunk_type`
     - `parent_scope`
     - `line_start`
     - `line_end`
     - `content`
     - `signature`
     - `doc_comment`
     - `indexed_at`
     - `file_hash`
   - Persist path: `.codanna/index/code_chunks/tantivy`
   - V1 kimlik kararı: `chunk_id == symbol_id`

3. Chunk vector index'i ekle (ayrı semantic storage).
   - Path: `.codanna/index/code_chunks/semantic`
   - Model: `semantic_search.model` ile aynı
   - Gerekli düzeltme:
     - Chunk embedding generator model2vec desteklemeli
     - fastembed fallback korunmalı
     - `documents` tarafındaki sabit `384` dimension riski giderilmeli

4. Chunk üretim akışı (incremental, ikinci parser yok).
   - Entegrasyon noktası: `IndexFacade` indexleme sonrası
   - Algoritma:
     - `get_file_info(path)` ile file hash al
     - cache ile karşılaştır
     - değişen dosyada eski chunkları sil
     - `find_symbols_by_file(file_id)` ile yeni chunkları üret/yaz
   - Chunk text formatı:
     - `# <file>`
     - `# Scope: <parent_scope>`
     - `# <kind> <signature_or_name>`
     - `<doc_comment>`
     - `<body/snippet>`
   - Line range: symbol range (`start_line`, `end_line`)

5. Chunk retrieval pipeline'ını uygula.
   - Yeni facade metodu:
     - `hybrid_chunk_search(query, limit, lang) -> Vec<ChunkSearchResult>`
   - Adımlar:
     - BM25 recall (field boost: `chunk_type`, `signature`, `doc_comment`, `content`, `file_path`)
     - vector recall
     - RRF
     - rerank
     - confidence gate
   - Rerank input:
     - domain-aware metin (`file`, `scope`, `kind`, `signature`, `doc`, `snippet`)
   - Çıktı:
     - `chunk_id`
     - `symbol_id`
     - `file_path`
     - `line_start`
     - `line_end`
     - `snippet`
     - `parent_scope`
     - `language`
     - `score`

6. MCP/CLI yüzeyini ekle, BC koru.
   - Dosyalar:
     - `src/mcp/mod.rs`
     - `src/cli/commands/mcp.rs`
     - `src/cli/args.rs`
   - Yeni tool: `semantic_search_chunks`
   - `semantic_search_docs` aynı kalır (symbol)
   - Opsiyonel alias: `semantic_search_code` -> symbol path
   - Tool açıklamaları net ayrılır:
     - code-symbol
     - code-chunk
     - docs-markdown

7. Unified arama tool'u ekle (symbol+chunk birlikte).
   - Yeni tool: `semantic_search_unified`
   - Birleşim:
     - symbol ve chunk sonuçları tip etiketli döner (`result_type = symbol|chunk`)
     - normalize + RRF birleşimi uygulanır
   - Amaç:
     - navigation + explanation kalitesini tek sorguda artırmak

8. Observability ve güvenlik.
   - Log metrikleri:
     - `bm25_ms`, `vector_ms`, `rrf_ms`, `rerank_ms`, `total_ms`
     - `candidates_per_stage`
     - `gate_drop_count`
   - Timeout fallback zorunlu
   - Chunk confidence gate fail olursa boş dönüş + düşük güven mesajı

9. Test ve acceptance.
   - Unit:
     - chunk text builder
     - chunk BM25 boosts
     - chunk confidence gate
     - cache/incremental hash
   - Integration:
     - `semantic_search_chunks` için 30-50 gerçek query seti
   - Karşılaştırma:
     - symbol-only vs chunk-only vs unified
   - Kabul:
     - understanding query'lerde Recall@10 ve nDCG artışı
     - no-result oranı kontrolü
     - warm query p95 hedefi (rerank açık): `<= 800ms`

## Known Limitations - V1
1. Module-level serbest yorum blokları (module comment) bağımsız chunk olarak aranmaz.
2. Konfigürasyon dosyaları (`*.toml`, `*.yaml`, `*.yml`, `*.env`, `*.json`) code chunk hattına dahil değildir.
3. Fonksiyonlar arası boşluk/geçiş metinleri (symbol dışı free-text bölgeler) chunk olarak aranmaz.

## Assumptions and Defaults
1. V1 chunk birimi symbol-range'tir; dosya-window chunk V2.
2. V1'de `chunk_id == symbol_id`.
3. `semantic_search_docs` semantiği korunur; kırıcı rename yok.
4. `search_documents` (md/txt) hattı ayrı kalır, code chunk ile karışmaz.

## Rollout Order
1. Config + chunk store şeması
2. Chunk üretim + incremental
3. Chunk hybrid retrieval + rerank + gate
4. MCP/CLI tool ekleri + BC
5. Unified search
6. Evaluation ve tuning
