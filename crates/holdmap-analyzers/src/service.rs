//! [`AnalyzerService`]: the one entry point surfaces use. It lists analyzers with their
//! consent state, builds the exact data preview, records approvals pinned to the manifest and
//! binary, runs only approved and unchanged analyzers, normalises their output and applies
//! review states.

use crate::cli_runner::CliRunner;
use crate::consent::{ConsentStore, Grant, ResultStore, Review, ReviewStore, SecretIndex};
use crate::finding::{
    clean_id, clean_refs, clean_relative_path, clean_text, fingerprint, hex, AnalyzerResult,
    Detail, Finding, FindingState, Location, RunStatus, Severity, Subject, MAX_DETAIL,
    MAX_FINDINGS_PER_SUBJECT, MAX_FIX, MAX_TITLE,
};
use crate::input::{AnalyzerInput, InputProjector, InventoryProjector};
use crate::manifest::{
    AnalyzerManifest, OutputFormat, Permissions, RunnerKind, OUTPUT, SUBJECT_DIR,
};
use crate::osv::{request_bodies, OsvParser, OsvRunner};
use crate::parse::{for_format, OutputParser, Parsed};
use crate::probe::{BinaryProbe, SystemProbe};
use crate::runner::{
    AnalyzerRunner, BasicSandbox, RawOutput, RunContext, RunError, SandboxLevel, ENV_NAMES,
};
use crate::secrets::{check_value, Secret, SecretError, SecretStore};
use holdmap_core::agents::AgentsReport;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

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
    /// Declared API keys and whether one is stored (names only).
    #[serde(default)]
    pub secrets: Vec<SecretStatus>,
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
    /// An API key could not be stored, read or deleted.
    #[error(transparent)]
    Secret(#[from] SecretError),
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
}

impl Stores {
    /// One object implementing all three.
    pub fn of<T: ConsentStore + ResultStore + ReviewStore + SecretIndex + 'static>(
        store: T,
    ) -> Self {
        let store = Arc::new(store);
        Self {
            consent: store.clone(),
            results: store.clone(),
            reviews: store.clone(),
            secrets: store,
        }
    }
}

/// The orchestrator.
pub struct AnalyzerService {
    catalog: Vec<AnalyzerManifest>,
    stores: Stores,
    probe: Box<dyn BinaryProbe>,
    projector: Box<dyn InputProjector>,
    cli: Box<dyn AnalyzerRunner>,
    builtin: Box<dyn AnalyzerRunner>,
    sandbox: SandboxLevel,
    keychain: Box<dyn SecretStore>,
    now: fn() -> u64,
}

impl AnalyzerService {
    /// Production wiring: bundled catalog, system probe, basic sandbox, OSV over HTTPS.
    pub fn new(stores: Stores) -> Self {
        Self::with_parts(
            crate::catalog::bundled(),
            stores,
            Box::new(SystemProbe::from_env()),
            Box::new(CliRunner::new(Box::new(BasicSandbox))),
            Box::new(OsvRunner::with_default_transport()),
        )
        .with_secrets(crate::secrets::default_store())
    }

    /// Explicit wiring (tests, other front ends).
    pub fn with_parts(
        catalog: Vec<AnalyzerManifest>,
        stores: Stores,
        probe: Box<dyn BinaryProbe>,
        cli: Box<dyn AnalyzerRunner>,
        builtin: Box<dyn AnalyzerRunner>,
    ) -> Self {
        Self {
            catalog,
            stores,
            probe,
            projector: Box::new(InventoryProjector),
            cli,
            builtin,
            sandbox: SandboxLevel::Basic,
            keychain: Box::new(crate::secrets::NoSecrets),
            now: now_ms,
        }
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
        let m = self.manifest(id)?;
        let name = self.declared(m, name)?;
        check_value(value)?;
        self.keychain.set(&m.id, name, value)?;
        self.stores.secrets.mark_secret(&m.id, name, true)?;
        Ok(())
    }

    /// Delete a stored API key. `true` when one existed.
    pub fn delete_secret(&self, id: &str, name: &str) -> Result<bool, ServiceError> {
        let m = self.manifest(id)?;
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

    fn manifest(&self, id: &str) -> Result<&AnalyzerManifest, ServiceError> {
        self.catalog
            .iter()
            .find(|m| m.id == id)
            .ok_or_else(|| ServiceError::Unknown(id.into()))
    }

    /// Every analyzer with its state. Never runs anything.
    pub fn list(&self) -> Vec<AnalyzerView> {
        self.catalog.iter().map(|m| self.view(m)).collect()
    }

    /// One analyzer.
    pub fn get(&self, id: &str) -> Result<AnalyzerView, ServiceError> {
        Ok(self.view(self.manifest(id)?))
    }

    fn view(&self, m: &AnalyzerManifest) -> AnalyzerView {
        let grant = self.stores.consent.grant(&m.id);
        let binary = (m.runner.kind == RunnerKind::Cli)
            .then(|| self.probe.locate(m))
            .flatten();
        let (state, changes) = state(
            m,
            grant.as_ref(),
            binary
                .as_ref()
                .map(|b| (b.path.as_path(), b.sha256.as_str())),
        );
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
            binary: binary.map(|b| b.path.display().to_string()),
            version: grant.as_ref().and_then(|g| g.version.clone()),
            approved_at_ms: grant.map(|g| g.approved_at_ms),
            last_run,
            secrets: self.secret_status(m),
        }
    }

    /// The exact data preview for the permissions sheet.
    pub fn preview(&self, id: &str, report: &AgentsReport) -> Result<Preview, ServiceError> {
        let m = self.manifest(id)?;
        let input = self.projector.project(report, m);
        let builtin = m.runner.kind == RunnerKind::Builtin;
        Ok(Preview {
            analyzer: self.view(m),
            command: (!builtin).then(|| {
                std::iter::once(m.runner.command.clone().unwrap_or_default())
                    .chain(m.runner.args.iter().map(|a| {
                        a.replace(SUBJECT_DIR, "<copy of the skill folder>")
                            .replace(OUTPUT, "<private report file>")
                    }))
                    .collect()
            }),
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
            env: if builtin {
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
            } else {
                self.sandbox.describe().into()
            },
            schedule:
                "When you click Run now, and while holdmap is open when the analysed items change."
                    .into(),
        })
    }

    /// Approve exactly the current manifest and program.
    pub fn approve(&self, id: &str) -> Result<AnalyzerView, ServiceError> {
        let m = self.manifest(id)?;
        let mut grant = Grant {
            analyzer: m.id.clone(),
            manifest_sha256: m.sha256.clone(),
            binary: None,
            binary_sha256: None,
            version: None,
            approved_at_ms: (self.now)(),
        };
        if m.runner.kind == RunnerKind::Cli {
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

    /// Turn an analyzer off and forget its results.
    pub fn revoke(&self, id: &str) -> Result<bool, ServiceError> {
        let m = self.manifest(id)?;
        self.stores.results.forget(&m.id)?;
        Ok(self.stores.consent.revoke(&m.id)?)
    }

    /// Hash of the input an analyzer would get now.
    pub fn input_signature(&self, id: &str, report: &AgentsReport) -> Result<String, ServiceError> {
        let m = self.manifest(id)?;
        Ok(signature(&self.projector.project(report, m)))
    }

    /// Run approved analyzers whose input changed since their last result. Returns the ids run.
    pub fn run_changed(&self, report: &AgentsReport) -> Vec<String> {
        let mut ran = Vec::new();
        for m in &self.catalog {
            if self.view(m).state != AnalyzerState::Approved {
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
        let m = self.manifest(id)?;
        let grant = self.stores.consent.grant(&m.id);
        let mut cx = RunContext::default();
        let mut version = grant.as_ref().and_then(|g| g.version.clone());
        match m.runner.kind {
            RunnerKind::Cli => {
                let info = self.probe.locate(m);
                let (state, changes) = state(
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
            RunnerKind::Mcp => {
                return Err(
                    RunError::Unavailable("MCP analyzers are not supported yet.".into()).into(),
                )
            }
        }
        let (secrets, secret_notes) = self.run_secrets(m);
        cx.secrets = secrets;
        let input = self.projector.project(report, m);
        let runner = if m.runner.kind == RunnerKind::Builtin {
            &self.builtin
        } else {
            &self.cli
        };
        let outputs = runner.run(m, &input, &cx)?;
        let parser: Box<dyn OutputParser> = match m.output.format {
            OutputFormat::Osv => Box::new(OsvParser),
            other => for_format(other).expect("parser for every non-builtin format"),
        };
        let mut result = assemble(m, version, &input, &outputs, parser.as_ref(), (self.now)());
        result.input_signature = Some(signature(&input));
        result.notes.extend(secret_notes);
        self.stores.results.save(&result)?;
        Ok(self.with_reviews(result))
    }

    /// Latest results with review states applied.
    pub fn insights(&self) -> Insights {
        let results: Vec<AnalyzerResult> = self
            .catalog
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
        let review = match state {
            FindingState::Open => None,
            FindingState::Reviewed => Some(Review {
                state,
                until_ms: None,
            }),
            FindingState::Snoozed => Some(Review {
                state,
                until_ms: Some(
                    (self.now)().saturating_add(snooze_ms.unwrap_or(7 * 24 * 3_600_000)),
                ),
            }),
        };
        Ok(self.stores.reviews.set_review(fingerprint, review)?)
    }

    fn with_reviews(&self, mut result: AnalyzerResult) -> AnalyzerResult {
        let reviews = self.stores.reviews.reviews();
        let now = (self.now)();
        for f in &mut result.findings {
            f.state = reviews
                .get(&f.fingerprint)
                .map(|r| r.effective(now))
                .unwrap_or_default();
        }
        result
    }
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

fn state(
    m: &AnalyzerManifest,
    grant: Option<&Grant>,
    binary: Option<(&std::path::Path, &str)>,
) -> (AnalyzerState, Vec<String>) {
    if m.runner.kind == RunnerKind::Cli && binary.is_none() {
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
        let parsed = match parser.parse(out, m) {
            Ok(p) => p,
            Err(e) => {
                failed += 1;
                notes.push(prefix(&format!("{e}.")));
                continue;
            }
        };
        partial |= parsed.status == Some(RunStatus::Partial) || !out.notes.is_empty();
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
    match file.find("holdmap-analyzer-") {
        Some(i) => match file[i..].find("/subject/") {
            Some(j) => file[i + j + "/subject/".len()..].to_string(),
            None => file.to_string(),
        },
        None => file.to_string(),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::unstage;

    #[test]
    fn staged_paths_become_relative() {
        assert_eq!(
            unstage("file:///tmp/holdmap-analyzer-ab12/subject/scripts/x.sh"),
            "scripts/x.sh"
        );
        assert_eq!(unstage("SKILL.md"), "SKILL.md");
    }
}
