//! Optimized int8 static embedding model.
//!
//! Loads a model2vec-format safetensors file with I8 embeddings and keeps
//! them in native int8 at runtime.  Mean-pooling accumulates into `i32`
//! (SIMD-friendly, no f32 conversion per lookup) and only casts to f32
//! for the final mean + L2 normalisation step.
//!
//! Compared to model2vec-rs which converts I8→f32 at load time, this
//! halves resident memory and improves cache locality for the embedding
//! table lookup.

use safetensors::SafeTensors;
use serde_json::Value;
use std::path::Path;
use tokenizers::Tokenizer;

/// Optimized int8 static embedding model.
///
/// The embedding matrix is kept in `Vec<i8>` (flat `[vocab_size * dim]`).
/// Accumulation during mean-pooling uses `i32` to stay in integer domain
/// until the final division + optional L2 normalisation.
pub struct OptimizedStaticModel {
    tokenizer: Tokenizer,
    /// Flat embedding table: `embeddings_i8[row * dim .. (row+1) * dim]`.
    embeddings_i8: Vec<i8>,
    dim: usize,
    vocab_size: usize,
    normalize: bool,
    median_token_length: usize,
    unk_token_id: Option<u32>,
}

impl std::fmt::Debug for OptimizedStaticModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OptimizedStaticModel")
            .field("dim", &self.dim)
            .field("vocab_size", &self.vocab_size)
            .field("normalize", &self.normalize)
            .field("unk_token_id", &self.unk_token_id)
            .finish()
    }
}

/// The model compiled into this binary: model2vec potion-code-16M-v2, quantised to int8
/// with a single global symmetric scale.
pub const SHIPPED_MODEL: &str = "minishlab/potion-code-16M-v2";

impl OptimizedStaticModel {
    /// Resolve a model identifier to a local directory path.
    ///
    /// Priority order:
    /// 1. If `model_path` is already a directory with `model.safetensors` → use it
    /// 2. Try `~/.quarry/models/{name}-int8/` (preferred int8 variant)
    /// 3. Try `~/.quarry/models/{name}/`
    ///
    /// For HuggingFace identifiers like `minishlab/potion-retrieval-32M`,
    /// extracts the repo name (`potion-retrieval-32M`) for lookup.
    fn resolve_model_path(model_path: &str) -> Result<std::path::PathBuf, String> {
        // Strip "model2vec:" prefix if present.
        let model_path = model_path.strip_prefix("model2vec:").unwrap_or(model_path);

        let direct = Path::new(model_path);
        if direct.join("model.safetensors").exists() {
            return Ok(direct.to_path_buf());
        }

        // Extract model name from HuggingFace-style identifier (org/name → name).
        let name = model_path.rsplit('/').next().unwrap_or(model_path);

        let models_dir = dirs::home_dir()
            .ok_or_else(|| "cannot determine home directory".to_string())?
            .join(".quarry")
            .join("models");

        // Try name-int8 first (preferred).
        let int8_path = models_dir.join(format!("{name}-int8"));
        if int8_path.join("model.safetensors").exists() {
            return Ok(int8_path);
        }

        // Try name as-is.
        let plain_path = models_dir.join(name);
        if plain_path.join("model.safetensors").exists() {
            return Ok(plain_path);
        }

        Err(format!(
            "model not found: tried '{}', '{}', '{}'",
            direct.display(),
            int8_path.display(),
            plain_path.display(),
        ))
    }

    /// Load a model2vec model from a local directory, keeping int8 embeddings
    /// in native format.
    ///
    /// Expected files: `model.safetensors`, `tokenizer.json`, `config.json`.
    ///
    /// If `model_path` is not a local directory (e.g. a HuggingFace identifier
    /// like `minishlab/potion-retrieval-32M`), attempts to resolve it under
    /// `~/.quarry/models/` by extracting the model name and trying common
    /// suffixes (`-int8`, as-is).
    pub fn from_local(model_path: &str) -> Result<Self, String> {
        // On-disk wins when it is there, so anyone who quantised their own copy keeps it.
        // When it is not, the shipped model answers instead of the whole engine failing.
        let base = match Self::resolve_model_path(model_path) {
            Ok(base) => base,
            Err(e) if Self::is_shipped_model(model_path) => {
                tracing::debug!(target: "semantic", "using the embedded model ({e})");
                return Self::embedded();
            }
            Err(e) => return Err(e),
        };
        let config = std::fs::read_to_string(base.join("config.json"))
            .map_err(|e| format!("failed to read config.json: {e}"))?;
        let tokenizer = std::fs::read(base.join("tokenizer.json"))
            .map_err(|e| format!("failed to read tokenizer.json: {e}"))?;
        let model = std::fs::read(base.join("model.safetensors"))
            .map_err(|e| format!("failed to read model.safetensors: {e}"))?;
        Self::from_bytes(&config, &tokenizer, &model)
    }

    /// Whether this identifier names the model compiled into the binary.
    fn is_shipped_model(model_path: &str) -> bool {
        let name = model_path
            .strip_prefix("model2vec:")
            .unwrap_or(model_path)
            .rsplit('/')
            .next()
            .unwrap_or(model_path);
        matches!(
            name,
            "potion-code-16M-v2" | "potion-code-16M-v2-int8" | SHIPPED_MODEL
        )
    }

    /// The model this binary ships with, loaded from bytes baked in at compile time.
    ///
    /// A retrieval engine that cannot embed anything until the user has hunted down a
    /// model file is not one binary, it is two. Sixteen megabytes buys a tool that works
    /// the moment it is installed, offline, with nothing to fetch and nothing to configure.
    pub fn embedded() -> Result<Self, String> {
        Self::from_bytes(
            include_str!("../../assets/model/potion-code-16M-v2-int8/config.json"),
            include_bytes!("../../assets/model/potion-code-16M-v2-int8/tokenizer.json"),
            include_bytes!("../../assets/model/potion-code-16M-v2-int8/model.safetensors"),
        )
    }

    /// Shared loader: the source of the three files does not matter once they are bytes.
    fn from_bytes(
        cfg_bytes: &str,
        tokenizer_bytes: &[u8],
        model_bytes: &[u8],
    ) -> Result<Self, String> {
        // --- config.json ---
        let cfg: Value =
            serde_json::from_str(cfg_bytes).map_err(|e| format!("bad config.json: {e}"))?;
        let normalize = cfg
            .get("normalize")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        // --- tokenizer ---
        let tokenizer = Tokenizer::from_bytes(tokenizer_bytes)
            .map_err(|e| format!("failed to load tokenizer: {e}"))?;

        // Median token length for char-level pre-truncation.
        let mut lens: Vec<usize> = tokenizer
            .get_vocab(false)
            .keys()
            .map(|tk| tk.len())
            .collect();
        lens.sort_unstable();
        let median_token_length = lens.get(lens.len() / 2).copied().unwrap_or(1);

        // UNK token ID.
        let unk_token_id = {
            let spec_json = tokenizer
                .to_string(false)
                .map_err(|e| format!("tokenizer -> JSON failed: {e}"))?;
            let spec: Value =
                serde_json::from_str(&spec_json).map_err(|e| format!("bad tokenizer json: {e}"))?;
            let unk_str = spec
                .get("model")
                .and_then(|m| m.get("unk_token"))
                .and_then(Value::as_str)
                .unwrap_or("[UNK]");
            tokenizer.token_to_id(unk_str)
        };

        // --- safetensors (int8 path) ---
        let safet = SafeTensors::deserialize(model_bytes)
            .map_err(|e| format!("failed to parse safetensors: {e}"))?;

        let tensor = safet
            .tensor("embeddings")
            .map_err(|e| format!("embeddings tensor not found: {e}"))?;

        let shape: [usize; 2] = tensor
            .shape()
            .try_into()
            .map_err(|_| "embedding tensor is not 2-D".to_string())?;
        let [vocab_size, dim] = shape;

        if tensor.dtype() != safetensors::Dtype::I8 {
            return Err(format!(
                "OptimizedStaticModel requires I8 tensor, got {:?}",
                tensor.dtype()
            ));
        }

        // Reinterpret &[u8] as &[i8], then copy into owned Vec<i8>.
        let raw: &[u8] = tensor.data();
        let expected_len = vocab_size * dim;
        if raw.len() != expected_len {
            return Err(format!(
                "tensor byte length {} != vocab_size*dim {}",
                raw.len(),
                expected_len
            ));
        }

        // SAFETY: u8 and i8 have identical layout; we copy into an owned vec.
        let embeddings_i8: Vec<i8> = raw.iter().map(|&b| b as i8).collect();

        tracing::info!(
            target: "semantic",
            "OptimizedStaticModel loaded: vocab={vocab_size}, dim={dim}, \
             normalize={normalize}, unk_id={unk_token_id:?}, \
             median_tok_len={median_token_length}, \
             embedding_table={:.1}MB (int8)",
            (expected_len as f64) / 1_048_576.0
        );

        Ok(Self {
            tokenizer,
            embeddings_i8,
            dim,
            vocab_size,
            normalize,
            median_token_length,
            unk_token_id,
        })
    }

    /// Embedding dimensionality.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Encode a single text into an embedding vector.
    pub fn encode_single(&self, text: &str) -> Vec<f32> {
        let truncated = Self::truncate_str(text, 512, self.median_token_length);
        let encoding = self
            .tokenizer
            .encode_fast(truncated, false)
            .expect("tokenization failed");

        let ids = encoding.get_ids();
        self.pool_ids_i8(ids)
    }

    /// Encode a batch of texts with optional max-token truncation.
    pub fn encode_batch(
        &self,
        texts: &[String],
        max_length: Option<usize>,
        batch_size: usize,
    ) -> Vec<Vec<f32>> {
        let max_tok = max_length.unwrap_or(512);
        let mut results = Vec::with_capacity(texts.len());

        for batch in texts.chunks(batch_size) {
            let truncated: Vec<&str> = batch
                .iter()
                .map(|t| Self::truncate_str(t, max_tok, self.median_token_length))
                .collect();

            let encodings = self
                .tokenizer
                .encode_batch_fast::<String>(truncated.into_iter().map(Into::into).collect(), false)
                .expect("tokenization failed");

            for encoding in encodings {
                let mut ids = encoding.get_ids().to_vec();
                // Remove UNK tokens.
                if let Some(unk) = self.unk_token_id {
                    ids.retain(|&id| id != unk);
                }
                // Truncate to max tokens.
                ids.truncate(max_tok);
                results.push(self.pool_ids_i8(&ids));
            }
        }

        results
    }

    /// Encode a batch of texts using rayon for parallel tokenization + pooling.
    ///
    /// Each text is independently tokenized and pooled on a rayon thread.
    /// Best for large batches (>10 texts) where parallelism overhead is amortised.
    pub fn encode_batch_parallel(
        &self,
        texts: &[String],
        max_length: Option<usize>,
    ) -> Vec<Vec<f32>> {
        use rayon::prelude::*;

        let max_tok = max_length.unwrap_or(512);

        texts
            .par_iter()
            .map(|text| {
                let truncated = Self::truncate_str(text, max_tok, self.median_token_length);
                let encoding = self
                    .tokenizer
                    .encode_fast(truncated, false)
                    .expect("tokenization failed");

                let mut ids = encoding.get_ids().to_vec();
                if let Some(unk) = self.unk_token_id {
                    ids.retain(|&id| id != unk);
                }
                ids.truncate(max_tok);
                self.pool_ids_i8(&ids)
            })
            .collect()
    }

    /// Mean-pool token IDs into an f32 embedding, accumulating in i32.
    ///
    /// Hot path — the inner loop over `dim` elements (typically 512) is a
    /// straight i8→i32 widening addition that LLVM auto-vectorises with
    /// NEON (aarch64) / SSE-AVX (x86_64) when compiled with
    /// `-C target-cpu=native`.
    #[inline]
    fn pool_ids_i8(&self, ids: &[u32]) -> Vec<f32> {
        let dim = self.dim;
        let mut sum = vec![0i32; dim];
        let mut count: u32 = 0;

        for &id in ids {
            // Skip UNK.
            if self.unk_token_id == Some(id) {
                continue;
            }
            let row_idx = id as usize;
            if row_idx >= self.vocab_size {
                continue;
            }

            let row_start = row_idx * dim;
            let row = &self.embeddings_i8[row_start..row_start + dim];

            // i8 → i32 accumulation (auto-vectorisable).
            for i in 0..dim {
                sum[i] += row[i] as i32;
            }
            count += 1;
        }

        let denom = count.max(1) as f32;
        let mut out = vec![0.0f32; dim];
        for i in 0..dim {
            out[i] = sum[i] as f32 / denom;
        }

        if self.normalize {
            let norm = out.iter().map(|&v| v * v).sum::<f32>().sqrt().max(1e-12);
            for v in &mut out {
                *v /= norm;
            }
        }

        out
    }

    /// Char-level truncation matching model2vec-rs behaviour.
    fn truncate_str(s: &str, max_tokens: usize, median_len: usize) -> &str {
        let max_chars = max_tokens.saturating_mul(median_len);
        match s.char_indices().nth(max_chars) {
            Some((byte_idx, _)) => &s[..byte_idx],
            None => s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL_PATH: &str = "/Users/sait/.quarry/models/potion-retrieval-32M-int8";

    fn model_available() -> bool {
        Path::new(MODEL_PATH).join("model.safetensors").exists()
    }

    #[test]
    #[ignore = "Requires local int8 model"]
    fn test_load_and_encode_single() {
        if !model_available() {
            eprintln!("Skipping: model not found at {MODEL_PATH}");
            return;
        }
        let model = OptimizedStaticModel::from_local(MODEL_PATH).unwrap();
        assert_eq!(model.dim(), 512);

        let emb = model.encode_single("parse JSON data");
        assert_eq!(emb.len(), 512);

        // Verify L2-normalized (norm ≈ 1.0).
        let norm: f32 = emb.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01, "expected unit norm, got {norm}");
    }

    #[test]
    #[ignore = "Requires local int8 model"]
    fn test_encode_batch() {
        if !model_available() {
            eprintln!("Skipping: model not found at {MODEL_PATH}");
            return;
        }
        let model = OptimizedStaticModel::from_local(MODEL_PATH).unwrap();
        let texts: Vec<String> = vec![
            "parse JSON data".to_string(),
            "connect to database".to_string(),
            "calculate hash".to_string(),
        ];
        let results = model.encode_batch(&texts, Some(512), 1024);
        assert_eq!(results.len(), 3);
        for emb in &results {
            assert_eq!(emb.len(), 512);
        }
    }
}
