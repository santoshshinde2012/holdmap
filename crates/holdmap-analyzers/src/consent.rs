//! Approvals, latest results and review states, behind three narrow traits so the service can
//! be tested in memory and stored on disk in production ([`crate::store::FileStore`]).

use crate::finding::{push_history, AnalyzerResult, FindingState, RunRecord};
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
    /// Whether it may also run on its own when the analysed items change even though it uses
    /// the network. Off until the user turns it on; reset by every new approval.
    #[serde(default)]
    pub background_network: bool,
    /// MCP analyzers: the pin of the tool list the user reviewed.
    #[serde(default)]
    pub tools_sha256: Option<String>,
    /// MCP analyzers on another machine: the user explicitly allowed sending metadata there.
    #[serde(default)]
    pub remote_opt_in: bool,
}

/// A user's decision about one finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    /// Reviewed or snoozed.
    pub state: FindingState,
    /// For snoozes: when it reopens (Unix epoch ms).
    #[serde(default)]
    pub until_ms: Option<u64>,
    /// Why, in the user's words (capped and secret-masked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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

/// Manifests the user registered themselves (`holdmap analyzers add`), as their exact text.
pub trait UserManifestStore: Send + Sync {
    /// Every registered manifest's text.
    fn user_manifests(&self) -> Vec<String>;
    /// Save (or replace) the manifest for `id`.
    fn add_user_manifest(&self, id: &str, text: &str) -> io::Result<()>;
    /// Remove it; `true` when one existed.
    fn remove_user_manifest(&self, id: &str) -> io::Result<bool>;
}

/// The latest tool listing of each MCP analyzer (what the user reviews before approving, and
/// what a changed server listed instead).
pub trait ToolPinStore: Send + Sync {
    /// The listing in `slot` for `analyzer`.
    fn tool_snapshot(
        &self,
        analyzer: &str,
        slot: ToolSlot,
    ) -> Option<crate::mcp_tools::ToolSnapshot>;
    /// Replace (or with `None`, forget) it.
    fn set_tool_snapshot(
        &self,
        analyzer: &str,
        slot: ToolSlot,
        snapshot: Option<&crate::mcp_tools::ToolSnapshot>,
    ) -> io::Result<()>;
}

/// Which tool listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolSlot {
    /// The most recent listing.
    Latest,
    /// The listing the user approved.
    Approved,
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
    /// Past runs of `analyzer`, newest first.
    fn history(&self, _analyzer: &str) -> Vec<RunRecord> {
        Vec::new()
    }
    /// Add a run to the front of its analyzer's history (capped).
    fn append_history(&self, _record: &RunRecord) -> io::Result<()> {
        Ok(())
    }
}

/// Review states by finding fingerprint.
pub trait ReviewStore: Send + Sync {
    /// All reviews.
    fn reviews(&self) -> BTreeMap<String, Review>;
    /// Set (or with `None`, clear) the review of one fingerprint.
    fn set_review(&self, fingerprint: &str, review: Option<Review>) -> io::Result<()>;
}

/// Which secret names the user stored for each analyzer (names only, never values), so lists
/// and previews don't have to open the keychain.
pub trait SecretIndex: Send + Sync {
    /// Names stored for `analyzer`.
    fn secret_names(&self, analyzer: &str) -> Vec<String>;
    /// Record that `name` is stored (`true`) or deleted (`false`).
    fn mark_secret(&self, analyzer: &str, name: &str, stored: bool) -> io::Result<()>;
}

/// Add or remove `name` in a sorted list.
pub(crate) fn mark_in(names: &mut Vec<String>, name: &str, stored: bool) {
    names.retain(|n| n != name);
    if stored {
        names.push(name.into());
        names.sort();
    }
}

/// In-memory stores for tests and the browser demo.
#[derive(Debug, Default)]
pub struct MemoryStore {
    grants: Mutex<BTreeMap<String, Grant>>,
    results: Mutex<BTreeMap<String, AnalyzerResult>>,
    reviews: Mutex<BTreeMap<String, Review>>,
    secrets: Mutex<BTreeMap<String, Vec<String>>>,
    catalogs: Mutex<BTreeMap<u8, crate::catalog::SignedCatalog>>,
    manifests: Mutex<BTreeMap<String, String>>,
    tools: Mutex<BTreeMap<(String, ToolSlot), crate::mcp_tools::ToolSnapshot>>,
    history: Mutex<BTreeMap<String, Vec<RunRecord>>>,
    trust: Mutex<BTreeMap<String, crate::trust::TrustRecord>>,
    schedule: Mutex<Option<crate::schedule::Schedule>>,
}

impl crate::schedule::ScheduleStore for MemoryStore {
    fn schedule(&self) -> crate::schedule::Schedule {
        self.schedule.lock().unwrap().clone().unwrap_or_default()
    }
    fn set_schedule(&self, schedule: &crate::schedule::Schedule) -> io::Result<()> {
        *self.schedule.lock().unwrap() = Some(schedule.clone());
        Ok(())
    }
}

impl crate::trust::TrustStore for MemoryStore {
    fn trust_records(&self) -> BTreeMap<String, crate::trust::TrustRecord> {
        self.trust.lock().unwrap().clone()
    }
    fn set_trust_records(
        &self,
        records: &BTreeMap<String, crate::trust::TrustRecord>,
    ) -> io::Result<()> {
        *self.trust.lock().unwrap() = records.clone();
        Ok(())
    }
}

impl ToolPinStore for MemoryStore {
    fn tool_snapshot(
        &self,
        analyzer: &str,
        slot: ToolSlot,
    ) -> Option<crate::mcp_tools::ToolSnapshot> {
        self.tools
            .lock()
            .unwrap()
            .get(&(analyzer.to_string(), slot))
            .cloned()
    }
    fn set_tool_snapshot(
        &self,
        analyzer: &str,
        slot: ToolSlot,
        snapshot: Option<&crate::mcp_tools::ToolSnapshot>,
    ) -> io::Result<()> {
        let mut all = self.tools.lock().unwrap();
        let key = (analyzer.to_string(), slot);
        match snapshot {
            Some(s) => all.insert(key, s.clone()),
            None => all.remove(&key),
        };
        Ok(())
    }
}

impl UserManifestStore for MemoryStore {
    fn user_manifests(&self) -> Vec<String> {
        self.manifests.lock().unwrap().values().cloned().collect()
    }
    fn add_user_manifest(&self, id: &str, text: &str) -> io::Result<()> {
        self.manifests
            .lock()
            .unwrap()
            .insert(id.into(), text.into());
        Ok(())
    }
    fn remove_user_manifest(&self, id: &str) -> io::Result<bool> {
        Ok(self.manifests.lock().unwrap().remove(id).is_some())
    }
}

impl crate::catalog::CatalogStore for MemoryStore {
    fn catalog(&self, slot: crate::catalog::CatalogSlot) -> Option<crate::catalog::SignedCatalog> {
        self.catalogs.lock().unwrap().get(&(slot as u8)).cloned()
    }
    fn set_catalog(
        &self,
        slot: crate::catalog::CatalogSlot,
        catalog: Option<&crate::catalog::SignedCatalog>,
    ) -> io::Result<()> {
        let mut all = self.catalogs.lock().unwrap();
        match catalog {
            Some(c) => all.insert(slot as u8, c.clone()),
            None => all.remove(&(slot as u8)),
        };
        Ok(())
    }
}

impl SecretIndex for MemoryStore {
    fn secret_names(&self, analyzer: &str) -> Vec<String> {
        self.secrets
            .lock()
            .unwrap()
            .get(analyzer)
            .cloned()
            .unwrap_or_default()
    }
    fn mark_secret(&self, analyzer: &str, name: &str, stored: bool) -> io::Result<()> {
        let mut all = self.secrets.lock().unwrap();
        mark_in(all.entry(analyzer.into()).or_default(), name, stored);
        Ok(())
    }
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
    fn history(&self, analyzer: &str) -> Vec<RunRecord> {
        self.history
            .lock()
            .unwrap()
            .get(analyzer)
            .cloned()
            .unwrap_or_default()
    }
    fn append_history(&self, record: &RunRecord) -> io::Result<()> {
        let mut all = self.history.lock().unwrap();
        push_history(
            all.entry(record.analyzer.clone()).or_default(),
            record.clone(),
        );
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
            reason: None,
        };
        assert_eq!(r.effective(99), FindingState::Snoozed);
        assert_eq!(r.effective(100), FindingState::Open);
        let reviewed = Review {
            state: FindingState::Reviewed,
            until_ms: None,
            reason: None,
        };
        assert_eq!(reviewed.effective(u64::MAX), FindingState::Reviewed);
    }
}
