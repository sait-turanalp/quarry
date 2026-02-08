//! Cross-encoder reranking for improved retrieval quality.
//!
//! After RRF merge of BM25 + vector results, reranking scores each (query, document)
//! pair with a cross-encoder model for higher precision. Uses fastembed's TextRerank
//! with Jina Reranker V1 Turbo (English, fast).

use fastembed::{
    OnnxSource, RerankInitOptions, RerankInitOptionsUserDefined, RerankerModel, TextRerank,
    TokenizerFiles, UserDefinedRerankingModel,
};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Errors during reranker initialization or inference.
#[derive(Debug, thiserror::Error)]
pub enum RerankError {
    #[error("Failed to initialize reranker: {0}")]
    InitError(String),

    #[error("Reranking failed: {0}")]
    RerankFailed(String),
}

/// Cross-encoder reranker using fastembed's TextRerank.
pub struct Reranker {
    model: Mutex<TextRerank>,
}

impl Reranker {
    /// Create a new reranker with the specified model.
    ///
    /// Supported model names: "JINARerankerV1TurboEn", "BGERerankerBase",
    /// "BGERerankerV2M3", "JINARerankerV2BaseMultiligual", or
    /// custom user-defined models via `custom:/absolute/or/relative/path`.
    pub fn new(model_name: &str) -> Result<Self, RerankError> {
        if let Some(custom_ref) = model_name.strip_prefix("custom:") {
            return Self::new_custom(custom_ref.trim());
        }

        let model_enum = parse_reranker_model(model_name)?;

        let cache_dir = crate::init::models_dir();
        let reranker =
            TextRerank::try_new(RerankInitOptions::new(model_enum).with_cache_dir(cache_dir))
                .map_err(|e| RerankError::InitError(format!("Model '{model_name}': {e}")))?;

        Ok(Self {
            model: Mutex::new(reranker),
        })
    }

    fn new_custom(custom_ref: &str) -> Result<Self, RerankError> {
        if custom_ref.is_empty() {
            return Err(RerankError::InitError(
                "Custom reranker path is empty. Use model = \"custom:/path/to/model-or-dir\""
                    .to_string(),
            ));
        }

        let custom_path = expand_user_path(custom_ref);
        let (onnx_path, tokenizer_dir) = resolve_paths(&custom_path)?;
        let tokenizer_files = load_tokenizer_files(&tokenizer_dir)?;
        let user_model =
            UserDefinedRerankingModel::new(OnnxSource::File(onnx_path), tokenizer_files);

        let reranker = TextRerank::try_new_from_user_defined(
            user_model,
            RerankInitOptionsUserDefined::default(),
        )
        .map_err(|e| {
            RerankError::InitError(format!(
                "Failed to load custom reranker from '{}': {e}",
                custom_ref
            ))
        })?;

        Ok(Self {
            model: Mutex::new(reranker),
        })
    }

    /// Rerank documents against a query.
    ///
    /// Returns `(original_index, score)` pairs sorted by score descending.
    pub fn rerank(
        &self,
        query: &str,
        documents: &[String],
        limit: usize,
    ) -> Result<Vec<(usize, f32)>, RerankError> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let mut model = self
            .model
            .lock()
            .map_err(|_| RerankError::RerankFailed("Lock poisoned".to_string()))?;

        let doc_refs: Vec<&str> = documents.iter().map(|s| s.as_str()).collect();
        let results = model
            .rerank(query, doc_refs, true, None)
            .map_err(|e| RerankError::RerankFailed(e.to_string()))?;

        let mut scored: Vec<(usize, f32)> =
            results.into_iter().map(|r| (r.index, r.score)).collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored)
    }
}

/// Parse a model name string into a RerankerModel enum variant.
fn parse_reranker_model(name: &str) -> Result<RerankerModel, RerankError> {
    match name {
        "JINARerankerV1TurboEn" => Ok(RerankerModel::JINARerankerV1TurboEn),
        "BGERerankerBase" => Ok(RerankerModel::BGERerankerBase),
        "BGERerankerV2M3" => Ok(RerankerModel::BGERerankerV2M3),
        "JINARerankerV2BaseMultiligual" => Ok(RerankerModel::JINARerankerV2BaseMultiligual),
        _ => Err(RerankError::InitError(format!(
            "Unknown reranker model: '{name}'. Available: JINARerankerV1TurboEn, BGERerankerBase, BGERerankerV2M3, JINARerankerV2BaseMultiligual, or custom:/path/to/model-or-dir"
        ))),
    }
}

fn resolve_paths(custom_path: &Path) -> Result<(PathBuf, PathBuf), RerankError> {
    if custom_path.is_file() {
        let tokenizer_dir = custom_path.parent().ok_or_else(|| {
            RerankError::InitError(format!(
                "Cannot resolve tokenizer directory for custom model path '{}'",
                custom_path.display()
            ))
        })?;
        return Ok((custom_path.to_path_buf(), tokenizer_dir.to_path_buf()));
    }

    if !custom_path.exists() {
        return Err(RerankError::InitError(format!(
            "Custom reranker path '{}' does not exist",
            custom_path.display()
        )));
    }

    let candidate_nested = custom_path.join("onnx").join("model.onnx");
    if candidate_nested.is_file() {
        return Ok((candidate_nested, custom_path.to_path_buf()));
    }

    let candidate_flat = custom_path.join("model.onnx");
    if candidate_flat.is_file() {
        return Ok((candidate_flat, custom_path.to_path_buf()));
    }

    Err(RerankError::InitError(format!(
        "No ONNX model found under '{}'. Expected 'model.onnx' or 'onnx/model.onnx'.",
        custom_path.display()
    )))
}

fn expand_user_path(input: &str) -> PathBuf {
    if input == "~" {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = input.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(input)
}

fn load_tokenizer_files(tokenizer_dir: &Path) -> Result<TokenizerFiles, RerankError> {
    Ok(TokenizerFiles {
        tokenizer_file: read_required_file(tokenizer_dir.join("tokenizer.json"))?,
        config_file: read_required_file(tokenizer_dir.join("config.json"))?,
        special_tokens_map_file: read_required_file(tokenizer_dir.join("special_tokens_map.json"))?,
        tokenizer_config_file: read_required_file(tokenizer_dir.join("tokenizer_config.json"))?,
    })
}

fn read_required_file(path: PathBuf) -> Result<Vec<u8>, RerankError> {
    std::fs::read(&path).map_err(|e| {
        RerankError::InitError(format!(
            "Failed to read required custom reranker file '{}': {}",
            path.display(),
            e
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_models() {
        assert!(parse_reranker_model("JINARerankerV1TurboEn").is_ok());
        assert!(parse_reranker_model("BGERerankerBase").is_ok());
        assert!(parse_reranker_model("BGERerankerV2M3").is_ok());
        assert!(parse_reranker_model("JINARerankerV2BaseMultiligual").is_ok());
    }

    #[test]
    fn test_parse_invalid_model() {
        let result = parse_reranker_model("NonExistentModel");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown reranker model"));
    }
}
