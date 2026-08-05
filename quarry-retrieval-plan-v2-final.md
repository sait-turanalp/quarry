0. Granite Embedding Entegrasyonu
  - granite-embedding-small-english-r2 (384d, fp16, 8K token)
  - Pooling: Model card/config.json'dan doğrulanacak (IBM Granite genelde mean pooling kullanır, CLS değil — yanlış pooling %10-15 kalite kaybı yaratır)
  - create_text_embedding() factory — built-in + user-defined model desteği
  - Tüm pipeline güncellemeleri (EmbeddingPool, SimpleSemanticSearch, IndexFacade, FastEmbedGenerator)
  - Varsayılan model: GraniteSmallEnglishR2
  - Model dizini: ~/.quarry/models/granite-small-r2/

  ---
  Adım 1: Body'yi Pipeline'a Taşı

  - RawSymbol'e body: Option<Box<str>> alanı ekle
  - PARSE stage'de range + dosya içeriği ile body text'i kes
  - Nested closure/lambda → parent fonksiyonun body'sine dahil, ayrı çıkarma
  - COLLECT stage'e body bilgisi ulaşsın

  Adım 2: Tüm Symbol'leri Embed Et

  - collect.rs:328 — if let Some(ref doc) = raw_sym.doc_comment filtresini kaldır
  - Doc comment'i olmayan symbol'ler de embed edilsin
  - Aşağıdaki birleştirme/skip kuralları uygulanacak (Adım 4'te detaylı)

  Adım 3: Context Enrichment

  Her chunk'a yapısal context header ekle:
  # src/services/user_service.rs
  # Scope: UserService
  # method get_user(id: u32) -> Option<User>

  Embedding text = context header + doc comment + signature + body

  Mevcut veriler:
  - file_path ✅
  - scope_context (ClassMember, Local{parent_name}) ✅
  - signature ✅
  - kind ✅
  - doc_comment ✅
  - body → Adım 1'de eklenecek
  - uses/calls → Phase 2'de çözülüyor, V1'de dahil etme

  Adım 4: AST-Aware Chunking

  3 Katmanlı Chunk Yapısı

  Katman 1 — Dosya Chunk'ı
  # src/services/user_service.rs
  # Module: crate::services::user_service
  # Imports: Database, Cache, User, AuthError
  # Defines: UserService (class), get_user, update_user, delete_user
  Body yok, sadece yapı haritası. "Bu dosya ne yapıyor?" sorgularını yakalar.

  Üretim noktası: COLLECT stage sonunda dosya bazlı aggregation.
  Bir dosyadaki tüm symbol'ler toplandıktan sonra, imports + symbol listesinden
  sentezlenir. Symbol bazlı değil, dosya bazlı bir meta-chunk.

  Katman 2 — Class/Struct Header Chunk'ı
  # src/services/user_service.rs
  # class UserService
  # Fields: db: Database, cache: Cache
  # Methods: constructor, get_user, update_user, delete_user

  class UserService {
    private db: Database;
    private cache: Cache;
    getUser(id: string): Promise<User> { ... }
    updateUser(id: string, data: Partial<User>): Promise<void> { ... }
  }
  Method signature'ları var, body'ler yok. Overlap bilinçli kabul — farklı sorgular farklı katmanlardan cevap alır.

  Üretim noktası: COLLECT stage'de class/struct/impl symbol'ü işlenirken,
  child method'ların signature'ları toplanarak sentezlenir.

  Katman 3 — Method/Function Chunk'ı
  # src/services/user_service.rs
  # Scope: UserService
  # method getUser(id: string): Promise<User>
  /// Fetches user from database, falls back to cache
  async getUser(id: string): Promise<User> {
    const cached = this.cache.get(id);
    if (cached) return cached;
    const user = await this.db.query('SELECT * FROM users WHERE id = ?', [id]);
    this.cache.set(id, user);
    return user;
  }

  Boyutlandırma Kuralları
  ┌────────────────────────────────────────────┬────────────────────────────────────────────────────────────┐
  │                   Durum                    │                          Strateji                          │
  ├────────────────────────────────────────────┼────────────────────────────────────────────────────────────┤
  │ Tek fonksiyon < 7.5K token                 │ Tek chunk (Katman 3)                                       │
  ├────────────────────────────────────────────┼────────────────────────────────────────────────────────────┤
  │ Küçük class < 4K token                     │ Tek chunk — header + tüm body'ler birlikte                 │
  ├────────────────────────────────────────────┼────────────────────────────────────────────────────────────┤
  │ Orta class 4K-7.5K token                   │ Header chunk + tüm method'lar tek chunk                    │
  ├────────────────────────────────────────────┼────────────────────────────────────────────────────────────┤
  │ Büyük class > 7.5K token                   │ Header chunk + her method ayrı chunk                       │
  ├────────────────────────────────────────────┼────────────────────────────────────────────────────────────┤
  │ Büyük fonksiyon > 7.5K token               │ İlk 3K + son 2K + context header (nadir durum)             │
  │                                            │ Ortadaki repetitive logic en az bilgi taşır                │
  ├────────────────────────────────────────────┼────────────────────────────────────────────────────────────┤
  │ Ardışık küçük birimler (const, type alias) │ Aynı scope'ta birleştir, max 2K token                      │
  ├────────────────────────────────────────────┼────────────────────────────────────────────────────────────┤
  │ Import blokları                            │ Dosya chunk'ına dahil, ayrı embed etme                     │
  ├────────────────────────────────────────────┼────────────────────────────────────────────────────────────┤
  │ Nested closure/lambda                      │ Parent fonksiyonun parçası, ayrı chunk değil               │
  ├────────────────────────────────────────────┼────────────────────────────────────────────────────────────┤
  │ Trait/Interface impl                       │ Kendi chunk'ı: # Implements: Display for User              │
  ├────────────────────────────────────────────┼────────────────────────────────────────────────────────────┤
  │ Re-export / barrel dosyaları               │ Kendi tanımladığı symbol yoksa → hiç embed etme            │
  ├────────────────────────────────────────────┼────────────────────────────────────────────────────────────┤
  │ Test dosyaları                             │ Default: skip. settings.toml'dan opsiyonel flag ile açılır │
  └────────────────────────────────────────────┴────────────────────────────────────────────────────────────┘
  Token Sayma

  tahmini_token = karakter_sayısı / 3.5

  Tahmini < 5K  → kesin sığar, kontrol etme
  Tahmini 5K-8K → gerçek tokenizer ile say, gerekirse kırp
  Tahmini > 8K  → kesin sığmaz, böl

  Güvenli max chunk boyutu: 7.5K token (500 token buffer — context header yer kaplıyor)

  Granite hard limit: 8192 token

  Adım 5: Hybrid Search (BM25 + Vector) — RRF

  - Tantivy (BM25) ve vector search sonuçlarını Reciprocal Rank Fusion ile birleştir
  - BM25: exact keyword match (fonksiyon adı, değişken adı)
  - Vector: semantik benzerlik (doğal dil sorgu)
  - ~50 satır implementasyon
  - Başlangıç: BM25 top20 + Vector top20 → RRF → top20
    (top50 fazla — BM25'in 30-50 arası sonuçları genelde gürültü, RRF skorunu düşürür)
  - Beklenen iyileştirme: %30-60 retrieval kalitesi artışı
  - Concurrency: BM25 blocking I/O, HNSW in-memory — tokio::spawn_blocking + tokio::join! ile paralel

  Adım 6: Lazy Embedding — Hash Bazlı Atlama

  - Dosya content_hash değişmediyse embedding'i yeniden üretme
  - Quarry'da content_hash zaten mevcut, embedding tarafında kullanılmıyor
  - Incremental index'te sadece değişen dosyaların embedding'leri güncellenir
  - Hash kapsamı: blake3(CHUNK_FORMAT_VERSION + file_content)
    Chunk formatı değiştiğinde (context header yapısı, katman kuralları vb.)
    CHUNK_FORMAT_VERSION artırılır → tüm embedding'ler otomatik yeniden üretilir

  Adım 7: Reranker — jina-reranker-v1-turbo-en (int8, 37MB)

  - fastembed desteği: RerankerModel enum'unda JINARerankerV1TurboEn yoksa
    UserDefinedRerankingModel pattern'i ile yüklenir (tokenizer + max_length + input names config)
  - model_int8.onnx (37MB)
  - Model dizini: ~/.quarry/models/jina-reranker-v1-turbo/
  - 8K token desteği, 37.8M parametre, 6 katman
  - Reranker input: NL query + enriched snippet (context header + signature + doc + body)
  - Sıralama hassasiyetinde int8 kalite kaybı ihmal edilebilir
  - Lisans: Apache 2.0 — ticari kullanım serbest

  Adım 8: Binary Quantization — 32x Bellek Tasarrufu

  - 384d float32 → binary (1-bit): symbol başına 1536 byte → 48 byte
  - 100K symbol'de: 150MB → 4.7MB
  - Sadece vector search aşamasında kullanılır:
    Binary HNSW ile aday seçimi (hamming distance) → top candidates
    Full float32 vektörlerle rescore → RRF'e gönderilecek top20
  - Jina reranker ayrı aşama, binary quantization ile ilgisi yok
  - Hem bellek hem arama hızı iyileşir

  ---
  Retrieval Pipeline (Son Hali)

  Kullanıcı sorgusu (doğal dil)
          │
          ▼
  ┌────────────────────────────────────────┐
  │  Parallel Search (tokio::join!)        │
  │  ├─ BM25 (Tantivy)            → top20 │
  │  └─ Vector (Binary HNSW→rescore)→top20│
  └────────────────────────────────────────┘
          │
          ▼
  ┌────────────────────────────────────────┐
  │  Hybrid Merge (RRF)                   │
  │  → top 20                             │
  └────────────────────────────────────────┘
          │
          ▼
  ┌────────────────────────────────────────┐
  │  Reranker (Jina Turbo int8)           │
  │  → top 10                             │
  └────────────────────────────────────────┘
          │
          ▼
        Sonuçlar

  Graceful Degradation
  - Reranker ONNX fail → skip, direkt RRF sonuçlarını döndür
  - Embedding model fail → BM25-only fallback
  - Her aşama bağımsız hata yönetimi, pipeline kısmen çalışabilir

  Metrics (ilk günden)
  - search_bm25_ms, search_vector_ms, rerank_ms, total_ms
  - tracing::info! ile her search çağrısında logla
  - Bottleneck tespiti sonradan çok zor — baştan ekle

  Uygulama Sırası
  ┌──────┬───────────────────────────────┬───────────────┬───────┐
  │ Sıra │             Adım              │     Etki      │ Efor  │
  ├──────┼───────────────────────────────┼───────────────┼───────┤
  │  0   │ Granite embedding entegrasyon │ Temel altyapı │ Orta  │
  ├──────┼───────────────────────────────┼───────────────┼───────┤
  │  1   │ Body'yi pipeline'a taşı       │ Temel altyapı │ Orta  │
  ├──────┼───────────────────────────────┼───────────────┼───────┤
  │  2   │ Tüm symbol'leri embed et      │ Yüksek        │ Düşük │
  ├──────┼───────────────────────────────┼───────────────┼───────┤
  │  3   │ Context enrichment            │ Çok yüksek    │ Orta  │
  ├──────┼───────────────────────────────┼───────────────┼───────┤
  │  4   │ AST-aware chunking            │ Yüksek        │ Yüksek│
  ├──────┼───────────────────────────────┼───────────────┼───────┤
  │  5   │ Hybrid search RRF             │ Çok yüksek    │ Düşük │
  ├──────┼───────────────────────────────┼───────────────┼───────┤
  │  6   │ Lazy embedding                │ Performans    │ Düşük │
  ├──────┼───────────────────────────────┼───────────────┼───────┤
  │  7   │ Reranker                      │ Yüksek        │ Orta  │
  ├──────┼───────────────────────────────┼───────────────┼───────┤
  │  8   │ Binary quantization           │ Performans    │ Orta  │
  └──────┴───────────────────────────────┴───────────────┴───────┘
