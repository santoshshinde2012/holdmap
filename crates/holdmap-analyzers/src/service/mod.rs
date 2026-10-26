//! [`AnalyzerService`]: the one entry point surfaces use. It lists analyzers with their
//! consent state, builds the exact data preview, records approvals pinned to the manifest and
//! binary, runs only approved and unchanged analyzers, normalises their output and applies
//! review states.

use crate::catalog::{
    Catalog, CatalogDiff, CatalogError, CatalogSource, CatalogStatus, CatalogStore, SignedCatalog,
};
use crate::cli_runner::{note_limits_coverage, CliRunner};
use crate::consent::{
    ConsentStore, Grant, ResultStore, Review, ReviewStore, SecretIndex, ToolPinStore, ToolSlot,
    UserManifestStore,
};
use crate::finding::{
    clean_id, clean_refs, clean_relative_path, clean_text, fingerprint, hex, AnalyzerResult,
    Detail, Finding, FindingState, Location, RunChanges, RunRecord, RunStatus, Severity, Subject,
    ToolCalls, MAX_DETAIL, MAX_FINDINGS_PER_SUBJECT, MAX_FIX, MAX_NOTE, MAX_TITLE,
};
use crate::input::{AnalyzerInput, InputProjector, InventoryProjector};
use crate::install::{InstallError, InstallEvent, InstallPlan, InstallRecord, Installer};
use crate::manifest::{
    AnalyzerManifest, ManifestError, OutputFormat, Permissions, RunnerKind, Transport, OUTPUT,
    SUBJECT_DIR,
};
use crate::mcp_runner::McpAnalyzerRunner;
use crate::osv::{request_bodies, OsvParser, OsvRunner};
use crate::parse::{for_format, OutputParser, Parsed};
use crate::probe::{BinaryProbe, SystemProbe};
use crate::runner::{
    AnalyzerRunner, CancelFlag, ProgressSink, RawOutput, RunContext, RunError, RunEvent, RunStep,
    SandboxLevel, ENV_NAMES,
};
use crate::secrets::{check_value, Secret, SecretError, SecretStore};
use holdmap_core::agents::AgentsReport;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

mod mcp;
pub use mcp::{ConfiguredServer, McpDraft, McpView, ToolListing, ToolReport};

/// Where an analyzer stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalyzerState {
    /// Its program was not found.
    NotInstalled,
    /// Off: it has never been approved, or approval was revoked.
    NotApproved,
    /// Approved and unchanged.
    Approved,
    /// Something changed since approval; it won't run until approved again.
    Changed,
}

/// Open finding counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeverityCounts {
    /// Critical.
    pub critical: usize,
    /// High.
    pub high: usize,
    /// Medium.
    pub medium: usize,
    /// Low.
    pub low: usize,
    /// Info.
    pub info: usize,
}

impl SeverityCounts {
    /// Count open findings.
    pub fn of<'a>(findings: impl IntoIterator<Item = &'a Finding>) -> Self {
        let mut c = Self::default();
        for f in findings
            .into_iter()
            .filter(|f| f.state == FindingState::Open)
        {
            match f.severity {
                Severity::Critical => c.critical += 1,
                Severity::High => c.high += 1,
                Severity::Medium => c.medium += 1,
                Severity::Low => c.low += 1,
                Severity::Info => c.info += 1,
            }
        }
        c
    }

    /// All open findings.
    pub fn total(&self) -> usize {
        self.critical + self.high + self.medium + self.low + self.info
    }
}

/// The last run, in brief.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSummary {
    /// When it finished (Unix epoch ms).
    pub run_at_ms: u64,
    /// How it ended.
    pub status: RunStatus,
    /// Subjects analysed.
    pub subjects: usize,
    /// Open findings by severity.
    pub open: SeverityCounts,
}

/// One analyzer as surfaces show it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzerView {
    /// Id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Publisher.
    pub publisher: String,
    /// One-line purpose.
    pub summary: String,
    /// Project page.
    pub homepage: String,
    /// Licence.
    pub license: String,
    /// Install hint (shown, never run).
    pub install: Option<String>,
    /// `builtin` or `cli`.
    pub kind: RunnerKind,
    /// Consent state.
    pub state: AnalyzerState,
    /// Why it is `changed`.
    pub changes: Vec<String>,
    /// Declared permissions.
    pub permissions: Permissions,
    /// Containment level.
    pub sandbox: SandboxLevel,
    /// The program that will run (CLI analyzers), when found.
    pub binary: Option<String>,
    /// Version approved.
    pub version: Option<String>,
    /// When it was approved.
    pub approved_at_ms: Option<u64>,
    /// Last run.
    pub last_run: Option<RunSummary>,
    /// Whether it runs on its own when the analysed items change (always for analyzers
    /// without network access; network analyzers only when the user allows it).
    #[serde(default)]
    pub background: bool,
    /// Declared API keys and whether one is stored (names only).
    #[serde(default)]
    pub secrets: Vec<SecretStatus>,
    /// Whether it comes from holdmap's signed catalog or the user registered it.
    #[serde(default)]
    pub source: ManifestSource,
    /// MCP analyzers: transport, endpoint, chosen tools and the tool-list pin.
    #[serde(default)]
    pub mcp: Option<McpView>,
    /// How its output is read (`sarif`, `json-map`, `auto`, ...), for the pipeline graph.
    pub output_format: crate::manifest::OutputFormat,
}

/// Where a manifest comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestSource {
    /// holdmap's signed catalog.
    #[default]
    Catalog,
    /// Registered by the user (`holdmap analyzers add`).
    User,
}

/// One declared API key: its environment variable name and whether the user stored a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretStatus {
    /// Environment variable name.
    pub name: String,
    /// Whether a value is in the keychain (as recorded by holdmap; the keychain isn't opened).
    pub stored: bool,
}

/// One subject on the permissions sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewSubject {
    /// The subject.
    pub subject: Subject,
    /// Exactly what the analyzer gets: a folder path or a package coordinate.
    pub input: String,
}

/// Everything the permissions sheet shows, built from the real input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preview {
    /// The analyzer.
    pub analyzer: AnalyzerView,
    /// The exact command line, with placeholders explained (CLI analyzers).
    pub command: Option<Vec<String>>,
    /// Subjects and their inputs.
    pub subjects: Vec<PreviewSubject>,
    /// Found but not analysed.
    pub skipped: Vec<String>,
    /// Exact request bodies (built-in network analyzers).
    pub payload: Vec<String>,
    /// Environment names the run sees; values are set by holdmap, never copied from yours.
    pub env: Vec<String>,
    /// Sandbox description.
    pub sandbox: String,
    /// When it runs once approved.
    pub schedule: String,
    /// Things to know before approving (MCP analyzers: when the server runs, and whether it
    /// looks like a server one of your agents is configured with).
    #[serde(default)]
    pub notices: Vec<String>,
    /// Exactly which inventory fields reach the analyzer, in plain words.
    #[serde(default)]
    pub projection: Vec<String>,
    /// MCP analyzers: the exact arguments of each call for the first few subjects.
    #[serde(default)]
    pub tool_calls: Vec<ToolCallPreview>,
}

/// One tool call as it will be made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallPreview {
    /// Tool name.
    pub tool: String,
    /// Subject name.
    pub subject: String,
    /// Arguments as JSON (the folder path shown is an example of the private copy).
    pub arguments: serde_json::Value,
    /// Problems against the tool's input schema.
    #[serde(default)]
    pub problems: Vec<String>,
}

/// All current findings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Insights {
    /// Latest result of every approved analyzer that has run.
    pub results: Vec<AnalyzerResult>,
    /// Open findings across them.
    pub open: SeverityCounts,
}

/// Why a service call failed.
#[derive(Debug, Error)]
pub enum ServiceError {
    /// No analyzer with that id.
    #[error("no analyzer called {0:?}")]
    Unknown(String),
    /// The run was refused or couldn't start.
    #[error(transparent)]
    Run(#[from] RunError),
    /// A store write failed.
    #[error("could not save: {0}")]
    Io(#[from] std::io::Error),
    /// A catalog update was refused.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// A manifest was rejected.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// An API key could not be stored, read or deleted.
    #[error(transparent)]
    Secret(#[from] SecretError),
    /// An install didn't finish.
    #[error(transparent)]
    Install(#[from] InstallError),
}

/// The stores the service needs.
#[derive(Clone)]
pub struct Stores {
    /// Approvals.
    pub consent: Arc<dyn ConsentStore>,
    /// Results.
    pub results: Arc<dyn ResultStore>,
    /// Reviews.
    pub reviews: Arc<dyn ReviewStore>,
    /// Which secret names are stored.
    pub secrets: Arc<dyn SecretIndex>,
    /// Staged and accepted catalogs.
    pub catalog: Arc<dyn CatalogStore>,
    /// User-registered manifests.
    pub manifests: Arc<dyn UserManifestStore>,
    /// MCP tool listings.
    pub tools: Arc<dyn ToolPinStore>,
}

impl Stores {
    /// One object implementing all three.
    pub fn of<
        T: ConsentStore
            + ResultStore
            + ReviewStore
            + SecretIndex
            + CatalogStore
            + UserManifestStore
            + ToolPinStore
            + 'static,
    >(
        store: T,
    ) -> Self {
        let store = Arc::new(store);
        Self {
            consent: store.clone(),
            results: store.clone(),
            reviews: store.clone(),
            secrets: store.clone(),
            catalog: store.clone(),
            manifests: store.clone(),
            tools: store,
        }
    }
}

/// The catalog in use and where it came from.
struct ActiveCatalog {
    catalog: Catalog,
    accepted: bool,
    manifests: Arc<Vec<AnalyzerManifest>>,
}

impl ActiveCatalog {
    fn new(catalog: Catalog, accepted: bool) -> Self {
        Self {
            manifests: Arc::new(catalog.manifests.clone()),
            catalog,
            accepted,
        }
    }
}

/// The shortest snooze: one hour.
pub const MIN_SNOOZE_MS: u64 = 3_600_000;
/// The longest snooze: 90 days, so nothing stays hidden indefinitely by accident.
pub const MAX_SNOOZE_MS: u64 = 90 * 86_400_000;

/// The orchestrator.
pub struct AnalyzerService {
    catalog: RwLock<ActiveCatalog>,
    catalog_key: String,
    catalog_source: Box<dyn CatalogSource>,
    stores: Stores,
    probe: Box<dyn BinaryProbe>,
    projector: Box<dyn InputProjector>,
    cli: Box<dyn AnalyzerRunner>,
    builtin: Box<dyn AnalyzerRunner>,
    mcp: Box<dyn McpAnalyzerRunner>,
    sandbox: SandboxLevel,
    cancel: CancelFlag,
    keychain: Box<dyn SecretStore>,
    now: fn() -> u64,
    installer: Option<Installer>,
    install_cancel: CancelFlag,
}

impl AnalyzerService {
    /// Production wiring: bundled catalog, system probe, the best OS sandbox available, OSV over
    /// HTTPS.
    pub fn new(stores: Stores) -> Self {
        let sandbox = crate::sandbox::best_available();
        let level = sandbox.level();
        let (active, accepted) =
            crate::catalog::load_active(stores.catalog.as_ref(), crate::catalog::PUBLIC_KEY);
        let manifests = active.manifests.clone();
        let mut service = Self::with_parts(
            manifests,
            stores,
            Box::new(SystemProbe::from_env()),
            Box::new(CliRunner::new(sandbox)),
            Box::new(OsvRunner::with_default_transport()),
        )
        .with_sandbox_level(level)
        .with_mcp_runner(Box::new(
            crate::mcp_runner::McpRunner::with_default_connector(crate::sandbox::best_available()),
        ))
        .with_secrets(crate::secrets::default_store());
        service.catalog = RwLock::new(ActiveCatalog::new(active, accepted));
        service
    }

    /// State this sandbox level on views and previews (it must match the CLI runner's).
    pub fn with_sandbox_level(mut self, level: SandboxLevel) -> Self {
        self.sandbox = level;
        self
    }

    /// Explicit wiring (tests, other front ends).
    pub fn with_parts(
        catalog: Vec<AnalyzerManifest>,
        stores: Stores,
        probe: Box<dyn BinaryProbe>,
        cli: Box<dyn AnalyzerRunner>,
        builtin: Box<dyn AnalyzerRunner>,
    ) -> Self {
        let bundled = crate::catalog::bundled_catalog();
        Self {
            catalog: RwLock::new(ActiveCatalog::new(
                Catalog {
                    manifests: catalog,
                    ..bundled
                },
                false,
            )),
            catalog_key: crate::catalog::PUBLIC_KEY.into(),
            catalog_source: Box::new(crate::catalog::HttpsCatalogSource),
            stores,
            probe,
            projector: Box::new(InventoryProjector),
            cli,
            builtin,
            mcp: Box::new(crate::mcp_runner::McpRunner::with_default_connector(
                Box::new(crate::runner::BasicSandbox),
            )),
            sandbox: SandboxLevel::Basic,
            cancel: CancelFlag::default(),
            keychain: Box::new(crate::secrets::NoSecrets),
            now: now_ms,
            installer: None,
            install_cancel: CancelFlag::default(),
        }
    }

    /// Allow in-app installs into `tools` (`<holdmap dir>/analyzers/tools`), and look for
    /// analyzers' programs there first. `probe` finds package managers; production passes
    /// [`SystemProbe::from_env`].
    pub fn with_tools_dir(
        mut self,
        tools: impl Into<std::path::PathBuf>,
        probe: SystemProbe,
    ) -> Self {
        let tools = tools.into();
        self.probe = Box::new(probe.clone().with_tools_dir(&tools));
        self.installer = Some(Installer::new(tools, probe));
        self
    }

    /// A different installer (tests: a short time limit).
    pub fn with_installer(mut self, installer: Installer) -> Self {
        self.installer = Some(installer);
        self
    }

    fn installable(&self, id: &str) -> Result<AnalyzerManifest, ServiceError> {
        self.manifest(id).or_else(|_| {
            let text = crate::presets::source(id)?;
            Ok(AnalyzerManifest::parse(text)?)
        })
    }

    fn installer(&self) -> Result<&Installer, ServiceError> {
        self.installer.as_ref().ok_or_else(|| {
            ServiceError::Install(InstallError::Unavailable(
                "In-app installs aren't available here.".into(),
            ))
        })
    }

    /// What installing an analyzer's (or a preset's) program would do. Runs nothing.
    pub fn install_plan(&self, id: &str) -> Result<InstallPlan, ServiceError> {
        let m = self.installable(id)?;
        Ok(self.installer()?.plan(&m))
    }

    /// Install an analyzer's (or a preset's) program after the user agreed to its plan. It
    /// stays off until approved; an approved analyzer whose program changes asks again.
    pub fn install(
        &self,
        id: &str,
        on: &dyn Fn(InstallEvent),
    ) -> Result<InstallRecord, ServiceError> {
        let m = self.installable(id)?;
        self.install_cancel.reset();
        Ok(self
            .installer()?
            .install(&m, &self.install_cancel, (self.now)(), on)?)
    }

    /// Stop the install in progress.
    pub fn cancel_install(&self) {
        self.install_cancel.cancel();
    }

    /// The runner for MCP analyzers (tests use an in-process fake).
    pub fn with_mcp_runner(mut self, runner: Box<dyn McpAnalyzerRunner>) -> Self {
        self.mcp = runner;
        self
    }

    /// Stop the run in progress (MCP analyzers stop between and during calls).
    pub fn cancel_runs(&self) {
        self.cancel.cancel();
    }

    /// Verify catalogs with this key and download them from `source` (tests).
    pub fn with_catalog_source(mut self, key: &str, source: Box<dyn CatalogSource>) -> Self {
        self.catalog_key = key.into();
        self.catalog_source = source;
        self
    }

    /// Which catalog is in use, and any verified update waiting for review.
    pub fn catalog_status(&self) -> CatalogStatus {
        let active = self.catalog.read().unwrap_or_else(|e| e.into_inner());
        CatalogStatus {
            version: active.catalog.version,
            published: active.catalog.published.clone(),
            accepted: active.accepted,
            pending: crate::catalog::pending(
                self.stores.catalog.as_ref(),
                &self.catalog_key,
                &active.catalog,
            ),
        }
    }

    /// Download the newest catalog (only when the user asks), verify it and stage it for
    /// review. Nothing changes until [`Self::accept_catalog`].
    pub fn fetch_catalog(&self) -> Result<CatalogDiff, ServiceError> {
        let offered = self
            .catalog_source
            .fetch()
            .map_err(CatalogError::Unavailable)?;
        self.stage_catalog(&offered)
    }

    /// Verify a catalog file the user supplied and stage it for review.
    pub fn stage_catalog(&self, offered: &SignedCatalog) -> Result<CatalogDiff, ServiceError> {
        let active = self.catalog.read().unwrap_or_else(|e| e.into_inner());
        Ok(crate::catalog::stage(
            self.stores.catalog.as_ref(),
            &self.catalog_key,
            &active.catalog,
            offered,
        )?)
    }

    /// Apply the staged catalog. Analyzers whose manifest changed ask for approval again.
    pub fn accept_catalog(&self) -> Result<CatalogStatus, ServiceError> {
        {
            let mut active = self.catalog.write().unwrap_or_else(|e| e.into_inner());
            let new = crate::catalog::accept(
                self.stores.catalog.as_ref(),
                &self.catalog_key,
                &active.catalog,
            )?;
            *active = ActiveCatalog::new(new, true);
        }
        Ok(self.catalog_status())
    }

    /// Discard the staged catalog.
    pub fn reject_catalog(&self) -> Result<(), ServiceError> {
        Ok(self
            .stores
            .catalog
            .set_catalog(crate::catalog::CatalogSlot::Pending, None)?)
    }

    /// Where API keys are kept (the OS keychain in production).
    pub fn with_secrets(mut self, keychain: Box<dyn SecretStore>) -> Self {
        self.keychain = keychain;
        self
    }

    fn secret_status(&self, m: &AnalyzerManifest) -> Vec<SecretStatus> {
        let stored = self.stores.secrets.secret_names(&m.id);
        m.permissions
            .secrets
            .iter()
            .map(|name| SecretStatus {
                name: name.clone(),
                stored: stored.contains(name),
            })
            .collect()
    }

    fn declared<'m>(&self, m: &'m AnalyzerManifest, name: &str) -> Result<&'m str, ServiceError> {
        m.permissions
            .secrets
            .iter()
            .find(|n| *n == name)
            .map(String::as_str)
            .ok_or_else(|| {
                ServiceError::Secret(SecretError::Invalid(format!(
                    "{} doesn't declare a key called {name:?}",
                    m.name
                )))
            })
    }

    /// Store an API key the analyzer's manifest declares. The value goes to the keychain only.
    pub fn set_secret(&self, id: &str, name: &str, value: &Secret) -> Result<(), ServiceError> {
        let m = &self.manifest(id)?;
        let name = self.declared(m, name)?;
        check_value(value)?;
        self.keychain.set(&m.id, name, value)?;
        self.stores.secrets.mark_secret(&m.id, name, true)?;
        Ok(())
    }

    /// Delete a stored API key. `true` when one existed.
    pub fn delete_secret(&self, id: &str, name: &str) -> Result<bool, ServiceError> {
        let m = &self.manifest(id)?;
        let name = self.declared(m, name)?;
        let existed = self.keychain.delete(&m.id, name)?;
        self.stores.secrets.mark_secret(&m.id, name, false)?;
        Ok(existed)
    }

    /// The keys a run gets, plus plain notes about any that are missing.
    fn run_secrets(&self, m: &AnalyzerManifest) -> (Vec<(String, Secret)>, Vec<String>) {
        let mut found = Vec::new();
        let mut notes = Vec::new();
        for status in self.secret_status(m) {
            if !status.stored {
                notes.push(format!(
                    "{} is not stored, so it ran without it.",
                    status.name
                ));
                continue;
            }
            match self.keychain.get(&m.id, &status.name) {
                Ok(Some(v)) => found.push((status.name, v)),
                Ok(None) => notes.push(format!(
                    "{} is no longer in the {}, so it ran without it.",
                    status.name,
                    self.keychain.label()
                )),
                Err(_) => notes.push(format!(
                    "{} could not be read from the {}, so it ran without it.",
                    status.name,
                    self.keychain.label()
                )),
            }
        }
        (found, notes)
    }

    /// Fix the clock (tests).
    pub fn with_clock(mut self, now: fn() -> u64) -> Self {
        self.now = now;
        self
    }

    fn signed(&self) -> Arc<Vec<AnalyzerManifest>> {
        self.catalog
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .manifests
            .clone()
    }

    /// The signed catalog's manifests plus valid user-registered ones (catalog ids win).
    fn catalog(&self) -> Arc<Vec<AnalyzerManifest>> {
        let signed = self.signed();
        let user = self.user_manifests(&signed);
        if user.is_empty() {
            return signed;
        }
        Arc::new(signed.iter().cloned().chain(user).collect())
    }

    fn user_manifests(&self, signed: &[AnalyzerManifest]) -> Vec<AnalyzerManifest> {
        let mut out: Vec<AnalyzerManifest> = Vec::new();
        for text in self.stores.manifests.user_manifests() {
            if let Ok(m) = AnalyzerManifest::parse(&text) {
                if !signed.iter().chain(&out).any(|o| o.id == m.id) {
                    out.push(m);
                }
            }
        }
        out
    }

    fn is_user(&self, id: &str) -> bool {
        !self.signed().iter().any(|m| m.id == id)
    }

    /// Register a manifest the user wrote or downloaded. It stays off until approved.
    pub fn add_manifest(&self, text: &str) -> Result<AnalyzerView, ServiceError> {
        if text.len() as u64 > crate::store::MAX_MANIFEST_BYTES {
            return Err(ManifestError::Rule("manifest is larger than 64 KB".into()).into());
        }
        let m = AnalyzerManifest::parse(text)?;
        if !self.is_user(&m.id) {
            return Err(ManifestError::Rule(format!(
                "{} is in holdmap's catalog; pick another id",
                m.id
            ))
            .into());
        }
        self.stores.manifests.add_user_manifest(&m.id, text)?;
        self.get(&m.id)
    }

    /// The quick-select presets, marking those already added.
    pub fn presets(&self) -> Vec<crate::presets::PresetView> {
        let catalog = self.catalog();
        crate::presets::views(
            |id| catalog.iter().any(|m| m.id == id),
            |m| self.probe.installed(m),
        )
    }

    /// Register a preset as the user's own analyzer. Like any added manifest it stays off
    /// until the user approves its permissions.
    pub fn add_preset(&self, id: &str) -> Result<AnalyzerView, ServiceError> {
        self.add_manifest(crate::presets::source(id)?)
    }

    /// Remove a user-registered analyzer: its approval, results and stored keys go too.
    pub fn remove_manifest(&self, id: &str) -> Result<bool, ServiceError> {
        if !self.is_user(id) {
            return Err(ManifestError::Rule(format!(
                "{id} is part of holdmap's catalog; turn it off instead"
            ))
            .into());
        }
        if let Ok(m) = self.manifest(id) {
            for name in &m.permissions.secrets {
                let _ = self.keychain.delete(&m.id, name);
                let _ = self.stores.secrets.mark_secret(&m.id, name, false);
            }
            self.stores.results.forget(&m.id)?;
            self.stores.consent.revoke(&m.id)?;
            for slot in [ToolSlot::Latest, ToolSlot::Approved] {
                self.stores.tools.set_tool_snapshot(&m.id, slot, None)?;
            }
        }
        Ok(self.stores.manifests.remove_user_manifest(id)?)
    }

    fn manifest(&self, id: &str) -> Result<AnalyzerManifest, ServiceError> {
        self.catalog()
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or_else(|| ServiceError::Unknown(id.into()))
    }

    /// Every analyzer with its state. Never runs anything.
    pub fn list(&self) -> Vec<AnalyzerView> {
        self.catalog().iter().map(|m| self.view(m)).collect()
    }

    /// One analyzer.
    pub fn get(&self, id: &str) -> Result<AnalyzerView, ServiceError> {
        Ok(self.view(&self.manifest(id)?))
    }

    fn view(&self, m: &AnalyzerManifest) -> AnalyzerView {
        let grant = self.stores.consent.grant(&m.id);
        let binary = needs_binary(m).then(|| self.probe.locate(m)).flatten();
        let (state, changes) = self.state_of(
            m,
            grant.as_ref(),
            binary
                .as_ref()
                .map(|b| (b.path.as_path(), b.sha256.as_str())),
        );
        let mcp = self.mcp_view(m, grant.as_ref());
        let last_run = self.stores.results.latest(&m.id).map(|r| {
            let r = self.with_reviews(r);
            RunSummary {
                run_at_ms: r.run_at_ms,
                status: r.status,
                subjects: r.subjects.len(),
                open: SeverityCounts::of(&r.findings),
            }
        });
        AnalyzerView {
            id: m.id.clone(),
            name: m.name.clone(),
            publisher: m.publisher.clone(),
            summary: m.summary.clone(),
            homepage: m.homepage.clone(),
            license: m.license.clone(),
            install: m.install.clone(),
            kind: m.runner.kind,
            state,
            changes,
            permissions: m.permissions.clone(),
            sandbox: self.sandbox,
            background: m.runner.kind != RunnerKind::Mcp
                && (!m.uses_network() || grant.as_ref().is_some_and(|g| g.background_network)),
            source: if self.is_user(&m.id) {
                ManifestSource::User
            } else {
                ManifestSource::Catalog
            },
            binary: binary.map(|b| b.path.display().to_string()),
            version: grant.as_ref().and_then(|g| g.version.clone()),
            approved_at_ms: grant.map(|g| g.approved_at_ms),
            last_run,
            secrets: self.secret_status(m),
            mcp,
            output_format: m.output.format,
        }
    }

    /// Consent state, including the MCP tool-list pin.
    fn state_of(
        &self,
        m: &AnalyzerManifest,
        grant: Option<&Grant>,
        binary: Option<(&std::path::Path, &str)>,
    ) -> (AnalyzerState, Vec<String>) {
        let (mut state, mut changes) = state(m, grant, binary);
        if m.runner.kind == RunnerKind::Mcp && state == AnalyzerState::Approved {
            if let Some(why) = self.tool_pin_change(m, grant) {
                state = AnalyzerState::Changed;
                changes.extend(why);
            }
        }
        (state, changes)
    }

    /// The exact data preview for the permissions sheet.
    pub fn preview(&self, id: &str, report: &AgentsReport) -> Result<Preview, ServiceError> {
        let m = &self.manifest(id)?;
        let input = self.projector.project(report, m);
        let builtin = m.runner.kind == RunnerKind::Builtin;
        Ok(Preview {
            analyzer: self.view(m),
            command: (!builtin).then(|| {
                if let Some(url) = m.endpoint().and(m.runner.url.as_ref()) {
                    return vec![
                        format!("connects to {url}"),
                        format!("then calls {} once per skill", tool_names(m)),
                    ];
                }
                let mut line: Vec<String> =
                    std::iter::once(m.runner.command.clone().unwrap_or_default())
                        .chain(m.runner.args.iter().map(|a| {
                            a.replace(SUBJECT_DIR, "<copy of the skill folder>")
                                .replace(OUTPUT, "<private report file>")
                        }))
                        .collect();
                if m.runner.kind == RunnerKind::Mcp {
                    line.push(format!("then calls {} once per skill", tool_names(m)));
                }
                line
            }),
            notices: notices(m, report),
            projection: projection(m),
            tool_calls: self.tool_call_preview(m, &input),
            subjects: input
                .subjects
                .iter()
                .map(|s| PreviewSubject {
                    subject: s.subject.clone(),
                    input: match (&s.dir, &s.package) {
                        (Some(dir), _) => dir.display().to_string(),
                        (_, Some(p)) => format!("{} {} {}", p.ecosystem, p.name, p.version),
                        _ => String::new(),
                    },
                })
                .collect(),
            skipped: input.skipped.clone(),
            payload: if builtin && m.output.format == OutputFormat::Osv {
                request_bodies(&input)
            } else {
                Vec::new()
            },
            env: if builtin || m.endpoint().is_some() {
                Vec::new()
            } else {
                ENV_NAMES
                    .iter()
                    .map(|n| n.to_string())
                    .chain(self.secret_status(m).into_iter().map(|s| {
                        if s.stored {
                            format!(
                                "{} (from your {}; value not shown)",
                                s.name,
                                self.keychain.label()
                            )
                        } else {
                            format!("{} (not stored; runs without it)", s.name)
                        }
                    }))
                    .collect()
            },
            sandbox: if builtin {
                "Runs inside holdmap. Only the exact request shown here is sent.".into()
            } else if let Some(e) = m.endpoint() {
                if e.loopback {
                    "holdmap doesn't start this server: it connects to a server you run on this computer, so holdmap's sandbox doesn't apply to it. Only the arguments shown here are sent.".into()
                } else {
                    format!("holdmap connects to a server on another machine ({}). Only the metadata shown here is sent; holdmap's sandbox doesn't apply to it.", e.host)
                }
            } else {
                self.sandbox.describe(m.uses_network())
            },
            schedule: schedule(m, self.view(m).background),
        })
    }

    /// Approve exactly the current manifest and program.
    pub fn approve(&self, id: &str) -> Result<AnalyzerView, ServiceError> {
        self.approve_with(id, false)
    }

    /// Approve, with `allow_remote` for an MCP server on another machine (metadata only).
    pub fn approve_with(&self, id: &str, allow_remote: bool) -> Result<AnalyzerView, ServiceError> {
        let m = &self.manifest(id)?;
        let tools_sha256 = self.check_mcp_approval(m, allow_remote)?;
        let mut grant = Grant {
            analyzer: m.id.clone(),
            manifest_sha256: m.sha256.clone(),
            binary: None,
            binary_sha256: None,
            version: None,
            approved_at_ms: (self.now)(),
            background_network: false,
            tools_sha256,
            remote_opt_in: allow_remote && m.is_remote(),
        };
        if needs_binary(m) {
            let info = self
                .probe
                .locate(m)
                .ok_or_else(|| RunError::Unavailable(format!("{} is not installed.", m.name)))?;
            grant.version = self.probe.version(m, &info.path);
            grant.binary = Some(info.path);
            grant.binary_sha256 = Some(info.sha256);
        }
        self.stores.consent.record(grant)?;
        self.get(id)
    }

    /// Allow or stop automatic runs, when items change, for an approved analyzer that uses the
    /// network. Analyzers without network access always run automatically once approved.
    pub fn set_background(&self, id: &str, allow: bool) -> Result<AnalyzerView, ServiceError> {
        let m = &self.manifest(id)?;
        let mut grant = self.stores.consent.grant(&m.id).ok_or_else(|| {
            RunError::NotApproved(format!("{} is off. Turn it on first.", m.name))
        })?;
        if m.runner.kind == RunnerKind::Mcp {
            return Err(RunError::NotApproved(format!(
                "{} starts an MCP server, so it only runs when you run it.",
                m.name
            ))
            .into());
        }
        if !m.uses_network() {
            return self.get(id);
        }
        grant.background_network = allow;
        self.stores.consent.record(grant)?;
        self.get(id)
    }

    /// Turn an analyzer off and forget its results.
    pub fn revoke(&self, id: &str) -> Result<bool, ServiceError> {
        let m = &self.manifest(id)?;
        self.stores.results.forget(&m.id)?;
        Ok(self.stores.consent.revoke(&m.id)?)
    }

    /// Hash of the input an analyzer would get now.
    pub fn input_signature(&self, id: &str, report: &AgentsReport) -> Result<String, ServiceError> {
        let m = &self.manifest(id)?;
        Ok(signature(&self.projector.project(report, m)))
    }

    /// Run approved analyzers whose input changed since their last result. Returns the ids run.
    pub fn run_changed(&self, report: &AgentsReport) -> Vec<String> {
        let mut ran = Vec::new();
        let catalog = self.catalog();
        for m in catalog.iter() {
            let view = self.view(m);
            if view.state != AnalyzerState::Approved || !view.background {
                continue;
            }
            let input = self.projector.project(report, m);
            if input.subjects.is_empty() {
                continue;
            }
            let last = self
                .stores
                .results
                .latest(&m.id)
                .and_then(|r| r.input_signature);
            if last.as_deref() != Some(signature(&input).as_str())
                && self.run(&m.id, report).is_ok()
            {
                ran.push(m.id.clone());
            }
        }
        ran
    }

    /// Run one approved, unchanged analyzer now.
    pub fn run(&self, id: &str, report: &AgentsReport) -> Result<AnalyzerResult, ServiceError> {
        self.run_with_progress(id, report, ProgressSink::default())
    }

    /// [`Self::run`], reporting each step (preparing, copying and scanning each subject,
    /// collecting) to `progress`.
    pub fn run_with_progress(
        &self,
        id: &str,
        report: &AgentsReport,
        progress: ProgressSink,
    ) -> Result<AnalyzerResult, ServiceError> {
        let m = &self.manifest(id)?;
        progress.send(RunEvent::step(RunStep::Preparing, 0));
        let grant = self.stores.consent.grant(&m.id);
        let mut cx = RunContext::default();
        let mut version = grant.as_ref().and_then(|g| g.version.clone());
        match m.runner.kind {
            RunnerKind::Mcp if !needs_binary(m) => {
                let (state, changes) = self.state_of(m, grant.as_ref(), None);
                refuse(m, state, &changes)?;
                if m.is_remote() && !grant.as_ref().is_some_and(|g| g.remote_opt_in) {
                    return Err(RunError::NotApproved(format!(
                        "{} is on another machine. Approve it again and allow sending metadata there.",
                        m.name
                    ))
                    .into());
                }
            }
            RunnerKind::Cli | RunnerKind::Mcp => {
                let info = self.probe.locate(m);
                let (state, changes) = self.state_of(
                    m,
                    grant.as_ref(),
                    info.as_ref().map(|b| (b.path.as_path(), b.sha256.as_str())),
                );
                refuse(m, state, &changes)?;
                let info = info.expect("approved implies located");
                let now = self.probe.version(m, &info.path);
                if now != version {
                    return Err(RunError::NotApproved(format!(
                        "{} reports a different version than when you approved it. Review and approve it again.",
                        m.name
                    ))
                    .into());
                }
                version = now;
                cx.binary = Some(info.path);
            }
            RunnerKind::Builtin => {
                let (state, changes) = state(m, grant.as_ref(), None);
                refuse(m, state, &changes)?;
                version = Some(env!("CARGO_PKG_VERSION").into());
            }
        }
        let (secrets, secret_notes) = self.run_secrets(m);
        cx.secrets = secrets;
        cx.tools_sha256 = grant.as_ref().and_then(|g| g.tools_sha256.clone());
        self.cancel.reset();
        cx.cancel = self.cancel.clone();
        let input = self.projector.project(report, m);
        let total = input.subjects.len();
        progress.send(RunEvent::step(RunStep::Preparing, total));
        if m.runner.kind != RunnerKind::Cli {
            progress.send(RunEvent::step(RunStep::Scanning, total));
        }
        cx.progress = progress.clone();
        let started = std::time::Instant::now();
        let outputs = match m.runner.kind {
            RunnerKind::Builtin => self.builtin.run(m, &input, &cx),
            RunnerKind::Cli => self.cli.run(m, &input, &cx),
            RunnerKind::Mcp => self.mcp.run(m, &input, &cx),
        };
        let outputs = match outputs {
            Err(RunError::ToolsChanged(mut changed)) => {
                changed.changes = self.record_changed_tools(m, &changed.snapshot)?;
                return Err(RunError::ToolsChanged(changed).into());
            }
            other => other?,
        };
        let duration_ms = started.elapsed().as_millis() as u64;
        progress.send(RunEvent::step(RunStep::Collecting, total));
        let parser: Box<dyn OutputParser> = match m.output.format {
            OutputFormat::Osv => Box::new(OsvParser),
            other => for_format(other).expect("parser for every non-builtin format"),
        };
        let mut result = assemble(m, version, &input, &outputs, parser.as_ref(), (self.now)());
        result.input_signature = Some(signature(&input));
        result.duration_ms = Some(duration_ms);
        result.notes.extend(secret_notes);
        if let Some(previous) = self.stores.results.latest(&m.id) {
            result.changes = changes_since(&previous, &mut result);
        }
        self.stores.results.save(&result)?;
        self.stores
            .results
            .append_history(&RunRecord::of(&result))?;
        Ok(self.with_reviews(result))
    }

    /// Past runs of one analyzer, newest first (counts only; findings are kept for the latest
    /// run).
    pub fn history(&self, id: &str) -> Result<Vec<RunRecord>, ServiceError> {
        let m = self.manifest(id)?;
        Ok(self.stores.results.history(&m.id))
    }

    /// The folder of a subject in an analyzer's latest result, only when it is a folder
    /// holdmap's inventory recorded and it still exists. Used to reveal it in the file manager;
    /// a path coming from the front end is never trusted.
    pub fn subject_dir(&self, id: &str, subject: &str) -> Result<std::path::PathBuf, ServiceError> {
        let m = self.manifest(id)?;
        let result = self
            .stores
            .results
            .latest(&m.id)
            .ok_or_else(|| ServiceError::Unknown(format!("{id} has no results")))?;
        let dir = result
            .subjects
            .iter()
            .find(|s| s.id == subject)
            .and_then(|s| s.dir.as_deref())
            .map(std::path::PathBuf::from)
            .filter(|d| d.is_absolute() && d.is_dir())
            .ok_or_else(|| ServiceError::Unknown(format!("{subject} has no folder to show")))?;
        Ok(dir)
    }

    /// Latest results with review states applied.
    pub fn insights(&self) -> Insights {
        let results: Vec<AnalyzerResult> = self
            .catalog()
            .iter()
            .filter(|m| self.stores.consent.grant(&m.id).is_some())
            .filter_map(|m| self.stores.results.latest(&m.id))
            .map(|r| self.with_reviews(r))
            .collect();
        let open = SeverityCounts::of(results.iter().flat_map(|r| &r.findings));
        Insights { results, open }
    }

    /// Mark a finding reviewed, snooze it for `snooze_ms`, or (with `Open`) reopen it.
    pub fn review(
        &self,
        fingerprint: &str,
        state: FindingState,
        snooze_ms: Option<u64>,
    ) -> Result<(), ServiceError> {
        self.review_with_reason(fingerprint, state, snooze_ms, None)
    }

    /// [`Self::review`] with the user's reason, kept (capped and secret-masked) next to the
    /// review and shown with the finding.
    /// Review or snooze a finding. Snoozes last from [`MIN_SNOOZE_MS`] to [`MAX_SNOOZE_MS`].
    pub fn review_with_reason(
        &self,
        fingerprint: &str,
        state: FindingState,
        snooze_ms: Option<u64>,
        reason: Option<&str>,
    ) -> Result<(), ServiceError> {
        let reason = reason
            .map(|r| clean_text(r, MAX_NOTE))
            .filter(|r| !r.is_empty());
        let review = match state {
            FindingState::Open => None,
            FindingState::Reviewed => Some(Review {
                state,
                until_ms: None,
                reason,
            }),
            FindingState::Snoozed => Some(Review {
                state,
                until_ms: Some(
                    (self.now)().saturating_add(
                        snooze_ms
                            .unwrap_or(7 * 24 * 3_600_000)
                            .clamp(MIN_SNOOZE_MS, MAX_SNOOZE_MS),
                    ),
                ),
                reason,
            }),
        };
        Ok(self.stores.reviews.set_review(fingerprint, review)?)
    }

    fn with_reviews(&self, mut result: AnalyzerResult) -> AnalyzerResult {
        let reviews = self.stores.reviews.reviews();
        let now = (self.now)();
        for f in &mut result.findings {
            let review = reviews.get(&f.fingerprint);
            f.state = review.map(|r| r.effective(now)).unwrap_or_default();
            let hidden = review.filter(|_| f.state != FindingState::Open);
            f.note = hidden.and_then(|r| r.reason.clone());
            f.until_ms = hidden
                .filter(|_| f.state == FindingState::Snoozed)
                .and_then(|r| r.until_ms);
        }
        result
    }
}

/// New and fixed findings against the previous run. Nothing is compared when either run
/// failed, and a partial run never reports fixes: a finding it didn't see may still be there.
fn changes_since(previous: &AnalyzerResult, current: &mut AnalyzerResult) -> Option<RunChanges> {
    if previous.status == RunStatus::Failed || current.status == RunStatus::Failed {
        return None;
    }
    let mut changes = RunChanges::between(previous, &mut current.findings);
    if current.status != RunStatus::Complete {
        changes.fixed.clear();
        changes.fixed_total = 0;
    }
    Some(changes)
}

fn refuse(m: &AnalyzerManifest, state: AnalyzerState, changes: &[String]) -> Result<(), RunError> {
    match state {
        AnalyzerState::Approved => Ok(()),
        AnalyzerState::NotInstalled => Err(RunError::Unavailable(format!(
            "{} is not installed.",
            m.name
        ))),
        AnalyzerState::NotApproved => Err(RunError::NotApproved(format!(
            "{} is off. Review its permissions and turn it on first.",
            m.name
        ))),
        AnalyzerState::Changed => Err(RunError::NotApproved(format!(
            "{} changed since you approved it ({}). Review and approve it again.",
            m.name,
            changes.join("; ")
        ))),
    }
}

/// Plain notices for the permissions sheet.
fn notices(m: &AnalyzerManifest, report: &AgentsReport) -> Vec<String> {
    let mut out = Vec::new();
    if m.runner.kind != RunnerKind::Mcp {
        return out;
    }
    match m.endpoint() {
        None => out.push(
            "This starts a local MCP server. holdmap starts it only when you run this analyzer, with a cleared environment and the same sandbox as other analyzers, and stops it when the run ends. It never runs on its own.".into(),
        ),
        Some(e) if e.loopback => out.push(
            "This connects to an MCP server you run on this computer. holdmap only connects when you run this analyzer, and never on its own.".into(),
        ),
        Some(e) => out.push(format!(
            "Warning: this sends skill names, file names and agent ids to a server on another machine ({}). Never file contents. holdmap only connects when you run it, and only if you allowed this when approving.",
            e.host
        )),
    }
    out.push(
        "holdmap lists the server's tools and pins them. If the server changes any tool, name, description or input, the analyzer stops until you review and approve it again. holdmap gives the server no way to ask you questions or reach an AI model.".into(),
    );
    for (agent, server) in configured_matches(m, report) {
        out.push(format!(
            "Warning: this looks like the MCP server {agent} is configured to use ({server}). holdmap starts its own copy: it gets none of your agent's settings or keys, and anything the server does when it starts (signing in, syncing, writing files it is allowed to) happens again."
        ));
    }
    out
}

/// Agents whose configured stdio MCP server starts the same program as `m`.
fn configured_matches(m: &AnalyzerManifest, report: &AgentsReport) -> Vec<(String, String)> {
    let base = |s: &str| {
        let name = s
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(s)
            .to_ascii_lowercase();
        name.strip_suffix(".exe")
            .map(str::to_string)
            .unwrap_or(name)
    };
    let mut programs: Vec<String> = m
        .runner
        .command
        .iter()
        .chain(&m.runner.args)
        .map(|s| base(s))
        .filter(|s| !s.is_empty() && !s.starts_with('-'))
        .collect();
    programs.dedup();
    let mut out = Vec::new();
    let host = m.runner.url.as_deref().and_then(|u| {
        let rest = u.split("://").nth(1)?;
        Some(rest.split(['/', '?']).next()?.to_ascii_lowercase())
    });
    for (product, caps) in crate::input::inventories(report) {
        for server in &caps.mcp_servers {
            if let (Some(h), Some(r)) = (&host, &server.remote_host) {
                let pair = (product.to_string(), server.name.clone());
                if h.eq_ignore_ascii_case(r) && !out.contains(&pair) {
                    out.push(pair);
                }
                continue;
            }
            let Some(package) = server.launch.as_ref().and_then(|l| l.package.as_deref()) else {
                continue;
            };
            let package = base(package);
            let package = package.split('@').next().unwrap_or(&package).to_string();
            if programs.contains(&package) {
                let pair = (product.to_string(), server.name.clone());
                if !out.contains(&pair) {
                    out.push(pair);
                }
            }
        }
    }
    out
}

fn schedule(m: &AnalyzerManifest, background: bool) -> String {
    if m.runner.kind == RunnerKind::Mcp {
        return if m.endpoint().is_some() {
            "Only when you click Run now. holdmap connects to the server for that run.".into()
        } else {
            "Only when you click Run now. holdmap starts the MCP server for that run and stops it afterwards.".into()
        };
    }
    match (m.uses_network(), background) {
        (false, _) => "When you click Run now, and while holdmap is open when the analysed items change.".into(),
        (true, true) => "When you click Run now, and while holdmap is open when the analysed items change (you allowed automatic runs that use the network).".into(),
        (true, false) => "Only when you click Run now. It uses the network, so it doesn't run on its own unless you allow automatic runs.".into(),
    }
}

/// Whether the analyzer runs a local program holdmap must find and pin.
fn needs_binary(m: &AnalyzerManifest) -> bool {
    match m.runner.kind {
        RunnerKind::Builtin => false,
        RunnerKind::Cli => true,
        RunnerKind::Mcp => m.runner.transport == Transport::Stdio,
    }
}

fn tool_names(m: &AnalyzerManifest) -> String {
    if m.runner.tools.is_empty() {
        return "no tools yet (choose them first)".into();
    }
    m.runner
        .tools
        .iter()
        .map(|t| {
            format!(
                "{}({})",
                t.name,
                t.args.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Exactly which inventory fields reach the analyzer.
fn projection(m: &AnalyzerManifest) -> Vec<String> {
    match (m.runner.kind, m.output.format) {
        (RunnerKind::Builtin, OutputFormat::Osv) => {
            vec!["package ecosystem, name and exact version (pinned packages only)".into()]
        }
        (RunnerKind::Builtin, _) => Vec::new(),
        (RunnerKind::Cli, _) => vec!["path of a private copy of the skill folder".into()],
        (RunnerKind::Mcp, _) => {
            let mut used: Vec<crate::mcp_map::Placeholder> = Vec::new();
            for p in m
                .runner
                .tools
                .iter()
                .flat_map(|t| t.args.values().flat_map(crate::mcp_map::used))
            {
                if !used.contains(&p) {
                    used.push(p);
                }
            }
            if used.is_empty() {
                vec!["nothing from your inventory (constant arguments only)".into()]
            } else {
                used.iter().map(|p| p.label().to_string()).collect()
            }
        }
    }
}

fn state(
    m: &AnalyzerManifest,
    grant: Option<&Grant>,
    binary: Option<(&std::path::Path, &str)>,
) -> (AnalyzerState, Vec<String>) {
    if needs_binary(m) && binary.is_none() {
        return (AnalyzerState::NotInstalled, Vec::new());
    }
    let Some(grant) = grant else {
        return (AnalyzerState::NotApproved, Vec::new());
    };
    let mut changes = Vec::new();
    if grant.manifest_sha256 != m.sha256 {
        changes.push("its manifest changed with a holdmap update".to_string());
    }
    if let Some((path, sha)) = binary {
        if grant.binary.as_deref() != Some(path) {
            changes.push(format!("a different program will run: {}", path.display()));
        } else if grant.binary_sha256.as_deref() != Some(sha) {
            changes.push("its program file changed".to_string());
        }
    }
    if changes.is_empty() {
        (AnalyzerState::Approved, changes)
    } else {
        (AnalyzerState::Changed, changes)
    }
}

fn signature(input: &AnalyzerInput) -> String {
    let mut hash = Sha256::new();
    for s in &input.subjects {
        hash.update(s.subject.id.as_bytes());
        hash.update([0]);
        if let Some(dir) = &s.dir {
            hash.update(folder_stamp(dir).as_bytes());
        }
    }
    hex(&hash.finalize()[..16])
}

/// Names, sizes and modification times of a folder's files (bounded), so edits change it.
fn folder_stamp(dir: &std::path::Path) -> String {
    let mut parts = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten().take(2_000) {
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() && e.file_name() != ".git" && parts.len() < 5_000 {
                stack.push(e.path());
            } else if meta.is_file() {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                parts.push(format!("{}:{}:{mtime}", e.path().display(), meta.len()));
            }
        }
    }
    parts.sort();
    parts.join("\n")
}

/// Turn raw outputs into one normalised result.
fn assemble(
    m: &AnalyzerManifest,
    version: Option<String>,
    input: &AnalyzerInput,
    outputs: &[RawOutput],
    parser: &dyn OutputParser,
    now: u64,
) -> AnalyzerResult {
    let mut findings: Vec<Finding> = Vec::new();
    let mut details = Vec::new();
    let mut notes: Vec<String> = input.skipped.clone();
    let mut failed = 0;
    let mut partial = false;
    let mut coverage: Vec<f32> = Vec::new();
    let mut calls: Vec<ToolCalls> = Vec::new();
    for out in outputs.iter() {
        if let Some(tool) = &out.tool {
            let i = calls
                .iter()
                .position(|c| &c.tool == tool)
                .unwrap_or_else(|| {
                    calls.push(ToolCalls {
                        tool: tool.clone(),
                        calls: 0,
                        failed: 0,
                    });
                    calls.len() - 1
                });
            calls[i].calls += 1;
            calls[i].failed += usize::from(out.failed);
        }
    }
    for out in outputs {
        let label = out
            .subjects
            .first()
            .filter(|_| out.subjects.len() == 1)
            .map(|s| s.name.clone());
        let prefix = |n: &str| {
            label
                .as_ref()
                .map_or(n.to_string(), |l| format!("{l}: {n}"))
        };
        notes.extend(out.notes.iter().map(|n| prefix(n)));
        if out.failed {
            failed += 1;
            continue;
        }
        if out.nothing_to_check {
            notes.push(prefix("nothing to check here."));
            continue;
        }
        let parsed = match parser.parse(out, m) {
            Ok(p) => p,
            Err(e) => {
                failed += 1;
                notes.push(prefix(&format!("{e}.")));
                continue;
            }
        };
        partial |= parsed.status == Some(RunStatus::Partial)
            || out.notes.iter().any(|n| note_limits_coverage(n));
        if parsed.status == Some(RunStatus::Failed) {
            failed += 1;
            notes.push(prefix("the analyzer reported that the analysis failed."));
            continue;
        }
        coverage.extend(parsed.coverage);
        let (f, d) = normalise(m, out, parsed);
        findings.extend(f);
        details.extend(d);
    }
    let mut by_fp = BTreeMap::new();
    for f in findings {
        by_fp.entry(f.fingerprint.clone()).or_insert(f);
    }
    let mut findings: Vec<Finding> = by_fp.into_values().collect();
    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.subject.cmp(&b.subject))
            .then_with(|| a.title.cmp(&b.title))
    });
    let status = if !outputs.is_empty() && failed == outputs.len() {
        RunStatus::Failed
    } else if failed > 0 || partial {
        RunStatus::Partial
    } else {
        RunStatus::Complete
    };
    notes.dedup();
    notes.truncate(20);
    AnalyzerResult {
        analyzer: m.id.clone(),
        analyzer_version: version.map(|v| clean_text(&v, 40)),
        run_at_ms: now,
        status,
        coverage: (!coverage.is_empty())
            .then(|| coverage.iter().sum::<f32>() / coverage.len() as f32),
        subjects: input.subjects.iter().map(|s| s.subject.clone()).collect(),
        findings,
        details,
        notes: notes.iter().map(|n| clean_text(n, 200)).collect(),
        input_signature: None,
        duration_ms: None,
        tool_calls: calls,
        changes: None,
    }
}

fn normalise(m: &AnalyzerManifest, out: &RawOutput, parsed: Parsed) -> (Vec<Finding>, Vec<Detail>) {
    let single = (out.subjects.len() == 1).then(|| out.subjects[0].id.clone());
    let known = |id: &str| out.subjects.iter().any(|s| s.id == id);
    let mut per_subject: BTreeMap<String, usize> = BTreeMap::new();
    let mut findings = Vec::new();
    for p in parsed.findings {
        let Some(subject) = p
            .subject
            .clone()
            .filter(|s| known(s))
            .or_else(|| single.clone())
        else {
            continue;
        };
        let count = per_subject.entry(subject.clone()).or_default();
        *count += 1;
        if *count > MAX_FINDINGS_PER_SUBJECT {
            continue;
        }
        let rule_id = clean_id(&p.rule_id);
        let location = Location {
            file: p
                .location
                .file
                .as_deref()
                .map(unstage)
                .and_then(|f| clean_relative_path(&f)),
            line: p.location.line,
        };
        findings.push(Finding {
            fingerprint: fingerprint(
                &m.id,
                &rule_id,
                &subject,
                p.producer_fingerprint.as_deref(),
                &location,
            ),
            analyzer: m.id.clone(),
            title: clean_text(
                p.title
                    .as_deref()
                    .or(p.category.as_deref())
                    .unwrap_or(&rule_id),
                MAX_TITLE,
            ),
            category: p
                .category
                .map(|c| clean_text(&c.replace('_', " "), 60))
                .filter(|c| !c.is_empty()),
            severity: p.severity.unwrap_or(Severity::Medium),
            fix: p
                .fix
                .map(|f| clean_text(&f, MAX_FIX))
                .filter(|f| !f.is_empty()),
            refs: clean_refs(p.refs.iter().map(String::as_str)),
            state: FindingState::Open,
            note: None,
            until_ms: None,
            new: false,
            subject,
            location,
            rule_id,
        });
    }
    let details = parsed
        .details
        .into_iter()
        .filter_map(|d| {
            Some(Detail {
                subject: d.subject.filter(|s| known(s)).or_else(|| single.clone())?,
                label: clean_text(&d.label, 40),
                value: clean_text(&d.value, MAX_DETAIL),
            })
        })
        .collect();
    (findings, details)
}

/// Make a path inside the staged copy relative to the subject.
fn unstage(file: &str) -> String {
    let file = file.strip_prefix("file://").unwrap_or(file);
    let staged = crate::cli_runner::STAGED_SUBJECT;
    // Relative to a CLI run's working directory, which holds the staged copy.
    if let Some(rest) = file
        .strip_prefix("./")
        .unwrap_or(file)
        .strip_prefix(staged)
        .and_then(|r| r.strip_prefix(['/', '\\']))
    {
        return rest.to_string();
    }
    let Some(i) = file.find("holdmap-analyzer-") else {
        return file.to_string();
    };
    let rest = &file[i..];
    for folder in [staged, "subject"] {
        let marker = format!("/{folder}/");
        if let Some(j) = rest.find(&marker) {
            return rest[j + marker.len()..].to_string();
        }
    }
    // MCP runs stage each subject under subjects/<n>/.
    if let Some(j) = rest.find("/subjects/") {
        let after = &rest[j + "/subjects/".len()..];
        if let Some(k) = after.find('/') {
            return after[k + 1..].to_string();
        }
    }
    file.to_string()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::unstage;
    use crate::cli_runner::{note_limits_coverage, settings_note};

    #[test]
    fn leaving_scanner_settings_out_does_not_make_a_run_partial() {
        assert!(!note_limits_coverage(&settings_note(1)));
        assert!(!note_limits_coverage(&settings_note(2)));
        assert!(note_limits_coverage(
            "The folder is larger than the staging limit; only part of it was analysed."
        ));
        assert!(note_limits_coverage("Stopped: the run was cancelled."));
    }

    #[test]
    fn staged_paths_become_relative() {
        assert_eq!(
            unstage("file:///tmp/holdmap-analyzer-ab12/subject/scripts/x.sh"),
            "scripts/x.sh"
        );
        assert_eq!(
            unstage("/tmp/holdmap-analyzer-ab12/subjects/3/SKILL.md"),
            "SKILL.md"
        );
        assert_eq!(unstage("SKILL.md"), "SKILL.md");
        assert_eq!(
            unstage("/tmp/holdmap-analyzer-ab12/holdmap-subject/scripts/x.sh"),
            "scripts/x.sh"
        );
        // Relative to the run's working directory (Cisco Skill Scanner).
        assert_eq!(unstage("holdmap-subject/SKILL.md"), "SKILL.md");
        assert_eq!(unstage("./holdmap-subject/a/b.py"), "a/b.py");
        // A skill's own folder named "subject" is left alone.
        assert_eq!(unstage("subject/notes.md"), "subject/notes.md");
    }
}

#[cfg(test)]
mod change_tests {
    use super::*;

    fn run(status: RunStatus, fps: &[&str]) -> AnalyzerResult {
        let mut r: AnalyzerResult = serde_json::from_value(serde_json::json!({
            "analyzer": "a.b", "analyzer_version": null, "run_at_ms": 1, "status": status,
            "coverage": null, "subjects": [], "findings": [],
        }))
        .unwrap();
        r.findings = fps
            .iter()
            .map(|fp| {
                serde_json::from_value(serde_json::json!({
                    "fingerprint": fp, "analyzer": "a.b", "rule_id": "r", "category": null,
                    "severity": "high", "title": "t", "subject": "s", "fix": null,
                }))
                .unwrap()
            })
            .collect();
        r
    }

    #[test]
    fn a_partial_run_never_claims_fixes_and_a_failed_one_compares_nothing() {
        let before = run(RunStatus::Complete, &["a", "b"]);
        let mut partial_run = run(RunStatus::Partial, &["a", "c"]);
        let partial = changes_since(&before, &mut partial_run).unwrap();
        assert_eq!((partial.new_total, partial.fixed_total), (1, 0));
        assert!(partial.fixed.is_empty());
        assert!(!partial_run.findings[0].new && partial_run.findings[1].new);
        assert_eq!(
            changes_since(&before, &mut run(RunStatus::Failed, &[])),
            None
        );
        assert_eq!(
            changes_since(&run(RunStatus::Failed, &[]), &mut before.clone()),
            None
        );
        let complete = changes_since(&before, &mut run(RunStatus::Complete, &["a"])).unwrap();
        assert_eq!((complete.new_total, complete.fixed_total), (0, 1));
    }
}
