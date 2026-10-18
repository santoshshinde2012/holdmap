//! Quick-select presets: manifests for well-known local analyzers that holdmap ships ready to
//! add. A preset is not part of the signed catalog. Adding one registers it as the user's own
//! analyzer, so it stays off until the user reviews its permissions sheet and approves that
//! exact manifest and binary, like any manifest they add themselves.

use crate::manifest::{AnalyzerManifest, ManifestError};
use serde::{Deserialize, Serialize};

/// The preset manifests, in the order the UI lists them.
pub const SOURCES: &[&str] = &[
    include_str!("../presets/cisco.skill-scanner.toml"),
    include_str!("../presets/google.osv-scanner.toml"),
    include_str!("../presets/aquasecurity.trivy-secrets.toml"),
    include_str!("../presets/pycqa.bandit.toml"),
];

/// What the add flow shows for one preset before anything is registered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetView {
    /// Analyzer id once added.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Who publishes the tool.
    pub publisher: String,
    /// What it checks.
    pub summary: String,
    /// Project page.
    pub homepage: String,
    /// SPDX licence.
    pub license: String,
    /// How to install the tool (shown, never run).
    pub install: Option<String>,
    /// Hosts it contacts; empty means none.
    pub network: Vec<String>,
    /// Whether the user already added it.
    pub added: bool,
    /// Whether its program was found (on PATH or a usual install folder).
    pub installed: bool,
}

/// Every preset manifest, parsed with the same rules as a manifest the user adds.
pub fn manifests() -> Vec<(AnalyzerManifest, &'static str)> {
    SOURCES
        .iter()
        .map(|text| {
            let m = AnalyzerManifest::parse(text).expect("presets are valid manifests");
            (m, *text)
        })
        .collect()
}

/// The manifest text of one preset.
pub fn source(id: &str) -> Result<&'static str, ManifestError> {
    manifests()
        .into_iter()
        .find(|(m, _)| m.id == id)
        .map(|(_, text)| text)
        .ok_or_else(|| {
            let known: Vec<String> = manifests().into_iter().map(|(m, _)| m.id).collect();
            ManifestError::Rule(format!(
                "no preset named {id:?}; presets: {}",
                known.join(", ")
            ))
        })
}

/// The views, marking those `added` reports as registered and `installed` finds.
pub fn views(
    added: impl Fn(&str) -> bool,
    installed: impl Fn(&AnalyzerManifest) -> bool,
) -> Vec<PresetView> {
    manifests()
        .into_iter()
        .map(|(m, _)| PresetView {
            added: added(&m.id),
            installed: installed(&m),
            network: m.permissions.network.clone(),
            id: m.id,
            name: m.name,
            publisher: m.publisher,
            summary: m.summary,
            homepage: m.homepage,
            license: m.license,
            install: m.install,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{OutputFormat, RunnerKind};

    #[test]
    fn presets_are_local_command_line_tools_that_never_start_mcp_servers_or_need_keys() {
        let all = manifests();
        assert_eq!(all.len(), SOURCES.len());
        for (m, _) in &all {
            assert_eq!(m.runner.kind, RunnerKind::Cli, "{}", m.id);
            assert!(!m.permissions.spawns_mcp_servers, "{}", m.id);
            assert!(
                m.permissions.secrets.is_empty(),
                "{}: no API keys by default",
                m.id
            );
            assert_eq!(m.permissions.read, ["{subject.dir}"], "{}", m.id);
            assert_eq!(m.output.format, OutputFormat::Sarif, "{}", m.id);
            assert!(m.homepage.starts_with("https://github.com/"), "{}", m.id);
            assert!(m.install.is_some(), "{}", m.id);
            assert!(
                !m.runner.args.iter().any(|a| a.contains("/dev/null")),
                "{}: use {{empty.file}}, which works on Windows too",
                m.id
            );
            assert_eq!(
                m.uses_network(),
                !m.permissions.sends.is_empty(),
                "{}: network use is explained",
                m.id
            );
        }
    }

    #[test]
    fn presets_ignore_config_files_a_skill_could_ship_to_hide_findings() {
        let args = |id: &str| {
            manifests()
                .into_iter()
                .find(|(m, _)| m.id == id)
                .unwrap()
                .0
                .runner
                .args
                .join(" ")
        };
        assert!(args("pycqa.bandit").contains("--ini {empty.file}"));
        assert!(args("google.osv-scanner").contains("--config {empty.file}"));
        assert!(args("google.osv-scanner").contains("--no-resolve"));
        let trivy = args("aquasecurity.trivy-secrets");
        assert!(trivy.contains("--disable-telemetry") && trivy.contains("--skip-version-check"));
        assert!(
            !args("cisco.skill-scanner").contains("--use-"),
            "no LLM or cloud analyzers"
        );
    }

    #[test]
    fn presets_never_collide_with_the_signed_catalog() {
        let signed: Vec<String> = crate::catalog::bundled()
            .into_iter()
            .map(|m| m.id)
            .collect();
        for (m, _) in manifests() {
            assert!(!signed.contains(&m.id), "{}", m.id);
        }
    }

    #[test]
    fn an_unknown_preset_names_the_known_ones() {
        let e = source("nope").unwrap_err().to_string();
        assert!(
            e.contains("cisco.skill-scanner") && e.contains("pycqa.bandit"),
            "{e}"
        );
    }
}
