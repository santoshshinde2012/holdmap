//! The first-party analyzer catalog: a JSON file signed with holdmap's catalog key
//! (minisign, Ed25519). The copy compiled into holdmap is verified like any other, and a newer
//! one (fetched on request or imported from a file) is only staged: the user reviews a diff and
//! accepts it before it applies. Approvals stay pinned to each manifest, so an accepted change
//! to an analyzer the user turned on still asks again before it runs.

use crate::manifest::{AnalyzerManifest, ManifestError, RunnerKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io;
use thiserror::Error;

/// holdmap's catalog public key (minisign format).
pub const PUBLIC_KEY: &str = include_str!("../catalog/holdmap-catalog.pub");
/// Where `holdmap analyzers catalog fetch` downloads a newer catalog from.
pub const CATALOG_URL: &str =
    "https://raw.githubusercontent.com/santoshshinde2012/holdmap/main/crates/holdmap-analyzers/catalog/analyzers.json";
/// Catalog file format version.
pub const CATALOG_SCHEMA: u32 = 1;
/// Largest catalog file accepted.
pub const MAX_CATALOG_BYTES: usize = 512 * 1024;

const BUNDLED_JSON: &str = include_str!("../catalog/analyzers.json");
const BUNDLED_SIG: &str = include_str!("../catalog/analyzers.json.minisig");

/// The manifests the catalog file is generated from, in catalog order (SkillSpector first).
pub const SOURCES: &[&str] = &[
    include_str!("../manifests/nvidia.skillspector.toml"),
    include_str!("../manifests/holdmap.osv.toml"),
];

/// Built-in analyzers holdmap implements; a catalog can't introduce new ones.
const BUILTINS: &[&str] = &["holdmap.osv"];

/// A catalog file plus its detached signature, as stored and transported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedCatalog {
    /// The catalog JSON.
    pub json: String,
    /// The minisign signature over it.
    pub signature: String,
}

impl SignedCatalog {
    /// The copy compiled into holdmap.
    pub fn bundled() -> Self {
        Self {
            json: BUNDLED_JSON.into(),
            signature: BUNDLED_SIG.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogFile {
    schema: u32,
    version: u32,
    published: String,
    analyzers: Vec<CatalogEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogEntry {
    manifest: String,
}

/// A verified catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct Catalog {
    /// Increases with every published change.
    pub version: u32,
    /// Publication date (YYYY-MM-DD).
    pub published: String,
    /// Validated manifests, in catalog order.
    pub manifests: Vec<AnalyzerManifest>,
}

/// Why a catalog was refused.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CatalogError {
    /// The signature is missing, malformed, from another key or doesn't match.
    #[error("the catalog signature is not valid for holdmap's catalog key")]
    Signature,
    /// It couldn't be downloaded or read.
    #[error("{0}")]
    Unavailable(String),
    /// The file isn't a catalog.
    #[error("the catalog file is malformed: {0}")]
    Malformed(String),
    /// An entry failed manifest validation.
    #[error("catalog entry {index}: {error}")]
    Manifest {
        /// Position in the file.
        index: usize,
        /// Why.
        error: ManifestError,
    },
    /// It is not newer than the catalog in use.
    #[error("this catalog (version {got}) is not newer than the one in use (version {have})")]
    NotNewer {
        /// In use.
        have: u32,
        /// Offered.
        got: u32,
    },
}

/// The trusted comment that binds a signature to one catalog version.
pub fn trusted_comment(version: u32) -> String {
    format!("holdmap analyzer catalog v{version}")
}

/// Verify `signed` with `public_key` and parse it.
pub fn verify(signed: &SignedCatalog, public_key: &str) -> Result<Catalog, CatalogError> {
    if signed.json.len() > MAX_CATALOG_BYTES {
        return Err(CatalogError::Malformed("too large".into()));
    }
    let key = minisign_verify::PublicKey::decode(public_key.trim())
        .map_err(|_| CatalogError::Signature)?;
    let sig = minisign_verify::Signature::decode(signed.signature.trim())
        .map_err(|_| CatalogError::Signature)?;
    key.verify(signed.json.as_bytes(), &sig, false)
        .map_err(|_| CatalogError::Signature)?;
    let file: CatalogFile =
        serde_json::from_str(&signed.json).map_err(|e| CatalogError::Malformed(e.to_string()))?;
    if file.schema != CATALOG_SCHEMA {
        return Err(CatalogError::Malformed(format!(
            "schema {} is not supported",
            file.schema
        )));
    }
    if sig.trusted_comment() != trusted_comment(file.version) {
        return Err(CatalogError::Signature);
    }
    let mut manifests: Vec<AnalyzerManifest> = Vec::new();
    for (index, entry) in file.analyzers.iter().enumerate() {
        let m = AnalyzerManifest::parse_bundled(&entry.manifest)
            .map_err(|error| CatalogError::Manifest { index, error })?;
        if m.runner.kind == RunnerKind::Builtin && !BUILTINS.contains(&m.id.as_str()) {
            return Err(CatalogError::Malformed(format!(
                "{} claims to be built in, but holdmap has no such built-in",
                m.id
            )));
        }
        if manifests.iter().any(|o| o.id == m.id) {
            return Err(CatalogError::Malformed(format!("{} is listed twice", m.id)));
        }
        manifests.push(m);
    }
    Ok(Catalog {
        version: file.version,
        published: file.published,
        manifests,
    })
}

/// The verified catalog compiled into holdmap. A bad bundled catalog is a build bug.
pub fn bundled_catalog() -> Catalog {
    verify(&SignedCatalog::bundled(), PUBLIC_KEY).expect("the bundled catalog is signed")
}

/// Every bundled manifest, in catalog order.
pub fn bundled() -> Vec<AnalyzerManifest> {
    bundled_catalog().manifests
}

/// The catalog JSON for `version` built from `manifests` (the signing tool and the freshness
/// test use this, so the signed file always matches the manifests in the repository).
pub fn render(version: u32, published: &str, manifests: &[&str]) -> String {
    let file = CatalogFile {
        schema: CATALOG_SCHEMA,
        version,
        published: published.into(),
        analyzers: manifests
            .iter()
            .map(|m| CatalogEntry {
                manifest: (*m).into(),
            })
            .collect(),
    };
    let mut text = serde_json::to_string_pretty(&file).expect("serialisable");
    text.push('\n');
    text
}

/// The version and date inside a catalog JSON, without verifying it.
pub fn peek_version(json: &str) -> Option<(u32, String)> {
    let file: CatalogFile = serde_json::from_str(json).ok()?;
    Some((file.version, file.published))
}

/// What a catalog update changes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogDiff {
    /// Version in use.
    pub from_version: u32,
    /// Offered version.
    pub to_version: u32,
    /// Its publication date.
    pub published: String,
    /// New analyzers (ids and names).
    pub added: Vec<String>,
    /// Analyzers it removes.
    pub removed: Vec<String>,
    /// Analyzers whose manifest changes.
    pub changed: Vec<EntryChange>,
}

impl CatalogDiff {
    /// No analyzer changes.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

/// One analyzer's manifest changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryChange {
    /// Analyzer id.
    pub id: String,
    /// Field-level changes.
    pub fields: Vec<FieldChange>,
}

/// One field's change, in dotted form (`permissions.network`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldChange {
    /// Dotted field name.
    pub field: String,
    /// Before (empty when added).
    pub before: String,
    /// After (empty when removed).
    pub after: String,
}

/// The changes from `old` to `new`.
pub fn diff(old: &Catalog, new: &Catalog) -> CatalogDiff {
    let label = |m: &AnalyzerManifest| format!("{} ({})", m.name, m.id);
    let mut out = CatalogDiff {
        from_version: old.version,
        to_version: new.version,
        published: new.published.clone(),
        ..Default::default()
    };
    for m in &new.manifests {
        match old.manifests.iter().find(|o| o.id == m.id) {
            None => out.added.push(label(m)),
            Some(o) if o.sha256 != m.sha256 => {
                let (a, b) = (flatten(o), flatten(m));
                let mut fields = Vec::new();
                for key in a.keys().chain(b.keys().filter(|k| !a.contains_key(*k))) {
                    let before = a.get(key).cloned().unwrap_or_default();
                    let after = b.get(key).cloned().unwrap_or_default();
                    if before != after {
                        fields.push(FieldChange {
                            field: key.clone(),
                            before,
                            after,
                        });
                    }
                }
                if fields.is_empty() {
                    fields.push(FieldChange {
                        field: "comments or formatting".into(),
                        before: String::new(),
                        after: String::new(),
                    });
                }
                out.changed.push(EntryChange {
                    id: m.id.clone(),
                    fields,
                });
            }
            Some(_) => {}
        }
    }
    for o in &old.manifests {
        if !new.manifests.iter().any(|m| m.id == o.id) {
            out.removed.push(label(o));
        }
    }
    out
}

fn flatten(m: &AnalyzerManifest) -> BTreeMap<String, String> {
    fn walk(prefix: &str, v: &Value, out: &mut BTreeMap<String, String>) {
        match v {
            Value::Object(map) => {
                for (k, v) in map {
                    let key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    walk(&key, v, out);
                }
            }
            Value::Array(items) => {
                let text: Vec<String> = items
                    .iter()
                    .map(|i| match i {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect();
                out.insert(prefix.into(), text.join(", "));
            }
            Value::Null => {}
            Value::String(s) => {
                out.insert(prefix.into(), s.clone());
            }
            other => {
                out.insert(prefix.into(), other.to_string());
            }
        }
    }
    let mut out = BTreeMap::new();
    let mut v = serde_json::to_value(m).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut v {
        map.remove("sha256");
    }
    walk("", &v, &mut out);
    out
}

/// Where a staged or accepted catalog is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSlot {
    /// Verified, waiting for the user's review.
    Pending,
    /// Accepted by the user; used when newer than the bundled one.
    Accepted,
}

/// Staged and accepted catalogs.
pub trait CatalogStore: Send + Sync {
    /// The catalog in `slot`.
    fn catalog(&self, slot: CatalogSlot) -> Option<SignedCatalog>;
    /// Replace (or with `None`, clear) the catalog in `slot`.
    fn set_catalog(&self, slot: CatalogSlot, catalog: Option<&SignedCatalog>) -> io::Result<()>;
}

/// Which catalog is in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogStatus {
    /// Its version.
    pub version: u32,
    /// Its publication date.
    pub published: String,
    /// `true` when it is a newer catalog the user accepted, `false` for the built-in copy.
    pub accepted: bool,
    /// A verified update waiting for review, with what it changes.
    pub pending: Option<CatalogDiff>,
}

/// The catalog in use: an accepted one when it verifies and is newer than the built-in copy.
pub fn load_active(store: &dyn CatalogStore, public_key: &str) -> (Catalog, bool) {
    let bundled = bundled_catalog();
    match store
        .catalog(CatalogSlot::Accepted)
        .and_then(|s| verify(&s, public_key).ok())
    {
        Some(c) if c.version > bundled.version => (c, true),
        _ => (bundled, false),
    }
}

/// Verify `offered` and stage it for review when it is newer than `active`.
pub fn stage(
    store: &dyn CatalogStore,
    public_key: &str,
    active: &Catalog,
    offered: &SignedCatalog,
) -> Result<CatalogDiff, CatalogError> {
    let new = verify(offered, public_key)?;
    if new.version <= active.version {
        return Err(CatalogError::NotNewer {
            have: active.version,
            got: new.version,
        });
    }
    store
        .set_catalog(CatalogSlot::Pending, Some(offered))
        .map_err(|e| CatalogError::Malformed(format!("couldn't save it: {e}")))?;
    Ok(diff(active, &new))
}

/// The staged update's diff against `active`, if one is waiting and still verifies.
pub fn pending(
    store: &dyn CatalogStore,
    public_key: &str,
    active: &Catalog,
) -> Option<CatalogDiff> {
    let staged = verify(&store.catalog(CatalogSlot::Pending)?, public_key).ok()?;
    (staged.version > active.version).then(|| diff(active, &staged))
}

/// Accept the staged update: verify it again, keep it, and return it.
pub fn accept(
    store: &dyn CatalogStore,
    public_key: &str,
    active: &Catalog,
) -> Result<Catalog, CatalogError> {
    let staged = store
        .catalog(CatalogSlot::Pending)
        .ok_or_else(|| CatalogError::Malformed("no catalog update is waiting".into()))?;
    let new = verify(&staged, public_key)?;
    if new.version <= active.version {
        return Err(CatalogError::NotNewer {
            have: active.version,
            got: new.version,
        });
    }
    let save = |slot, c| {
        store
            .set_catalog(slot, c)
            .map_err(|e| CatalogError::Malformed(format!("couldn't save it: {e}")))
    };
    save(CatalogSlot::Accepted, Some(&staged))?;
    save(CatalogSlot::Pending, None)?;
    Ok(new)
}

/// Downloads a catalog (only when the user asks).
pub trait CatalogSource: Send + Sync {
    /// Fetch the catalog and its signature.
    fn fetch(&self) -> Result<SignedCatalog, String>;
}

/// [`CATALOG_URL`] over HTTPS.
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpsCatalogSource;

impl CatalogSource for HttpsCatalogSource {
    #[cfg(feature = "osv-http")]
    fn fetch(&self) -> Result<SignedCatalog, String> {
        let get = |url: &str| -> Result<String, String> {
            let agent: ureq::Agent = ureq::Agent::config_builder()
                .timeout_global(Some(std::time::Duration::from_secs(20)))
                .build()
                .into();
            let mut resp = agent
                .get(url)
                .call()
                .map_err(|e| format!("couldn't download the catalog: {e}"))?;
            resp.body_mut()
                .with_config()
                .limit(MAX_CATALOG_BYTES as u64)
                .read_to_string()
                .map_err(|e| format!("couldn't read the catalog: {e}"))
        };
        Ok(SignedCatalog {
            json: get(CATALOG_URL)?,
            signature: get(&format!("{CATALOG_URL}.minisig"))?,
        })
    }

    #[cfg(not(feature = "osv-http"))]
    fn fetch(&self) -> Result<SignedCatalog, String> {
        Err("this build of holdmap can't download catalogs; import a catalog file instead".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_catalog_is_signed_and_lists_skillspector_first() {
        let c = bundled_catalog();
        assert_eq!(c.manifests[0].id, "nvidia.skillspector");
        assert_eq!(c.manifests.len(), SOURCES.len());
        assert!(c
            .manifests
            .iter()
            .all(|m| !m.permissions.spawns_mcp_servers));
    }

    #[test]
    fn the_signed_file_matches_the_manifests_in_the_repository() {
        let (version, published) = peek_version(BUNDLED_JSON).unwrap();
        assert_eq!(
            BUNDLED_JSON,
            render(version, &published, SOURCES),
            "manifests changed: re-sign with `cargo run -p holdmap-analyzers --example sign_catalog -- sign <key> <version+1> <date>`"
        );
    }

    #[test]
    fn a_tampered_catalog_is_refused() {
        let mut s = SignedCatalog::bundled();
        s.json = s.json.replace("api.osv.dev", "evil.example");
        assert_eq!(verify(&s, PUBLIC_KEY), Err(CatalogError::Signature));
        let mut s = SignedCatalog::bundled();
        s.signature = "garbage".into();
        assert_eq!(verify(&s, PUBLIC_KEY), Err(CatalogError::Signature));
    }
}
