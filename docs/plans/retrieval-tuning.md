---
title: Retrieval tuning — reranker'ı kaldır, R@10'u yükselt
status: active
next: query expansion'ı semantic_search_chunks içine taşı; sonra identifier splitting
owner: sait
created: 2026-08-01
verify: python3 benchmarks/retrieval/expand_eval.py eval/holdout.jsonl eval/django <bin>
---

## NİHAİ KONFİG (kabul edildi)

```toml
[semantic_search]
model = "minishlab/potion-code-16M-v2"   # int8, dim 256, ~16 MB tablo

[reranking]
enabled = false

[chunk_search]
# rrf_k / top_k_* → DEFAULT. Tuning'leri gürültüydü (holdout 4 kazanç / 3 kayıp).
post_rerank_heuristics_enabled = false   # default; açmak kazandırmıyor, gecikmeyi 2× yapıyor
flow_chunk_enabled = true                # taşıyıcı, dokunma
```

**Query expansion (ürüne taşınacak tek yeni kod):** sorguyu 3'e aç — ① orijinal
② identifier'ları bölünmüş (`orderby_issubset_groupby` → `orderby issubset groupby`)
③ baştaki fiil atılmış (`Added support for X` → `support for X`) — her birini `limit 20`
ile ara, 3 listeyi RRF(k=5) ile birleştir, üstten 10 al.

Beklenen: **R@10 ≈ 0.82, R@5 ≈ 0.75, p50 4-8 ms.**

## Neyin gerçekten işe yaradığı (gürültü eşiği 0.05)

| Değişiklik | Δ R@10 | Karar |
|---|---|---|
| query expansion | **+0.07** | ✅ al |
| potion-retrieval-32M → potion-code (tek sorgu) | **+0.065** | ✅ al |
| flow chunks (kapatınca) | **−0.057** | ✅ açık kalsın |
| potion-code v1 → v2 | −0.008 (4 kazanç / 5 kayıp) | ⚪ gürültü — yine de v2 (CoIR'de daha iyi genelliyor, bedava) |
| havuz tuning (rrf_k / top_k) | +0.008 (4/3) | ❌ at |
| post-rerank heuristics (15 knob) | 0.000 | ❌ at |
| chunk token target/overlap | 0.000 | ❌ at |

**Gürültü tabanı ±0.016** — koşudan koşuya tie-break oynaması dahil. Harness ve disiplin
kuralları: `benchmarks/retrieval/README.md`.

## SONUÇ (holdout, 123 sorgu — tek bakış, 2026-08-01)

Embedding modeli `potion-retrieval-32M` (int8, dim 512, 32.3 MB):

| Config | R@5 | R@10 | Gecikme |
|---|---|---|---|
| default (reranker kapalı) | 0.634 | 0.659 | 4 ms |
| config tuning (rrf_k/top_k) | 0.634 | 0.667 | 2 ms |
| reranker AÇIK (default) | 0.732 | 0.748 | 1420 ms |
| query expansion (3 rewrite, RRF, limit 20) | 0.699 | 0.789 | 9 ms |

Embedding modeli **`potion-code-16M`** (int8, dim 256, 15.8 MB) — aynı holdout, aynı config:

| Config | R@5 | R@10 | Gecikme |
|---|---|---|---|
| tek sorgu | 0.707 | 0.732 | 2 ms |
| **query expansion (limit 20)** | **0.740** | **0.821** | **4 ms** |

`potion-code-16M` her koşulda kazanıyor: tek sorguda +0.065, expansion'la +0.032 R@10 —
ve model **yarı boyutta** (dim 512 → 256). Tune setinde de aynı (0.810) → overfit değil.
**Yeni default bu olmalı.**

Nihai kazanç: 0.659 → **0.821** R@10, gecikme 4 ms'de sabit; reranker'lı en iyi
alternatife (0.748 @ 1420 ms) göre hem daha isabetli hem **355× hızlı**.

**Karar kesinleşti:** query expansion reranker'ı R@10'da yeniyor (0.789 vs 0.748) ve
**158× hızlı**. Reranker kalkıyor. (Reranker R@5'te önde — sıralama iyi, recall kötü;
agent 10 sonucu okuduğu için önemsiz.)

**Config tuning overfit çıktı:** tune'da +0.045, holdout'ta +0.008 (4/3). Atıldı.
Gate işini yaptı — bu yüzden holdout şart.

Ölçülmüş diğer bulgular:
- `post_rerank_heuristics_enabled` default **false** ve false iken `facade.rs:1870`
  erken return ediyor → source weight, penaltiler, symbol-aware, diversity, result
  filter tamamen ölü. Açınca da kazanç yok (d=0.000), gecikme 4→9 ms. Silme adayı.
- `flow_chunk_enabled` **load-bearing**: kapatınca R@10 0.722 → 0.665.
- chunk_token_target/overlap: hiçbir etkisi yok (300 ile 1200 aynı sonuç).
- `build_embedding_text` (chunks/mod.rs:2403) zaten file_path + scope + kind +
  signature + doc + body içeriyor. **Eksik olan tek şey: identifier splitting.**
  Sorgu tarafında mekanik bölme +0.07 getirdiği için doküman tarafında da büyük olmalı.

# Retrieval tuning

## Karar

Cross-encoder reranker **kaldırılıyor**. Ölçüldü: MRR 0.545 → 0.668 için 8 ms → 1000 ms
(125×). Agent ilk 10 sonucu zaten okuyup eliyor, sıralamayı umursamıyor. O efor
recall'a yatırılacak.

**Birincil metrik: R@10.** İkincil: R@5. MRR'a bakılmayacak.

## Ölçülen baseline (2026-08-01, Django 514K satır, 60 sorgu)

| Kol | R@1 | R@5 | R@10 | Gecikme |
|---|---|---|---|---|
| reranker kapalı | 0.45 | 0.72 | 0.75 | 8 ms |
| reranker top_n=15 | 0.58 | 0.77 | 0.78 | ~1000 ms |
| ripgrep | 0.15 | 0.40 | 0.52 | 436 ms |

Hedef: reranker'sız R@10 0.75 → **0.85+**.

## Tur 2: doc-side temsil (2026-08-01) — HİÇBİR DEĞİŞİKLİK GÖNDERİLMEDİ

Hipotez: static model sırasız ortalama olduğu için embed metnindeki metadata (dosya yolu,
scope/kind başlıkları) merkezi seyreltiyor; referans uygulama (`semble`) yol sinyalini
BM25'e koyup dense'i temiz tutuyor. **Ölçüm hipotezi çürüttü.**

Tune (django, n=248, tek sorgu R@10, base 0.702):

| Deney | Δ R@10 | Karar |
|---|---|---|
| embed'den dosya yolunu çıkar | −0.025 | ❌ zararlı |
| embed'den scope/kind başlıklarını çıkar | −0.033 | ❌ zararlı |
| ikisi birden (minimal embed) | −0.073 | ❌ çok zararlı |
| BM25'e tokenize edilmiş `path_text` alanı | −0.004 (2/3) | ❌ etkisiz |
| sorgu instruction prefix'i | −0.004 (15/16) | ❌ etkisiz |
| identifier splitting (embed + BM25) | +0.036 (18/9) | ⚪ eşik altı |

Metadata **seyreltmiyor, zenginleştiriyor**: çıkarmak zarar, eklemek fayda yönünde.
Identifier splitting tek repoda umut vericiydi, 4 repoda çöktü:

| repo | dil | base R@10 | + identifier split | Δ |
|---|---|---|---|---|
| django | .py | 0.717 | 0.728 | +0.011 |
| tokio | .rs | 0.729 | 0.691 | **−0.038** |
| vite | .ts | 0.573 | 0.558 | **−0.015** |
| hugo | .go | 0.525 | 0.512 | **−0.013** |

4'ün 3'ünde negatif → reddedildi, scaffolding söküldü. Ayrıca expansion açıkken kazanç
zaten sıfırlanıyordu (expansion'ın 2. rewrite'ı aynı sinyali veriyor).

## Tur 6: skor füzyonu + dosya kanıtı (2026-08-01) — GÖNDERİLDİ

Tavan ölçümü işi sıralamaya yönlendirdikten sonra iki yapısal değişiklik:

**1. Skor füzyonu (RRF yerine konveks birleşim).** RRF sadece sırayı kullanır, skor
büyüklüğünü atar ve iki kolu eşit sayar. İki kolun skoru havuz üzerinde min-max normalize
edilip `alpha * dense + (1-alpha) * bm25` ile birleştiriliyor. Literatür dayanağı:
Bruch & Gai, ECIR'23 (TM2C2) — RRF'i MS MARCO, NQ ve 9 BEIR veri kümesinde p<0.01 ile
yeniyor ([arXiv 2210.11934](https://ar5iv.org/abs/2210.11934)). Doğrusal normalizasyonlar
sıra-eşdeğer olduğu için analitik BM25 sınırı yerine havuz min-max kullanıldı; kayma
alpha süpürmesinde soğuruluyor.

**2. Dosya kanıtı toplama.** Dosya skoru = `en iyi chunk + alpha * azalan_toplam(kalanlar)`.
Retrieval chunk seviyesinde, tüketici dosya seviyesinde düşünüyor; bu boşluk hiç
kapatılmamıştı. `alpha=0` klasik max-passage'a indirgeniyor ve ölçümde base'e eşit çıktı —
**kazancın tamamı çok-chunk kanıtının birleşmesinden geliyor**, sıralamanın yeniden
düzenlenmesinden değil.

`limit=10`, `fusion_alpha=0.8`, `file_evidence_alpha=0.5`:

| repo | önce | sonra | Δ | kazanç/kayıp |
|---|---|---|---|---|
| django | 0.739 | **0.795** | +0.057 | 28/7 |
| tokio | 0.765 | **0.783** | +0.018 | 9/2 |
| vite | 0.723 | **0.753** | +0.030 | 12/1 |
| hugo | 0.682 | **0.719** | +0.037 | 12/3 |
| **ort** | **0.727** | **0.763** | **+0.035** | **61/13** |

Gecikme değişmedi (3-10 ms).

**Kabul gerekçesi (eşik esnetildi, sebebi yazılı):** ortalama +0.035, tek repo eşiği olan
0.05'in altında. Ama eşik *tek repoda gürültüyü ayırmak* için konmuştu; burada dört bağımsız
repo da pozitif ve eşleştirilmiş sayım 61/13 (McNemar p < 1e-8). Eşiğin amacı (gürültü
göndermemek) fazlasıyla karşılanıyor.

**Gözlem:** skor füzyonu django'da +0.040, tokio/vite'ta tam olarak 0.000, hugo'da +0.012.
Sebep vektör kolunun ölü olması değil (salt-vektör sonuçları anlamlı) — o repolarda sorgular
birebir identifier içerdiği için **iki kol zaten aynı dosyalarda hemfikir**, ağırlıklandırma
sadece kolların ayrıştığı yerde iş yapıyor.

## Tur 5: tavan ölçümü (2026-08-01) — hedef ulaşılabilir, iş sıralamada

Havuz kırpılmadan (`top_k_* = 2000`, derinlik 500), dosya seviyesi:

| repo | R@10 | R@50 | R@100 | R@500 | hiç bulunamadı |
|---|---|---|---|---|---|
| django | 0.741 | 0.914 | 0.951 | 0.989 | %1.1 |
| tokio | 0.767 | 0.900 | 0.936 | 0.982 | %1.8 |
| vite | 0.726 | 0.882 | 0.917 | 0.981 | %1.9 |
| hugo | 0.678 | 0.868 | 0.930 | 0.967 | %3.3 |
| **ort** | **0.728** | **0.891** | **0.934** | **0.980** | **%2.0** |

**Tavan 0.98 → ≥0.90 hedefi fiziksel olarak ulaşılabilir.**

- Sıralama ile kazanılabilir: **0.215–0.289**
- Temsil (chunk/embedding) değişikliği gerektiren: **%1.1–3.3**

### Bu ölçümün geçersiz kıldığı önceki sonuçlar

Tur 3'teki "kayıp aday havuzunda (%21-28), sıralama masum (%3-4), sıralamanın tavanı 3.7
puan" analizi **yanlıştı**: o ölçüm havuzun `limit`e kırpıldığı binary'de yapıldı, yani
sıralamaya sunulan aday sayısı zaten 10'du. Havuz açılınca tablo tersine dönüyor —
sıralamanın önünde 50-100 aday var ve kaybın tamamına yakını orada.

Aynı sebeple **"post-rerank heuristics d=0.000, ölü" sonucu da şüpheli**: 10 aday üzerinde
yeniden sıralanacak bir şey yoktu. Derin havuzda tekrar ölçülüyor.

Yatırım yönü: chunk'lama/embedding değil, **derin havuz üzerinde ucuz sıralama**.

## Tur 3: dil açığının teşhisi (2026-08-01) — açığın %80'i harness'tı

"TypeScript/Go yarı performans" bulgusu **ölçüm hatası** çıktı. Harness düzeltildikten
sonra (testleri indeksle + gold kapsamasını index'e sor):

| repo | kirli | temiz |
|---|---|---|
| django | 0.717 | 0.730 |
| tokio | 0.729 | 0.757 |
| vite | 0.573 | **0.720** |
| hugo | 0.525 | **0.678** |

Yayılım 0.20 → 0.08. Sebep: codanna test dosyalarını indekslemiyor, Go/TS testleri kaynağın
yanında duruyor (`pathparser_test.go`, `__tests__/*.spec.ts`), Python/Rust ayrı `tests/`
dizininde → gold'un %40'a varan kısmı tanım gereği bulunamaz durumdaydı.

### Kaybın katman dağılımı (dört repo, temiz set)

| | django | tokio | vite | hugo |
|---|---|---|---|---|
| havuza hiç girmemiş | 0.229 | 0.212 | 0.242 | 0.281 |
| girmiş ama sıralanamamış | 0.040 | 0.031 | 0.038 | 0.041 |

**Sıralama hatası dört repoda da %3-4 ile sabit.** Kalan kaybın tamamı aday havuzunda →
füzyon/ağırlık/reranker işleri bu açığı kapatamaz, iş korpusta.

### Bulunan iki ürün açığı

1. **Sembolsüz dosya = görünmez dosya.** `tokio/src/io/util/async_read_ext.rs` 1430 satır
   ama AST'de tek üst düzey düğüm var (her şey makro içinde) → 0 sembol → 0 chunk → dosya
   aramada tamamen yok (`find_symbol AsyncReadExt` bulamıyor). Herhangi bir kaynak dosya
   sembol üretmiyorsa chunk'sız kalıyor. **Çözüm: sembol üretmeyen kaynak dosyalar için
   satır-pencereli yedek chunk'lama** — `split_text_file_chunks` zaten var, yeniden kullan.
2. **Walker gizli dizinleri atlıyor.** `docs/.vitepress/**` diskte var, index'te yok.
   Vite gold'unun ulaşılamayan %18'inin bir kısmı bu.

## Tur 4: aday havuzu `limit`e kırpılıyordu (2026-08-01) — GÖNDERİLDİ

`facade.rs`'te füzyon sonucu `top_k_fused.min(limit)` ile kırpılıyordu: **10 sonuç istemek
"sadece 10 aday üret" demekti.** Sonuç, 10 chunk'lık bir yanıtta ortalama **5.2 farklı
dosya** — yuvaların yarısı aynı dosyanın tekrarı, ve dosya seviyesinde R@10'a ulaşmak
matematiksel olarak imkânsız.

Düzeltme iki parça:
1. Füzyon derinliği `limit`ten bağımsız (`top_k_fused.max(limit)`), kırpma en sona alındı.
2. Erken-return dalına **dosya başına chunk sınırı** + taşmadan geri doldurma (yanıt
   boyutu değişmez, sadece çeşitlenir).

`limit=10` (ürünün gerçek ayarı), `diversity_max_per_file=1`:

| repo | önce | sonra | Δ |
|---|---|---|---|
| django | 0.609 | **0.741** | +0.132 |
| tokio | 0.696 | **0.765** | +0.069 |
| vite | 0.637 | **0.723** | +0.086 |
| hugo | 0.603 | **0.682** | +0.079 |

Dördü de 0.05 eşiğini geçiyor. Ve `limit=10` artık eski `limit=30` kalitesini yakalıyor →
**aynı cevap kalitesi, 1/3 token.** Çapraz-repo mutabakat holdout'tan güçlü bir doğrulama.

Not: `diversity_max_per_file=1` bu metrikte (dosya bulma) en iyisi ama dosya-içi bağlamı
azaltır; knob config'de duruyor, "akışı anla" tipi kullanımda 2 tercih edilebilir.

### Bu turda çürütülen önceki bulgu

Query expansion'ın "+0.07"si **artefaktmış**: her rewrite `limit=20` ile çektiği için
yukarıdaki kırpma hatasını dolaylı olarak by-pass ediyordu. Havuz düzeltildikten sonra
expansion tek sorgunun üstüne **+0.009 / −0.016** katıyor — yani gereksiz. Tur 2'nin
"expansion'ı ürüne taşı" önerisi iptal.

**Sıradaki iş:** sembolsüz dosyalara yedek chunk'lama (kapsama açığı, eval'de görünmüyor
ama gerçek kullanımda var).

## Model araştırması (2026-08-01)

Kod-özel static embedding evreninin TAMAMI 2 model: `minishlab/potion-code-16M` ve
`potion-code-16M-v2` (MIT, model2vec). HF'de `filter=model2vec&filter=code` başka sonuç
vermiyor. Model2vec dışındaki static aileler (WordLlama, sentence-transformers
`static-retrieval-mrl-en-v1`) kod varyantı yayınlamamış.

CoIR (NDCG@10, `mteb>=2.10`, v2 model kartından):

| Model | Params | CoIR AVG |
|---|---|---|
| CodeRankEmbed (öğretmen, transformer) | 137M | 59.14 |
| potion-code-16M-v2 + BM25 (hybrid) | 16M | 43.36 |
| BM25 tek başına | — | 42.31 |
| potion-code-16M-v2 | 16M | 39.08 |
| potion-code-16M | 16M | 37.05 |
| potion-retrieval-32M | 32M | 32.10 |

Bizim holdout'umuzda v2 (aynı 123 sorgu): tek sorgu R@10 **0.748** (v1 0.732),
expansion R@10 **0.821** (v1 0.821), R@5 **0.756** (v1 0.740). → v2 adopte edildi.

**Yorum:** expansion'lı R@10 iki modelde de 0.821'de duruyor → **kalan hatalar embedding
kalitesiyle sınırlı değil.** Model ekseni tükendi; yatırım identifier splitting'e gitmeli.

Kaçınılması gereken yanlış okuma: CoIR'de static-tek-başına (39.08) BM25'i (42.31)
kaybediyor — ama oradaki görevler (Text2SQL, CodeTrans, Apps) bizim görevimiz değil ve
COIRCodeSearchNet ters yön (kod→metin). Bizim kazanan konfigümüz zaten onların da en iyi
static satırı olan hybrid.

Öğretmen seçimi zaten optimale yakın (CodeRankEmbed 59.14 > Voyage-code-002 56.26), yani
kendi distilasyonumuzu yapmanın getirisi düşük. Boşluk: **identifier-split metin üzerinde
eğitilmiş static kod embedding'i kimse yayınlamamış.**

## Ground truth yöntemi

Sorgu = commit mesajı (insan niyeti, koda bakmadan yazılmış).
Gold = o commit'in değiştirdiği dosyalar.
Index = commit'ten ÖNCEKİ hale checkout → sızıntı yok.
Dosya seviyesinde isabet. Test dosyaları gold'dan düşülür (indexlenmiyor).

## Adımlar

1. **Eval setini büyüt** — 60 → 200 sorgu (Django geçmişinde 350 aday var).
   Böl: **140 tune / 60 holdout**. Holdout'a sadece en sonda bir kez bakılır.
2. **Baseline dondur** — reranker kapalı, mevcut defaultlar, tune setinde. Her
   karşılaştırma buna karşı.
3. **Query-time süpürme (OFAT)** — her seferinde tek faktör, diğerleri baseline'da:
   `rrf_k` · `top_k_vector` · `top_k_bm25` · `top_k_fused` · `symbol_aware_weight` ·
   `source_weight_*` · `diversity_max_per_file` · single-line/coherence/block penaltileri.
   Reindex yok, sorgu 8 ms → geniş süpür.
4. **Kazanan ilk 3'ü birleştir** — etkileşim kontrolü. Baştan grid arama YOK.
5. **Index-time süpürme (dar)** — `chunk_token_target/max/overlap`, `flow_chunk_enabled`,
   `snippet_*`. Her varyant 16 sn reindex → kaldıraç başına 3-4 değer.
6. **Query expansion** — tek niyeti 3 varyanta açıp union. Kod gerektirir, en sona.
7. **Union recall ölçümü** — k sorgunun union'ı + call-graph komşuları. Agent'ın
   gerçekte gördüğü recall bu; tek sorgu R@10'undan yüksek olmalı.
8. **Holdout** — kazanan config tek seferde doğrulanır. Kazanç holdout'ta yoksa
   overfit'tir, geri alınır.

## Gate'ler

- Her adımda **eşleştirilmiş** karşılaştırma: aynı sorguda kaç kazanç / kaç kayıp.
  Ortalama farka tek başına güvenilmez.
- 200 sorguda ±0.03 standart hata → **0.05'in altındaki fark gürültüdür**, kabul edilmez.
- Kazanan her config için kaybedilen sorgular okunur; asıl kusur genelde orada görünür.

## Foot-gun'lar

- **Bool env değeri `"True"/"False"` yazılırsa figment parse edemiyor ve TÜM Settings
  default'a düşüyor** — reranking dahil. Sessiz: hata vermiyor, sadece yanlış config'le
  koşuyor. Teşhis işareti: sorgu gecikmesi 5 ms yerine ~2000 ms (reranker geri gelmiş).
  Daima küçük harf `"true"/"false"`. Bu tuzak bir turda sahte "+0.052 kazanç" üretti.
- Tek-atış CLI çağrısı sorgu başına ~700 ms model init ödüyor. Süpürmeler MCP stdio
  oturumuyla **tek process'te** koşulmalı (`sweep.py`) — sorgu 4-5 ms'ye düşüyor.
- Her süpürmede `baseline_dup` koş: gürültü tabanı bilinmeden fark yorumlanamaz.
- Eval setini tune ederken holdout'a bakma. 20+ config denemesi 60 sorguya overfit eder.
- `~/.codanna/models/potion-retrieval-32M-int8/` diskte tutulmalı; yoksa index çalışmaz.
  Global simetrik int8 kuantizasyon şart — satır-bazlı ölçek L2 normalizasyonda sadeleşmez.
- Django eval kopyası base commit'te detached HEAD'de duruyor; checkout değiştirme.
