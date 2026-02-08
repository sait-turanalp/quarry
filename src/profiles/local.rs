//! Local overrides - personal settings at .codanna/profile.local.json

use super::error::ProfileResult;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocalOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

impl LocalOverrides {
    pub fn from_json(json: &str) -> ProfileResult<Self> {
        let overrides: Self = serde_json::from_str(json)?;
        Ok(overrides)
    }
}
