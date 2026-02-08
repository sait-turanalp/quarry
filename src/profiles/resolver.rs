//! Profile resolution logic - determines which profile to use

/// Resolves profile names from multiple sources
#[derive(Debug, Clone)]
pub struct ProfileResolver;

impl ProfileResolver {
    /// Create a new resolver
    pub fn new() -> Self {
        Self
    }

    /// Resolve profile name from tiered sources
    /// Priority: CLI > Local > Manifest > None
    pub fn resolve_profile_name(
        &self,
        cli: Option<String>,
        local: Option<String>,
        manifest: Option<String>,
    ) -> Option<String> {
        cli.or(local).or(manifest)
    }
}

impl Default for ProfileResolver {
    fn default() -> Self {
        Self::new()
    }
}
