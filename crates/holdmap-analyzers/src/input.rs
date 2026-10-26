//! What an analyzer is given: subjects projected from holdmap's inventory under the manifest's
//! declared inputs only. Folder paths go to local CLI analyzers; package coordinates go to OSV.
//! Nothing else (environment, command lines, config contents) is ever part of an input.

use crate::finding::{Subject, SubjectKind};
use crate::manifest::{AnalyzerManifest, PerSubject};
use holdmap_core::agents::{AgentCapabilities, AgentsReport, McpRunner};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A package version OSV can be asked about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PackageCoord {
    /// OSV ecosystem: `npm` or `PyPI`.
    pub ecosystem: String,
    /// Package name.
    pub name: String,
    /// Exact version.
    pub version: String,
}

impl PackageCoord {
    /// Subject id: `package:<ecosystem>/<name>@<version>`.
    pub fn subject_id(&self) -> String {
        format!("package:{}/{}@{}", self.ecosystem, self.name, self.version)
    }
}

/// One subject with the single input it carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputSubject {
    /// The subject.
    pub subject: Subject,
    /// Skill folder (CLI analyzers).
    pub dir: Option<PathBuf>,
    /// Package (OSV).
    pub package: Option<PackageCoord>,
}

/// Everything one run is given.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzerInput {
    /// Subjects, in a stable order.
    pub subjects: Vec<InputSubject>,
    /// Things that were found but are not analysed, in plain language (e.g. unpinned packages).
    pub skipped: Vec<String>,
}

/// Builds an analyzer's input from the inventory.
pub trait InputProjector: Send + Sync {
    /// Project only what the manifest declares.
    fn project(&self, report: &AgentsReport, manifest: &AnalyzerManifest) -> AnalyzerInput;
}

/// Projects skills and pinned MCP packages from an [`AgentsReport`].
#[derive(Debug, Default, Clone, Copy)]
pub struct InventoryProjector;

impl InputProjector for InventoryProjector {
    fn project(&self, report: &AgentsReport, manifest: &AnalyzerManifest) -> AnalyzerInput {
        match manifest.runner.per_subject {
            PerSubject::Skill => skill_subjects(report),
            PerSubject::Packages => packages(report),
        }
    }
}

/// Every inventory in the report: running agents, then configured agents that aren't running.
pub(crate) fn inventories(
    report: &AgentsReport,
) -> impl Iterator<Item = (&str, &AgentCapabilities)> {
    report
        .agents
        .iter()
        .map(|a| (a.product.as_str(), &a.capabilities))
        .chain(
            report
                .configured
                .iter()
                .map(|c| (c.product.as_str(), &c.capabilities)),
        )
}

/// Every skill folder in the report once, with the agents that load it.
pub fn skill_subjects(report: &AgentsReport) -> AnalyzerInput {
    let mut by_dir: BTreeMap<PathBuf, InputSubject> = BTreeMap::new();
    for (product, capabilities) in inventories(report) {
        for skill in &capabilities.skills {
            let Some(dir) = skill.source_path.parent().map(PathBuf::from) else {
                continue;
            };
            let entry = by_dir.entry(dir.clone()).or_insert_with(|| InputSubject {
                subject: Subject {
                    id: skill.id.clone(),
                    kind: SubjectKind::Skill,
                    name: skill.name.clone(),
                    agents: Vec::new(),
                    dir: Some(dir.display().to_string()),
                    scope: Some(skill.scope.label().to_string()),
                },
                dir: Some(dir),
                package: None,
            });
            if !entry.subject.agents.iter().any(|a| a == product) {
                entry.subject.agents.push(product.to_string());
            }
        }
    }
    let mut skipped = Vec::new();
    if by_dir.is_empty() {
        let agents = inventories(report).count();
        let what = match agents {
            0 => "no agents were found".to_string(),
            1 => "1 agent loads no skills".to_string(),
            n => format!("{n} agents load no skills"),
        };
        skipped.push(format!(
            "No skills to check: {what}. Add a skill folder with a SKILL.md to have it scanned."
        ));
    }
    AnalyzerInput {
        subjects: by_dir.into_values().collect(),
        skipped,
    }
}

fn packages(report: &AgentsReport) -> AnalyzerInput {
    let mut by_coord: BTreeMap<PackageCoord, InputSubject> = BTreeMap::new();
    let mut skipped = Vec::new();
    let mut servers = 0usize;
    for (product, capabilities) in inventories(report) {
        for server in &capabilities.mcp_servers {
            servers += 1;
            let Some(launch) = &server.launch else {
                continue;
            };
            let Some(spec) = launch.package.as_deref() else {
                continue;
            };
            let coord = match launch.runner {
                McpRunner::Npx => npm_coord(spec),
                McpRunner::Uvx => pypi_coord(spec),
                _ => continue,
            };
            let Some(coord) = coord.filter(|_| launch.pinned == Some(true)) else {
                let note = format!(
                    "{} ({}) is not pinned to a version, so it was not checked.",
                    server.name, spec
                );
                if !skipped.contains(&note) {
                    skipped.push(note);
                }
                continue;
            };
            let entry = by_coord
                .entry(coord.clone())
                .or_insert_with(|| InputSubject {
                    subject: Subject {
                        id: coord.subject_id(),
                        kind: SubjectKind::Package,
                        name: format!("{}@{}", coord.name, coord.version),
                        agents: Vec::new(),
                        dir: None,
                        scope: None,
                    },
                    dir: None,
                    package: Some(coord),
                });
            if !entry.subject.agents.iter().any(|a| a == product) {
                entry.subject.agents.push(product.to_string());
            }
        }
    }
    if by_coord.is_empty() {
        // Not a failure: there is simply nothing OSV can look up. Say why and what to do.
        let what = match servers {
            0 => "no MCP servers were found".to_string(),
            1 => "1 MCP server uses an unpinned or non-npm/PyPI command".to_string(),
            n => format!("{n} MCP servers use unpinned or non-npm/PyPI commands"),
        };
        skipped.insert(
            0,
            format!(
                "No pinned packages to check: {what}. Pin a version (npx pkg@1.2.3 or uvx pkg==1.2.3) to have it checked."
            ),
        );
    }
    AnalyzerInput {
        subjects: by_coord.into_values().collect(),
        skipped,
    }
}

/// `@scope/name@1.2.3` → npm coordinate, when the version is exact.
pub fn npm_coord(spec: &str) -> Option<PackageCoord> {
    let at = spec.get(1..)?.rfind('@')? + 1;
    let (name, version) = (&spec[..at], &spec[at + 1..]);
    exact(version).then(|| PackageCoord {
        ecosystem: "npm".into(),
        name: name.into(),
        version: version.into(),
    })
}

/// `name==1.2.3` or `name@1.2.3` → PyPI coordinate, when the version is exact.
pub fn pypi_coord(spec: &str) -> Option<PackageCoord> {
    let (name, version) = spec.split_once("==").or_else(|| spec.split_once('@'))?;
    let name = name.split('[').next().unwrap_or(name).trim();
    (!name.is_empty() && exact(version)).then(|| PackageCoord {
        ecosystem: "PyPI".into(),
        name: name.into(),
        version: version.trim().into(),
    })
}

fn exact(version: &str) -> bool {
    let v = version.trim();
    !v.is_empty()
        && v.chars().next().is_some_and(|c| c.is_ascii_digit())
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_specs_need_an_exact_version() {
        assert_eq!(
            npm_coord("@modelcontextprotocol/server-filesystem@2025.8.21"),
            Some(PackageCoord {
                ecosystem: "npm".into(),
                name: "@modelcontextprotocol/server-filesystem".into(),
                version: "2025.8.21".into()
            })
        );
        assert_eq!(npm_coord("left-pad@1.3.0").unwrap().name, "left-pad");
        assert_eq!(npm_coord("left-pad@latest"), None);
        assert_eq!(npm_coord("left-pad@^1.3.0"), None);
        assert_eq!(npm_coord("left-pad"), None);
        assert_eq!(
            pypi_coord("mcp-server-git==0.6.2").unwrap().ecosystem,
            "PyPI"
        );
        assert_eq!(pypi_coord("mcp[cli]==1.2.0").unwrap().name, "mcp");
        assert_eq!(pypi_coord("mcp-server-git"), None);
        assert_eq!(pypi_coord("x>=1.0"), None);
    }
}
