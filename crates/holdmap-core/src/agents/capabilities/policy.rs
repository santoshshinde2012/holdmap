//! Tool permissions and a few enumerated safety settings, as labelled configuration.
//!
//! Only rule strings from documented allow/deny/ask lists and allowlisted settings with
//! known values are kept. A rule that looks like it holds a secret is dropped.

use super::{CapabilityScope, Inventory};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_RULES: usize = 64;
const MAX_RULE_CHARS: usize = 200;

/// What a permission rule does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleDecision {
    /// Runs without asking.
    Allow,
    /// Always asks first.
    Ask,
    /// Never runs.
    Deny,
}

/// One configured tool permission rule, e.g. `Bash(npm run test:*)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRule {
    /// The rule as written, with anything secret-looking masked.
    pub rule: String,
    /// Allow, ask or deny.
    pub decision: RuleDecision,
    /// Where it is declared.
    pub scope: CapabilityScope,
    /// The settings file.
    pub source_path: PathBuf,
}

/// An allowlisted setting with a known value, e.g. Codex `sandbox_mode = "read-only"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigSetting {
    /// Setting key as written in the file.
    pub key: String,
    /// One of the documented values.
    pub value: String,
    /// Where it is declared.
    pub scope: CapabilityScope,
    /// The settings file.
    pub source_path: PathBuf,
}

/// Documented settings and their allowed values. Anything else is not exported.
const SETTINGS: &[(&str, &str, &[&str])] = &[
    (
        "codex",
        "approval_policy",
        &["untrusted", "on-failure", "on-request", "never"],
    ),
    (
        "codex",
        "sandbox_mode",
        &["read-only", "workspace-write", "danger-full-access"],
    ),
    (
        "claude-code",
        "permissions.defaultMode",
        &[
            "default",
            "acceptEdits",
            "plan",
            "bypassPermissions",
            "dontAsk",
        ],
    ),
];

impl Inventory<'_> {
    /// Read the product's separate settings files (those not already read for MCP).
    pub(super) fn policy_location(&mut self, base: &Path, scope: CapabilityScope) {
        if !self.bounded() || !self.safe_path(base) {
            return;
        }
        let files: &[&str] = match (self.product, scope) {
            ("claude-code", CapabilityScope::User) => &[".claude/settings.json"],
            ("claude-code", _) => &[".claude/settings.json", ".claude/settings.local.json"],
            ("cursor" | "cursor-agent", CapabilityScope::User) => &[".cursor/cli-config.json"],
            _ => &[],
        };
        for file in files {
            let path = base.join(file);
            if !self.seen_sources.insert(path.clone()) {
                continue;
            }
            if let Some(value) = self.read_json(&path) {
                self.json_policy(&value, scope, &path);
            }
        }
    }

    /// Project rules and settings from an already parsed JSON settings file.
    pub(super) fn json_policy(
        &mut self,
        value: &serde_json::Value,
        scope: CapabilityScope,
        path: &Path,
    ) {
        let list = |pointer: &str| -> Vec<&str> {
            value
                .pointer(pointer)
                .and_then(serde_json::Value::as_array)
                .map(|items| items.iter().filter_map(serde_json::Value::as_str).collect())
                .unwrap_or_default()
        };
        let lists: &[(&str, RuleDecision)] = match self.product {
            "claude-code" | "cursor" | "cursor-agent" => &[
                ("/permissions/allow", RuleDecision::Allow),
                ("/permissions/ask", RuleDecision::Ask),
                ("/permissions/deny", RuleDecision::Deny),
            ],
            "gemini" => &[
                ("/tools/allowed", RuleDecision::Allow),
                ("/tools/core", RuleDecision::Allow),
                ("/tools/exclude", RuleDecision::Deny),
                ("/coreTools", RuleDecision::Allow),
                ("/excludeTools", RuleDecision::Deny),
            ],
            _ => &[],
        };
        for (pointer, decision) in lists {
            for rule in list(pointer) {
                self.rule(rule, *decision, scope, path);
            }
        }
        for (product, key, allowed) in SETTINGS {
            if *product != self.product {
                continue;
            }
            let pointer = format!("/{}", key.replace('.', "/"));
            if let Some(found) = value.pointer(&pointer).and_then(serde_json::Value::as_str) {
                self.setting(key, found, allowed, scope, path);
            }
        }
    }

    /// Project settings from an already parsed TOML file.
    pub(super) fn toml_policy(&mut self, table: &toml::Table, scope: CapabilityScope, path: &Path) {
        for (product, key, allowed) in SETTINGS {
            let product_matches =
                *product == self.product || *product == "codex" && self.product == "codex-desktop";
            if !product_matches {
                continue;
            }
            if let Some(found) = table.get(*key).and_then(toml::Value::as_str) {
                self.setting(key, found, allowed, scope, path);
            }
        }
    }

    fn rule(&mut self, rule: &str, decision: RuleDecision, scope: CapabilityScope, path: &Path) {
        let rule = rule.trim();
        let masked = crate::redact::line(rule);
        if rule.is_empty()
            || rule.chars().count() > MAX_RULE_CHARS
            || rule.chars().any(char::is_control)
            || masked != rule
            || super::launch::looks_secret(rule)
        {
            self.out.limit(
                "Some permission rules were long, malformed or looked sensitive and were omitted.",
            );
            return;
        }
        if self.out.tool_rules.len() >= MAX_RULES {
            self.out
                .limit("Permission rules are limited to 64 per agent context.");
            return;
        }
        let entry = ToolRule {
            rule: rule.to_owned(),
            decision,
            scope,
            source_path: path.to_owned(),
        };
        if !self.out.tool_rules.contains(&entry) {
            self.out.tool_rules.push(entry);
        }
    }

    fn setting(
        &mut self,
        key: &str,
        value: &str,
        allowed: &[&str],
        scope: CapabilityScope,
        path: &Path,
    ) {
        if allowed.contains(&value) {
            self.out.settings.push(ConfigSetting {
                key: key.into(),
                value: value.into(),
                scope,
                source_path: path.to_owned(),
            });
        }
    }
}
