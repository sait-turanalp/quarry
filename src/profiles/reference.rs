//! Profile reference parsing - handles profile@provider syntax

use std::fmt;

/// Reference to a profile with optional provider
///
/// Supports syntax:
/// - "myprofile" -> profile only
/// - "myprofile@provider" -> profile with specific provider
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileReference {
    pub profile: String,
    pub provider: Option<String>,
}

impl ProfileReference {
    /// Parse profile reference from string
    ///
    /// # Examples
    ///
    /// ```
    /// use codanna::profiles::reference::ProfileReference;
    ///
    /// let ref1 = ProfileReference::parse("codanna");
    /// assert_eq!(ref1.profile, "codanna");
    /// assert_eq!(ref1.provider, None);
    ///
    /// let ref2 = ProfileReference::parse("codanna@claude-provider");
    /// assert_eq!(ref2.profile, "codanna");
    /// assert_eq!(ref2.provider, Some("claude-provider".to_string()));
    /// ```
    pub fn parse(input: &str) -> Self {
        if let Some((profile, provider)) = input.split_once('@') {
            Self {
                profile: profile.to_string(),
                provider: Some(provider.to_string()),
            }
        } else {
            Self {
                profile: input.to_string(),
                provider: None,
            }
        }
    }

    /// Create a new profile reference
    pub fn new(profile: String, provider: Option<String>) -> Self {
        Self { profile, provider }
    }

    /// Check if provider is specified
    pub fn has_provider(&self) -> bool {
        self.provider.is_some()
    }
}

impl fmt::Display for ProfileReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(provider) = &self.provider {
            write!(f, "{}@{}", self.profile, provider)
        } else {
            write!(f, "{}", self.profile)
        }
    }
}

impl From<&str> for ProfileReference {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for ProfileReference {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_profile_only() {
        let reference = ProfileReference::parse("codanna");
        assert_eq!(reference.profile, "codanna");
        assert_eq!(reference.provider, None);
        assert!(!reference.has_provider());
    }

    #[test]
    fn test_parse_profile_with_provider() {
        let reference = ProfileReference::parse("codanna@claude-provider");
        assert_eq!(reference.profile, "codanna");
        assert_eq!(reference.provider, Some("claude-provider".to_string()));
        assert!(reference.has_provider());
    }

    #[test]
    fn test_parse_multiple_at_signs() {
        // Only first @ is significant
        let reference = ProfileReference::parse("profile@provider@extra");
        assert_eq!(reference.profile, "profile");
        assert_eq!(reference.provider, Some("provider@extra".to_string()));
    }

    #[test]
    fn test_display_profile_only() {
        let reference = ProfileReference::new("codanna".to_string(), None);
        assert_eq!(reference.to_string(), "codanna");
    }

    #[test]
    fn test_display_profile_with_provider() {
        let reference =
            ProfileReference::new("codanna".to_string(), Some("claude-provider".to_string()));
        assert_eq!(reference.to_string(), "codanna@claude-provider");
    }

    #[test]
    fn test_roundtrip_parsing() {
        let inputs = vec!["codanna", "codanna@provider", "my-profile@my-provider"];

        for input in inputs {
            let reference = ProfileReference::parse(input);
            let output = reference.to_string();
            assert_eq!(input, output);
        }
    }

    #[test]
    fn test_from_str() {
        let reference: ProfileReference = "codanna@provider".into();
        assert_eq!(reference.profile, "codanna");
        assert_eq!(reference.provider, Some("provider".to_string()));
    }

    #[test]
    fn test_from_string() {
        let s = String::from("codanna@provider");
        let reference: ProfileReference = s.into();
        assert_eq!(reference.profile, "codanna");
        assert_eq!(reference.provider, Some("provider".to_string()));
    }

    #[test]
    fn test_empty_profile_name() {
        // Edge case: empty profile name
        let reference = ProfileReference::parse("@provider");
        assert_eq!(reference.profile, "");
        assert_eq!(reference.provider, Some("provider".to_string()));
    }

    #[test]
    fn test_empty_provider_name() {
        // Edge case: empty provider name
        let reference = ProfileReference::parse("profile@");
        assert_eq!(reference.profile, "profile");
        assert_eq!(reference.provider, Some("".to_string()));
    }
}
