//! Manifest parsers (Strategy + registry): each one knows one manifest format and extracts the
//! project name and dependency words. New ecosystems are added by registering a parser.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

/// Name + dependency words extracted from a manifest.
pub type Parsed = (Option<String>, Vec<String>);

/// Understands one manifest file (e.g. `package.json`).
pub trait ManifestParser: Send + Sync {
    /// File name that marks a project root.
    fn marker(&self) -> &'static str;
    /// Parse the manifest text found at `root/marker`.
    fn parse(&self, root: &Path, text: &str) -> Parsed;
}

/// Ordered parsers; the first marker found while walking up from a cwd wins.
pub struct ManifestRegistry {
    parsers: Vec<Box<dyn ManifestParser>>,
}

impl std::fmt::Debug for ManifestRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.markers()).finish()
    }
}

impl Default for ManifestRegistry {
    fn default() -> Self {
        Self::empty()
            .with(NodeManifest)
            .with(CargoManifest)
            .with(PyProjectManifest)
            .with(GoModManifest)
            .with(Generic("Gemfile"))
            .with(ComposerManifest)
            .with(Generic("pom.xml"))
            .with(Generic("build.gradle"))
            .with(Generic("build.gradle.kts"))
            .with(Generic("mix.exs"))
            .with(Generic("deno.json"))
            .with(DjangoManagePy)
            .with(Generic("requirements.txt"))
    }
}

impl ManifestRegistry {
    pub fn empty() -> Self {
        Self {
            parsers: Vec::new(),
        }
    }

    pub fn with(mut self, p: impl ManifestParser + 'static) -> Self {
        self.parsers.push(Box::new(p));
        self
    }

    /// The shared default registry.
    pub fn global() -> &'static ManifestRegistry {
        static R: OnceLock<ManifestRegistry> = OnceLock::new();
        R.get_or_init(ManifestRegistry::default)
    }

    pub fn markers(&self) -> Vec<&'static str> {
        self.parsers.iter().map(|p| p.marker()).collect()
    }

    /// The first parser whose marker exists in `dir`.
    pub fn find(&self, dir: &Path) -> Option<&dyn ManifestParser> {
        self.parsers
            .iter()
            .find(|p| dir.join(p.marker()).is_file())
            .map(|b| b.as_ref())
    }
}

pub struct NodeManifest;
impl ManifestParser for NodeManifest {
    fn marker(&self) -> &'static str {
        "package.json"
    }
    fn parse(&self, _root: &Path, text: &str) -> Parsed {
        parse_package_json(text)
    }
}

pub struct CargoManifest;
impl ManifestParser for CargoManifest {
    fn marker(&self) -> &'static str {
        "Cargo.toml"
    }
    fn parse(&self, _root: &Path, text: &str) -> Parsed {
        (toml_name(text, "[package]"), toml_deps(text))
    }
}

pub struct PyProjectManifest;
impl ManifestParser for PyProjectManifest {
    fn marker(&self) -> &'static str {
        "pyproject.toml"
    }
    fn parse(&self, root: &Path, text: &str) -> Parsed {
        let name = toml_name(text, "[project]").or_else(|| toml_name(text, "[tool.poetry]"));
        let mut deps = text_deps(text);
        deps.extend(requirements(root));
        (name, deps)
    }
}

pub struct GoModManifest;
impl ManifestParser for GoModManifest {
    fn marker(&self) -> &'static str {
        "go.mod"
    }
    fn parse(&self, _root: &Path, text: &str) -> Parsed {
        (
            text.lines()
                .find_map(|l| l.strip_prefix("module "))
                .map(|m| m.trim().rsplit('/').next().unwrap_or(m).to_string()),
            text_deps(text),
        )
    }
}

pub struct ComposerManifest;
impl ManifestParser for ComposerManifest {
    fn marker(&self) -> &'static str {
        "composer.json"
    }
    fn parse(&self, _root: &Path, text: &str) -> Parsed {
        let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
        (
            v["name"]
                .as_str()
                .map(|n| n.rsplit('/').next().unwrap_or(n).to_string()),
            json_keys(&v, &["require", "require-dev"]),
        )
    }
}

/// `manage.py` marks a Django project (deps come from requirements.txt next to it).
pub struct DjangoManagePy;
impl ManifestParser for DjangoManagePy {
    fn marker(&self) -> &'static str {
        "manage.py"
    }
    fn parse(&self, root: &Path, text: &str) -> Parsed {
        let mut deps = text_deps(text);
        deps.extend(requirements(root));
        deps.push("django".into());
        (None, deps)
    }
}

/// Any manifest whose words are enough for signature matching.
pub struct Generic(pub &'static str);
impl ManifestParser for Generic {
    fn marker(&self) -> &'static str {
        self.0
    }
    fn parse(&self, _root: &Path, text: &str) -> Parsed {
        (None, text_deps(text))
    }
}

fn requirements(root: &Path) -> Vec<String> {
    fs::read_to_string(root.join("requirements.txt"))
        .map(|r| text_deps(&r))
        .unwrap_or_default()
}

pub(crate) fn json_keys(v: &serde_json::Value, sections: &[&str]) -> Vec<String> {
    sections
        .iter()
        .filter_map(|s| v[*s].as_object())
        .flat_map(|o| o.keys().cloned())
        .collect()
}

fn parse_package_json(text: &str) -> Parsed {
    let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let name = v["name"]
        .as_str()
        .map(|n| n.rsplit('/').next().unwrap_or(n).to_string());
    let mut deps = json_keys(&v, &["dependencies", "devDependencies"]);
    // Scripts reveal the dev command (e.g. "dev": "vite").
    if let Some(scripts) = v["scripts"].as_object() {
        for s in scripts.values().filter_map(|s| s.as_str()) {
            deps.extend(s.split_whitespace().map(|w| format!("script:{w}")));
        }
    }
    (name, deps)
}

pub(crate) fn toml_name(text: &str, section: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_section = l == section;
            continue;
        }
        if in_section {
            if let Some(rest) = l.strip_prefix("name") {
                let v = rest.trim_start().strip_prefix('=')?.trim();
                return Some(v.trim_matches(|c| c == '"' || c == '\'').to_string());
            }
        }
    }
    None
}

fn toml_deps(text: &str) -> Vec<String> {
    let mut in_deps = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_deps = l.contains("dependencies");
            continue;
        }
        if in_deps {
            if let Some((k, _)) = l.split_once('=') {
                out.push(k.trim().to_string());
            }
        }
    }
    out
}

/// Loose dependency extraction: lowercase words of a manifest (good enough for signatures).
fn text_deps(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.'))
        .filter(|w| w.len() > 2)
        .map(|w| w.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl ManifestParser for Fake {
        fn marker(&self) -> &'static str {
            "fake.manifest"
        }
        fn parse(&self, _: &Path, text: &str) -> Parsed {
            (Some(text.trim().into()), vec![])
        }
    }

    #[test]
    fn registry_is_open_for_extension() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("fake.manifest"), "zeta\n").unwrap();
        assert!(ManifestRegistry::default().find(tmp.path()).is_none());
        let r = ManifestRegistry::empty().with(Fake);
        let p = r.find(tmp.path()).unwrap();
        assert_eq!(p.parse(tmp.path(), "zeta\n").0.as_deref(), Some("zeta"));
    }

    #[test]
    fn default_order_prefers_package_json() {
        let m = ManifestRegistry::default().markers();
        assert_eq!(m[0], "package.json");
        assert!(m.contains(&"requirements.txt"));
    }
}
