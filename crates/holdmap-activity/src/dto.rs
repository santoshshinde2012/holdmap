use serde::{Deserialize, Serialize};

/// The integration that reported an activity record; this is not verified process identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Codex,
    Claude,
    OpenTelemetry,
    Holdmap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// The instrumentation mechanism that supplied metadata.
pub enum Source {
    Hook,
    Otlp,
    Mcp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// A semantic call or internal step. Steps do not count as separate tool invocations.
pub enum ActivityKind {
    Agent,
    Tool,
    Model,
    Handoff,
    Step,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// The source's explicit lifecycle evidence, without guessing missing outcomes.
pub enum ActivityStatus {
    /// A hook observed a request before permission or execution.
    Requested,
    /// Instrumentation explicitly reported a start.
    Running,
    /// Instrumentation explicitly reported successful completion.
    Ok,
    /// Instrumentation explicitly reported failure.
    Error,
    /// Instrumentation explicitly reported interruption or cancellation.
    Cancelled,
    /// No explicit outcome is available, including an incomplete stopped collection.
    Unknown,
}

/// This allowlist deliberately has no payload, URL, path, message or error-text field.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivitySpan {
    /// Pseudonymous record identity, formatted as `hm1_` plus 64 lowercase hexadecimal digits.
    pub id: String,
    /// Empty when the source does not supply real distributed trace context.
    pub trace_id: String,
    /// An explicit parent span in the same distributed trace; session membership is insufficient.
    pub parent_id: Option<String>,
    /// Pseudonymous session grouping without an implied communication edge.
    pub session_id: Option<String>,
    /// Pseudonymous source-reported agent identity.
    pub agent_id: Option<String>,
    /// Pseudonymous source-reported agent ancestry, separate from span ancestry.
    pub parent_agent_id: Option<String>,
    /// Integration attribution, not authentication of a process.
    pub provider: Provider,
    /// Instrumentation mechanism used for this record.
    pub source: Source,
    /// Semantic call or internal step category.
    pub kind: ActivityKind,
    /// A bounded sanitized identifier; unsafe source labels are replaced with a generic label.
    pub name: String,
    /// UTC Unix milliseconds; hook timestamps are receipt times.
    pub start_at_ms: Option<u64>,
    /// UTC Unix milliseconds; completion logs may fall back to observation or receipt time.
    pub end_at_ms: Option<u64>,
    /// Source-supplied elapsed time or the difference between explicit OTLP span timestamps.
    pub duration_ms: Option<u64>,
    /// Outcome supported by this record's explicit lifecycle evidence.
    pub status: ActivityStatus,
    /// Serialized input/result size, not network traffic volume.
    pub input_bytes: Option<u64>,
    /// Serialized result size, not network traffic volume.
    pub output_bytes: Option<u64>,
    /// Source-reported association only; never authorizes process controls.
    pub pid: Option<u32>,
    /// Source-reported process start association; never authorizes process controls.
    pub process_started_at: Option<u64>,
}

/// A bounded, credential-free projection suitable for desktop and CLI presentation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActivitySnapshot {
    /// Retained spans, ordered by first receipt rather than inferred execution order.
    pub spans: Vec<ActivitySpan>,
    /// Normalized records received, including repeated lifecycle updates and identity conflicts.
    pub received: u64,
    /// Rejected records and retention evictions; transport rejections cannot imply a record count.
    pub dropped: u64,
    /// Human-readable coverage and privacy limits for the UI.
    pub limits: Vec<String>,
}

/// Public recording state. It deliberately contains no bearer token.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CollectorStatus {
    /// Whether the explicitly started local collector is running.
    pub active: bool,
    /// The local base endpoint while the collector is started.
    pub endpoint: Option<String>,
    /// UTC Unix milliseconds of the current collector's start.
    pub started_at_ms: Option<u64>,
    /// Normalized record count for the in-memory store.
    pub received: u64,
    /// Rejected records and evictions for the in-memory store.
    pub dropped: u64,
}

/// Returned only by explicit start. Debug deliberately omits the bearer token.
#[derive(Clone, Serialize, Deserialize)]
pub struct CollectorCredentials {
    /// Exact loopback base endpoint without any path or query.
    pub endpoint: String,
    /// A fresh 32-byte random bearer token, encoded as 64 lowercase hexadecimal digits.
    pub token: String,
}

impl std::fmt::Debug for CollectorCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectorCredentials")
            .field("endpoint", &self.endpoint)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

/// Fixed bounds shared by normalization, retention and the HTTP receiver.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CollectorLimits {
    /// Maximum retained unique span identities.
    pub max_spans: usize,
    /// Maximum uncompressed JSON request body size.
    pub max_body_bytes: usize,
    /// Maximum records examined in one request.
    pub max_records_per_request: usize,
    /// Fixed number of connection workers.
    pub max_workers: usize,
    /// Maximum queued connections waiting for those workers.
    pub max_pending_connections: usize,
    /// Global connection request allowance per one-second window.
    pub requests_per_second: usize,
}

/// Conservative bounds for one explicitly enabled local recording session.
pub const COLLECTOR_LIMITS: CollectorLimits = CollectorLimits {
    max_spans: 1_000,
    max_body_bytes: 256 * 1024,
    max_records_per_request: 256,
    max_workers: 4,
    max_pending_connections: 8,
    requests_per_second: 120,
};

pub(crate) fn visibility_limits() -> Vec<String> {
    vec![
        "Activity is opt-in and stays in memory on this computer; stopping collection keeps the current snapshot until it is cleared or Holdmap closes.".into(),
        "Only instrumented sources are visible. Process relationships and encrypted connections do not establish tool calls or internal messages.".into(),
        "Hooks can report a request before permission or execution. Missing completion stays unknown; OpenTelemetry spans usually arrive after completion.".into(),
        "Parents appear only when the source supplies correlation. Input and output sizes describe serialized payloads, not network traffic.".into(),
        "At most 1,000 spans are retained. Records can be lost through sampling, cancelled hooks, export delays, collector limits or incomplete instrumentation.".into(),
        "Only allowlisted metadata is retained; prompt, argument, result and raw error fields are excluded. Names are sanitized source labels; unsafe labels are redacted.".into(),
    ]
}
