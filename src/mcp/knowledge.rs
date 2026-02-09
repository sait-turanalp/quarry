use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EcosystemKnowledge {
    pub ecosystem_name: String,
    pub languages: Vec<String>,
    pub package_managers: Vec<String>,
    #[serde(default)]
    pub stack: Vec<StackCategory>,
    #[serde(default)]
    pub domain: Vec<DomainMapping>,
    #[serde(default)]
    pub entry_points: EntryPointPatterns,
    #[serde(default)]
    pub stop_words: StopWords,
    #[serde(default)]
    pub rule: Vec<BusinessRule>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StackCategory {
    pub category: String, // "runtime", "frontend", "backend", etc.
    pub packages: Vec<String>,
    pub description: String,
    #[serde(default)]
    pub is_utility: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DomainMapping {
    pub name: String,
    pub keywords: Vec<String>,
    pub libraries: Vec<String>,
    pub behavior: String,
    pub priority: u8,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct EntryPointPatterns {
    #[serde(default)]
    pub functions: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub react_patterns: Vec<String>,
    #[serde(default)]
    pub vue_patterns: Vec<String>,
    #[serde(default)]
    pub server_patterns: Vec<String>,
    #[serde(default)]
    pub check_package_bin: bool,
    #[serde(default)]
    pub attributes: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct StopWords {
    #[serde(default)]
    pub verbs: Vec<String>,
    #[serde(default)]
    pub ui_generic: Vec<String>,
    #[serde(default)]
    pub programming_generic: Vec<String>,
    #[serde(default)]
    pub rust_generic: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BusinessRule {
    pub id: String,
    pub condition: String, // DSL expression
    pub severity: String,  // "error", "warning", "info"
    pub message: String,
}

pub struct KnowledgeBase {
    ecosystems: HashMap<String, EcosystemKnowledge>, // language → knowledge
}

impl KnowledgeBase {
    pub fn load_from_dir(path: &Path) -> Result<Self, anyhow::Error> {
        let mut ecosystems = HashMap::new();

        // Load all .toml files from knowledge/ directory
        if path.exists() && path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                let file_path = entry.path();
                if file_path.extension().and_then(|s| s.to_str()) == Some("toml") {
                    let content = std::fs::read_to_string(&file_path)?;
                    let knowledge: EcosystemKnowledge = toml::from_str(&content)
                        .map_err(|e| anyhow::anyhow!("Failed to parse {:?}: {}", file_path, e))?;

                    // Index by each language in this ecosystem
                    for lang in &knowledge.languages {
                        ecosystems.insert(lang.to_lowercase(), knowledge.clone());
                    }
                }
            }
        }

        Ok(KnowledgeBase { ecosystems })
    }

    pub fn get_for_language(&self, language: &str) -> Option<&EcosystemKnowledge> {
        self.ecosystems.get(&language.to_lowercase())
    }

    pub fn match_keywords_to_domains(
        &self,
        language: &str,
        keywords: &[String],
    ) -> Vec<(String, String, u8, usize)> {
        // (domain_name, behavior_text, priority, match_count)
        let Some(eco) = self.get_for_language(language) else {
            return Vec::new();
        };

        let mut matches = Vec::new();
        for domain in &eco.domain {
            let match_count = keywords
                .iter()
                .filter(|kw| {
                    let kw_lower = kw.to_lowercase();
                    domain.keywords.iter().any(|dk| {
                        let dk_lower = dk.to_lowercase();
                        // Exact match or prefix match (for compound words)
                        kw_lower == dk_lower
                            || kw_lower.starts_with(&dk_lower)
                            || dk_lower.starts_with(&kw_lower)
                    })
                })
                .count();

            if match_count >= 1 {
                matches.push((
                    domain.name.clone(),
                    domain.behavior.clone(),
                    domain.priority,
                    match_count,
                ));
            }
        }

        // Sort by (match_count desc, priority desc)
        matches.sort_by(|a, b| b.3.cmp(&a.3).then(b.2.cmp(&a.2)));
        matches
    }

    pub fn categorize_packages(
        &self,
        language: &str,
        packages: &[String],
    ) -> HashMap<String, Vec<String>> {
        // category → packages
        let Some(eco) = self.get_for_language(language) else {
            return HashMap::new();
        };

        let mut categorized = HashMap::new();
        for stack in &eco.stack {
            let matched: Vec<String> = packages
                .iter()
                .filter(|pkg| {
                    stack
                        .packages
                        .iter()
                        .any(|sp| pkg.to_lowercase().contains(&sp.to_lowercase()))
                })
                .cloned()
                .collect();

            if !matched.is_empty() {
                categorized.insert(stack.category.clone(), matched);
            }
        }

        categorized
    }

    pub fn get_stop_words(&self, language: &str) -> Vec<String> {
        let Some(eco) = self.get_for_language(language) else {
            return Vec::new();
        };

        let mut stop_words = Vec::new();
        stop_words.extend(eco.stop_words.verbs.clone());
        stop_words.extend(eco.stop_words.ui_generic.clone());
        stop_words.extend(eco.stop_words.programming_generic.clone());
        stop_words.extend(eco.stop_words.rust_generic.clone());
        stop_words
    }

    pub fn evaluate_rules(&self, language: &str, context: &RuleContext) -> Vec<TriggeredRule> {
        let Some(eco) = self.get_for_language(language) else {
            return Vec::new();
        };

        let mut triggered = Vec::new();
        for rule in &eco.rule {
            if evaluate_rule_condition(&rule.condition, context) {
                triggered.push(TriggeredRule {
                    id: rule.id.clone(),
                    severity: rule.severity.clone(),
                    message: rule.message.clone(),
                });
            }
        }

        triggered
    }
}

// Simple DSL evaluator for business rules
fn evaluate_rule_condition(condition: &str, context: &RuleContext) -> bool {
    // Parse condition DSL: "has_domain:X AND NOT has_library:Y"
    // Simple implementation for MVP

    let tokens: Vec<&str> = condition.split_whitespace().collect();
    let mut result = true;
    let mut negate_next = false;
    let mut current_op = LogicalOp::And;

    for token in tokens {
        match token {
            "AND" => {
                current_op = LogicalOp::And;
                continue;
            }
            "OR" => {
                current_op = LogicalOp::Or;
                continue;
            }
            "NOT" => {
                negate_next = true;
                continue;
            }
            _ => {
                if let Some(check_result) = evaluate_token(token, context) {
                    let value = if negate_next {
                        !check_result
                    } else {
                        check_result
                    };

                    result = match current_op {
                        LogicalOp::And => result && value,
                        LogicalOp::Or => result || value,
                    };

                    negate_next = false;
                }
            }
        }
    }

    result
}

#[derive(Debug, Clone, Copy)]
enum LogicalOp {
    And,
    Or,
}

fn evaluate_token(token: &str, context: &RuleContext) -> Option<bool> {
    if let Some(domain) = token.strip_prefix("has_domain:") {
        Some(context.domains.iter().any(|d| d == domain))
    } else if let Some(library) = token.strip_prefix("has_library:") {
        let libs: Vec<&str> = library.split(',').collect();
        Some(
            libs.iter()
                .any(|lib| context.libraries.iter().any(|l| l == lib)),
        )
    } else if let Some(keyword) = token.strip_prefix("has_keyword:") {
        Some(context.keywords.iter().any(|k| k == keyword))
    } else if let Some(category) = token.strip_prefix("has_category:") {
        Some(context.categories.iter().any(|c| c == category))
    } else if token == "has_entry_point" {
        Some(context.has_entry_point)
    } else {
        None
    }
}

pub struct RuleContext {
    pub domains: Vec<String>,
    pub libraries: Vec<String>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub has_entry_point: bool,
}

pub struct TriggeredRule {
    pub id: String,
    pub severity: String,
    pub message: String,
}
