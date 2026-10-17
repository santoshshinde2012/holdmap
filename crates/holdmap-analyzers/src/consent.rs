//! Approvals, latest results and review states, behind three narrow traits so the service can
//! be tested in memory and stored on disk in production ([`crate::store::FileStore`]).

use crate::finding::{AnalyzerResult, FindingState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;

/// The user's approval of one analyzer, pinned to exactly what they saw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    /// Analyzer id.
    pub analyzer: String,
    /// SHA-256 of the manifest bytes that were approved.
    pub manifest_sha256: String,
    /// The approved executable (CLI analyzers).
    #[serde(default)]
    pub binary: Option<PathBuf>,
    /// Its SHA-256 at approval.
    #[serde(default)]
    pub binary_sha256: Option<String>,
    /// Its reported version at approval.
    #[serde(default)]
    pub version: Option<String>,
    /// When it was approved (Unix epoch ms).
    pub approved_at_ms: u64,
}

/// A user's decision about one finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    /// Reviewed or snoozed.
    pub state: FindingState,
    /// For snoozes: when it reopens (Unix epoch ms).
    #[serde(default)]
    pub until_ms: Option<u64>,
}

impl Review {
    /// The state in effect at `now_ms` (an expired snooze is open again).
    pub fn effective(&self, now_ms: u64) -> FindingState {
        match (self.state, self.until_ms) {
            (FindingState::Snoozed, Some(until)) if until <= now_ms => FindingState::Open,
            (state, _) => state,
        }
    }
}

/// Approvals.
pub trait ConsentStore: Send + Sync {
    /// The approval for `analyzer`, if any.
    fn grant(&self, analyzer: &str) -> Option<Grant>;
    /// Save an approval (replacing an older one).
    fn record(&self, grant: Grant) -> io::Result<()>;
    /// Remove an approval; `true` when one existed.
    fn revoke(&self, analyzer: &str) -> io::Result<bool>;
}

/// Latest results.
pub trait ResultStore: Send + Sync {
    /// The latest result of `analyzer`.
    fn latest(&self, analyzer: &str) -> Option<AnalyzerResult>;
    /// Replace the latest result.
    fn save(&self, result: &AnalyzerResult) -> io::Result<()>;
    /// Forget the result of `analyzer`.
    fn forget(&self, analyzer: &str) -> io::Result<()>;
}

/// Review states by finding fingerprint.
pub trait ReviewStore: Send + Sync {
    /// All reviews.
    fn reviews(&self) -> BTreeMap<String, Review>;
    /// Set (or with `None`, clear) the review of one fingerprint.
    fn set_review(&self, fingerprint: &str, review: Option<Review>) -> io::Result<()>;
}

/// In-memory stores for tests and the browser demo.
#[derive(Debug, Default)]
pub struct MemoryStore {
    grants: Mutex<BTreeMap<String, Grant>>,
    results: Mutex<BTreeMap<String, AnalyzerResult>>,
    reviews: Mutex<BTreeMap<String, Review>>,
}

impl ConsentStore for MemoryStore {
    fn grant(&self, analyzer: &str) -> Option<Grant> {
        self.grants.lock().unwrap().get(analyzer).cloned()
    }
    fn record(&self, grant: Grant) -> io::Result<()> {
        self.grants
            .lock()
            .unwrap()
            .insert(grant.analyzer.clone(), grant);
        Ok(())
    }
    fn revoke(&self, analyzer: &str) -> io::Result<bool> {
        Ok(self.grants.lock().unwrap().remove(analyzer).is_some())
    }
}

impl ResultStore for MemoryStore {
    fn latest(&self, analyzer: &str) -> Option<AnalyzerResult> {
        self.results.lock().unwrap().get(analyzer).cloned()
    }
    fn save(&self, result: &AnalyzerResult) -> io::Result<()> {
        self.results
            .lock()
            .unwrap()
            .insert(result.analyzer.clone(), result.clone());
        Ok(())
    }
    fn forget(&self, analyzer: &str) -> io::Result<()> {
        self.results.lock().unwrap().remove(analyzer);
        Ok(())
    }
}

impl ReviewStore for MemoryStore {
    fn reviews(&self) -> BTreeMap<String, Review> {
        self.reviews.lock().unwrap().clone()
    }
    fn set_review(&self, fingerprint: &str, review: Option<Review>) -> io::Result<()> {
        let mut reviews = self.reviews.lock().unwrap();
        match review {
            Some(r) => reviews.insert(fingerprint.into(), r),
            None => reviews.remove(fingerprint),
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_expired_snooze_reopens() {
        let r = Review {
            state: FindingState::Snoozed,
            until_ms: Some(100),
        };
        assert_eq!(r.effective(99), FindingState::Snoozed);
        assert_eq!(r.effective(100), FindingState::Open);
        let reviewed = Review {
            state: FindingState::Reviewed,
            until_ms: None,
        };
        assert_eq!(reviewed.effective(u64::MAX), FindingState::Reviewed);
    }
}
