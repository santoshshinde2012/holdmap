//! On-disk stores under `<holdmap dir>/analyzers/`: `grants.json`, `reviews.json` and
//! `results/<id>.json`. Files are private to the user (0600, directory 0700) and replaced
//! atomically. Results contain only normalised, capped findings.

use crate::consent::{ConsentStore, Grant, ResultStore, Review, ReviewStore};
use crate::finding::AnalyzerResult;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// File-backed stores.
#[derive(Debug)]
pub struct FileStore {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl FileStore {
    /// Stores rooted at `dir` (created on first write).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            lock: Mutex::new(()),
        }
    }

    /// `<holdmap dir>/analyzers`.
    pub fn open_default() -> Self {
        Self::new(holdmap_core::store::Store::default_dir().join("analyzers"))
    }

    /// The root folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn read<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        fs::read(self.dir.join(name))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn write<T: Serialize>(&self, name: &str, value: &T) -> io::Result<()> {
        let path = self.dir.join(name);
        let parent = path.parent().unwrap_or(&self.dir).to_path_buf();
        private_dir(&self.dir)?;
        private_dir(&parent)?;
        let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
        let tmp = parent.join(format!(
            ".{}.{}.tmp",
            path.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id()
        ));
        let result = (|| {
            let mut file = private_file(&tmp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&tmp, &path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }

    fn result_name(analyzer: &str) -> Option<String> {
        analyzer
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-'))
            .then(|| format!("results/{analyzer}.json"))
    }
}

fn private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn private_file(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

impl ConsentStore for FileStore {
    fn grant(&self, analyzer: &str) -> Option<Grant> {
        self.read::<BTreeMap<String, Grant>>("grants.json")
            .remove(analyzer)
    }
    fn record(&self, grant: Grant) -> io::Result<()> {
        let _guard = self.lock.lock().unwrap();
        let mut grants: BTreeMap<String, Grant> = self.read("grants.json");
        grants.insert(grant.analyzer.clone(), grant);
        self.write("grants.json", &grants)
    }
    fn revoke(&self, analyzer: &str) -> io::Result<bool> {
        let _guard = self.lock.lock().unwrap();
        let mut grants: BTreeMap<String, Grant> = self.read("grants.json");
        let existed = grants.remove(analyzer).is_some();
        self.write("grants.json", &grants)?;
        Ok(existed)
    }
}

impl ResultStore for FileStore {
    fn latest(&self, analyzer: &str) -> Option<AnalyzerResult> {
        let name = Self::result_name(analyzer)?;
        fs::read(self.dir.join(name))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
    }
    fn save(&self, result: &AnalyzerResult) -> io::Result<()> {
        let name = Self::result_name(&result.analyzer)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad analyzer id"))?;
        self.write(&name, result)
    }
    fn forget(&self, analyzer: &str) -> io::Result<()> {
        match Self::result_name(analyzer).map(|n| fs::remove_file(self.dir.join(n))) {
            Some(Err(e)) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

impl crate::consent::SecretIndex for FileStore {
    fn secret_names(&self, analyzer: &str) -> Vec<String> {
        self.read::<BTreeMap<String, Vec<String>>>("secret-names.json")
            .remove(analyzer)
            .unwrap_or_default()
    }
    fn mark_secret(&self, analyzer: &str, name: &str, stored: bool) -> io::Result<()> {
        let _guard = self.lock.lock().unwrap();
        let mut all: BTreeMap<String, Vec<String>> = self.read("secret-names.json");
        crate::consent::mark_in(all.entry(analyzer.into()).or_default(), name, stored);
        all.retain(|_, names| !names.is_empty());
        self.write("secret-names.json", &all)
    }
}

impl ReviewStore for FileStore {
    fn reviews(&self) -> BTreeMap<String, Review> {
        self.read("reviews.json")
    }
    fn set_review(&self, fingerprint: &str, review: Option<Review>) -> io::Result<()> {
        let _guard = self.lock.lock().unwrap();
        let mut reviews: BTreeMap<String, Review> = self.read("reviews.json");
        match review {
            Some(r) => reviews.insert(fingerprint.into(), r),
            None => reviews.remove(fingerprint),
        };
        self.write("reviews.json", &reviews)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{FindingState, RunStatus};

    #[test]
    fn grants_results_and_reviews_round_trip_privately() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().join("analyzers"));
        let grant = Grant {
            analyzer: "a.b".into(),
            manifest_sha256: "m".into(),
            binary: None,
            binary_sha256: None,
            version: None,
            approved_at_ms: 1,
        };
        store.record(grant.clone()).unwrap();
        assert_eq!(store.grant("a.b"), Some(grant));
        assert!(store.revoke("a.b").unwrap());
        assert!(!store.revoke("a.b").unwrap());
        let result = AnalyzerResult {
            analyzer: "a.b".into(),
            analyzer_version: None,
            run_at_ms: 2,
            status: RunStatus::Complete,
            coverage: None,
            subjects: vec![],
            findings: vec![],
            details: vec![],
            notes: vec![],
            input_signature: None,
        };
        store.save(&result).unwrap();
        assert_eq!(store.latest("a.b"), Some(result));
        store.forget("a.b").unwrap();
        assert_eq!(store.latest("a.b"), None);
        store
            .set_review(
                "fp",
                Some(Review {
                    state: FindingState::Reviewed,
                    until_ms: None,
                }),
            )
            .unwrap();
        assert_eq!(store.reviews().len(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&dir.path().join("analyzers/reviews.json")), 0o600);
            assert_eq!(mode(&dir.path().join("analyzers")), 0o700);
        }
    }

    #[test]
    fn analyzer_ids_cannot_escape_the_results_folder() {
        assert_eq!(FileStore::result_name("../../x"), None);
        assert_eq!(FileStore::result_name("a/b"), None);
    }
}
