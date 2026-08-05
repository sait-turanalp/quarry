# Retrieval eval harness

Sızıntısız, elle etiketleme gerektirmeyen retrieval ölçümü. Sorular:
"geliştirici niyetini söyledi — doğru dosya ilk 10'da mı?"

**Birincil metrik: R@10.** İkincil: R@5. MRR'a bakma — agent ilk 10'u okuyup kendi eliyor,
sıralama içindeki yer önemsiz.

## Ground truth nasıl kuruluyor

| | |
|---|---|
| Sorgu | Gerçek commit mesajı (insan niyeti, koda bakmadan yazılmış) |
| Gold | O commit'in değiştirdiği dosyalar |
| Index | Repo commit'ten **önceki** haline checkout → cevap index'te yok |
| Filtre | Test dosyaları gold'dan düşülür (indexlenmiyorlar) |

## Kullanım

```bash
# 1) Hedef repoyu klonla, geçmişte bir base commit'e in
git clone --depth 1100 https://github.com/django/django.git eval/django
cd eval/django && git checkout $(git rev-parse origin/main~900)

# 2) Eval setini üret, tune/holdout böl (böl! yoksa overfit edersin)
python3 build_eval_set.py eval/django $(git -C eval/django rev-parse HEAD) eval/all.jsonl 600

# 3) Indexle
cd eval/django && quarry init && quarry index . --force

# 4) Config süpürmesi (tek process, sorgu ~4ms)
python3 sweep.py eval/tune.jsonl eval/django <bin> spec.json

# 5) Index-time kaldıraçlar (varyant başına reindex)
python3 sweep_index.py eval/tune.jsonl eval/django <bin> spec_index.json

# 6) Query expansion + nihai ölçüm
python3 expand_eval.py eval/holdout.jsonl eval/django <bin> per_query.json
```

`quantize_static_int8.py` bir model2vec safetensors'ı `OptimizedStaticModel`'in
beklediği int8 formata çevirir → `~/.quarry/models/<isim>-int8/`.

## Çok repolu suite (tek repo kanıt değildir)

Bir değişiklik bir korpusta kazanıp diğerinde kaybedebilir. Identifier splitting django'da
**+0.011**, tokio'da **−0.038**, vite'ta **−0.015**, hugo'da **−0.013** ölçüldü — tek repoya
baksaydık yanlış karar verirdik. **Tek repo hipotezdir; kanıt repolar arası mutabakattır.**

```bash
python3 suite.py prepare <workdir>                  # klonla, base'e in, eval setlerini üret
python3 suite.py run <workdir> <bin> <spec.json>    # aynı spec'i her repoda koş
```

Repolar `repos.json`'da: django (.py) · tokio (.rs) · vite (.ts) · hugo (.go).
Yeni repo eklemek bir satır; `ext` alanı gold dosya uzantısını belirler.

**Repolar arası MUTLAK sayılar kıyaslanamaz**, yalnızca repo-içi delta'lar anlamlıdır:
sorgu kalitesi repodan repoya değişiyor. Hugo release-chore commit'leri ("releaser: Bump
versions for release of 0.147.7") temizlenmeden 0.372, temizlendikten sonra 0.525 ölçtü —
aradaki fark pipeline değil, cevaplanamaz sorgulardı.

## Zorunlu ön denetim: `coverage.py`

**Ölçmeden önce gold'un index'te olduğunu doğrula.** İndekslenmemiş bir gold dosyası
retrieval kaybı gibi puanlanır — yani bir indeksleme kuralı sessizce "kalite" sonucuna
dönüşür.

```bash
python3 coverage.py <eval.jsonl> <repo> <bin> [label]   # -> <eval>.checked.jsonl
```

Bu tuzağa iki kez düştük:
1. quarry varsayılan olarak test dosyalarını indekslemiyor; Go ve TypeScript testleri
   kaynağın yanına koyuyor (`pathparser_test.go`, `__tests__/x.spec.ts`), Python/Rust ise
   ayrı `tests/` dizinine. Sonuç: hugo gold'unun %40'ı, vite'ın %26'sı **tanım gereği**
   bulunamaz durumdaydı. Suite artık `QUARRY_INDEXING__INCLUDE_TESTS=true` ile indeksliyor.
2. Test desenlerini regex ile tahmin etmek işe yaramadı — dile göre değişiyor. Doğru yöntem
   tahmin değil, **index'e sormak**; `coverage.py` bunu yapar.

Düzeltmenin etkisi (aynı kod, aynı model, sadece harness):

| repo | kirli | temiz |
|---|---|---|
| django | 0.717 | 0.730 |
| tokio | 0.729 | 0.757 |
| vite | 0.573 | **0.720** |
| hugo | 0.525 | **0.678** |

Diller arası yayılım 0.20 → 0.08. "Dil açığı" sanılan şeyin ~%80'i ölçüm hatasıydı.

## `diagnose.py` — kaybı katmana yaz

```bash
python3 diagnose.py <eval.jsonl> <repo> <bin> [label]    # DIAG_LIMIT=30 önerilir
```

`R@10` tek başına *neden* düşük olduğunu söylemez; derinliğe bölmek söyler:

- **havuza hiç girmemiş** → aday havuzu cevabı hiç içermemiş (parser kapsaması / chunk'lama
  / embedding). Sıralama masum.
- **girmiş ama sıralanamamış** → cevap bulunmuş, sıralama batırmış (füzyon / ağırlıklar).

Dört repoda ölçüldü: sıralama hatası **%3-4 ile sabit**, kayıp neredeyse tamamen havuzda.

**`DIAG_LIMIT`/`limit` yeterince büyük olmalı.** Dosya seviyesinde R@10 ölçerken 10 chunk
istemek yetmez: aynı dosyadan gelen chunk'lar dedupe olunca 10'dan az farklı dosya kalır ve
metrik matematiksel olarak tavana çarpar (django limit=10 → 0.725, limit=30 → 0.776).

## Disiplin kuralları (bunlara uymayan ölçüm çöptür)

- **tune/holdout böl.** Holdout'a sadece EN SONDA bir kez bak. Bu oturumda config tuning
  tune'da +0.045 gösterdi, holdout'ta +0.008 çıktı — yani overfit'ti.
- **Eşleştirilmiş karşılaştır**: aynı sorguda kaç kazanç / kaç kayıp. Ortalama farka
  tek başına güvenme.
- **Gürültü tabanı ±0.016** (n=123, koşudan koşuya tie-break oynaması dahil).
  **0.05'in altındaki fark kabul edilmez.**
- Her süpürmede bir `baseline_dup` koş — gürültü tabanını bilmeden fark yorumlanamaz.

## Foot-gun'lar

- **Bool env değeri `"True"/"False"` yazılırsa figment parse edemiyor ve TÜM Settings
  default'a düşüyor** (reranking dahil), hata vermeden. Teşhis: sorgu 4 ms yerine ~2000 ms.
  Daima küçük harf `"true"/"false"`. Bu tuzak bir turda sahte "+0.052 kazanç" üretti.
- Tek-atış CLI çağrısı sorgu başına ~700 ms model init ödüyor. Süpürmeler `sweep.py`'ın
  MCP stdio oturumuyla koşulmalı → 4 ms.
- int8 kuantizasyon **global simetrik** olmalı. Satır-bazlı ölçek L2 normalizasyonda
  sadeleşmez, modeli bozar.
- Model dizini `~/.quarry/models/<name>-int8/` altında olmalı; yoksa index çalışmaz.
