use crate::Range;
use serde::{Deserialize, Serialize};

/// Flow-oriented AST block kind used for chunk-level retrieval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlowKind {
    IfElse,
    TryCatch,
    Switch,
    Loop,
    CallChain,
    ErrorPath,
}

/// A semantically meaningful block extracted from AST.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowBlock {
    pub kind: FlowKind,
    pub range: Range,
    pub label: Option<String>,
    pub parent_symbol_name: Option<String>,
}

impl FlowBlock {
    #[must_use]
    pub fn new(
        kind: FlowKind,
        range: Range,
        label: Option<String>,
        parent_symbol_name: Option<String>,
    ) -> Self {
        Self {
            kind,
            range,
            label,
            parent_symbol_name,
        }
    }
}
