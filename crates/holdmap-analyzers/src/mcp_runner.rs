//! Runs a user-registered MCP analyzer. A stdio server is started from its approved command
//! only for this run (cleared environment, same sandbox as CLI analyzers) and stopped after;
//! an HTTP server is reached at its approved URL. Before any call holdmap lists the server's
//! tools and compares them with the approved pin; any difference stops the run. Each chosen
//! tool is then called once per skill with arguments built from the mapping, checked against
//! the tool's input schema first. Calls have a time limit, can be cancelled, and their output
//! is capped.
//!
//! The MCP client sits behind [`McpConnector`] so the runner is tested with an in-process
//! fake; production uses [`RmcpConnector`] (feature `mcp-runner`, HTTP with `mcp-http`).
//! Server error text is never kept: notes are written by holdmap.

use crate::cli_runner::{finish_env, stage, StagingLimits};
use crate::finding::Subject;
use crate::input::AnalyzerInput;
use crate::manifest::{AnalyzerManifest, Transport};
use crate::mcp_map::{self, Placeholder, SubjectData};
use crate::mcp_tools::{ServerInfo, ToolInfo, ToolSnapshot};
use crate::runner::{
    AnalyzerRunner, CancelFlag, Containment, RawOutput, RunContext, RunError, Sandbox, ToolsChanged,
};
use crate::secrets::Secret;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// What one tool call returned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolOutput {
    /// Structured content as JSON, else the text blocks joined by newlines (capped).
    pub bytes: Vec<u8>,
    /// Whether `bytes` is the tool's structured content.
    pub structured: bool,
    /// The tool flagged its result as an error.
    pub is_error: bool,
    /// The result was larger than the output limit and was dropped.
    pub truncated: bool,
}

/// How to reach a server.
pub enum McpEndpoint {
    /// Start this prepared command (sandboxed, environment cleared) and speak over stdio.
    Stdio(Command),
    /// Connect to a Streamable HTTP endpoint.
    Http {
        /// The approved URL.
        url: String,
        /// Sent as `Authorization: Bearer` when set.
        bearer: Option<Secret>,
        /// Largest response body read.
        cap: usize,
    },
}

/// A connected MCP server, for the length of one run or listing.
pub trait McpSession {
    /// Server identity from the handshake.
    fn server(&self) -> Option<ServerInfo>;
    /// Every tool the server lists (all pages).
    fn tools(&self) -> Vec<ToolInfo>;
    /// Call `tool` with `args`, waiting at most `timeout`, keeping at most `cap` bytes and
    /// giving up when `cancel` is set (the server is told). Errors are holdmap's own words.
    fn call(
        &mut self,
        tool: &str,
        args: Map<String, Value>,
        timeout: Duration,
        cap: usize,
        cancel: &CancelFlag,
    ) -> Result<ToolOutput, String>;
    /// Disconnect, and stop the server and everything it started (stdio).
    fn close(self: Box<Self>);
}

/// Connects to an MCP server.
pub trait McpConnector: Send + Sync {
    /// Connect and complete the handshake and tool listing within `timeout`. Errors are
    /// holdmap's own words.
    fn connect(
        &self,
        endpoint: McpEndpoint,
        sandbox: &dyn Sandbox,
        timeout: Duration,
    ) -> Result<Box<dyn McpSession>, String>;
}

/// An analyzer runner that can also list an MCP server's tools.
pub trait McpAnalyzerRunner: AnalyzerRunner {
    /// Start or reach the server, list and pin its tools, and stop it. Nothing is called.
    fn list_tools(
        &self,
        manifest: &AnalyzerManifest,
        cx: &RunContext,
    ) -> Result<ToolSnapshot, RunError>;
}

/// The MCP runner.
pub struct McpRunner {
    sandbox: Box<dyn Sandbox>,
    connector: Box<dyn McpConnector>,
    limits: StagingLimits,
}

impl McpRunner {
    /// A runner reaching servers through `connector`, contained by `sandbox` (stdio).
    pub fn new(sandbox: Box<dyn Sandbox>, connector: Box<dyn McpConnector>) -> Self {
        Self {
            sandbox,
            connector,
            limits: StagingLimits::default(),
        }
    }

    /// Production wiring, when this build can run MCP analyzers.
    pub fn with_default_connector(sandbox: Box<dyn Sandbox>) -> Self {
        #[cfg(feature = "mcp-runner")]
        let connector: Box<dyn McpConnector> = Box::new(RmcpConnector);
        #[cfg(not(feature = "mcp-runner"))]
        let connector: Box<dyn McpConnector> = Box::new(Unsupported);
        Self::new(sandbox, connector)
    }

    fn connect(
        &self,
        manifest: &AnalyzerManifest,
        cx: &RunContext,
        work: &Path,
        timeout: Duration,
    ) -> Result<Box<dyn McpSession>, RunError> {
        let cap = (manifest.runner.max_output_mb * 1024 * 1024) as usize;
        let endpoint = match manifest.runner.transport {
            Transport::Stdio => {
                let binary = cx.binary.as_deref().ok_or_else(|| {
                    RunError::Unavailable(format!("{} is not installed.", manifest.name))
                })?;
                let home = work.join("home");
                let _ = std::fs::create_dir_all(&home);
                let mut cmd = self.sandbox.command(
                    binary,
                    &manifest.runner.args,
                    &Containment {
                        work,
                        home: &home,
                        network: manifest.uses_network(),
                    },
                );
                cmd.current_dir(work);
                finish_env(&mut cmd, manifest, binary, &cx.secrets);
                McpEndpoint::Stdio(cmd)
            }
            Transport::Http => McpEndpoint::Http {
                url: manifest.runner.url.clone().unwrap_or_default(),
                bearer: manifest.runner.auth_secret.as_ref().and_then(|name| {
                    cx.secrets
                        .iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, v)| v.clone())
                }),
                cap,
            },
        };
        self.connector
            .connect(endpoint, self.sandbox.as_ref(), timeout)
            .map_err(|why| {
                RunError::Unavailable(format!("The MCP server could not be reached: {why}."))
            })
    }
}

/// The endpoint as pinned: the command line or URL (never secrets).
pub fn endpoint_label(manifest: &AnalyzerManifest) -> String {
    match manifest.runner.transport {
        Transport::Stdio => std::iter::once(manifest.runner.command.clone().unwrap_or_default())
            .chain(manifest.runner.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" "),
        Transport::Http => manifest.runner.url.clone().unwrap_or_default(),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn private_folder() -> Result<tempfile::TempDir, RunError> {
    tempfile::Builder::new()
        .prefix("holdmap-analyzer-")
        .tempdir()
        .map_err(|_| RunError::Unavailable("Could not create a private staging folder.".into()))
}

#[cfg(not(feature = "mcp-runner"))]
struct Unsupported;

#[cfg(not(feature = "mcp-runner"))]
impl McpConnector for Unsupported {
    fn connect(
        &self,
        _: McpEndpoint,
        _: &dyn Sandbox,
        _: Duration,
    ) -> Result<Box<dyn McpSession>, String> {
        Err("this build of holdmap can't run MCP analyzers".into())
    }
}

fn output(subject: &Subject, tool: Option<&str>, note: Option<&str>, failed: bool) -> RawOutput {
    RawOutput {
        subjects: vec![subject.clone()],
        exit_code: None,
        bytes: Vec::new(),
        notes: note.map(|n| vec![n.to_string()]).unwrap_or_default(),
        failed,
        tool: tool.map(str::to_string),
        nothing_to_check: false,
    }
}

impl McpAnalyzerRunner for McpRunner {
    fn list_tools(
        &self,
        manifest: &AnalyzerManifest,
        cx: &RunContext,
    ) -> Result<ToolSnapshot, RunError> {
        let work = private_folder()?;
        let timeout = Duration::from_secs(manifest.runner.timeout_secs.min(60));
        let session = self.connect(manifest, cx, work.path(), timeout)?;
        let snapshot = ToolSnapshot::new(
            &endpoint_label(manifest),
            session.server(),
            session.tools(),
            now_ms(),
        );
        session.close();
        Ok(snapshot)
    }
}

impl AnalyzerRunner for McpRunner {
    fn run(
        &self,
        manifest: &AnalyzerManifest,
        input: &AnalyzerInput,
        cx: &RunContext,
    ) -> Result<Vec<RawOutput>, RunError> {
        if input.subjects.is_empty() {
            return Err(RunError::NoSubjects("no skills were found".into()));
        }
        let Some(pin) = cx.tools_sha256.as_deref() else {
            return Err(RunError::NotApproved(format!(
                "{} has no approved tool list. List its tools and approve it first.",
                manifest.name
            )));
        };
        let tools = &manifest.runner.tools;
        let needs: Vec<Placeholder> = tools
            .iter()
            .flat_map(|t| t.args.values().flat_map(mcp_map::used))
            .collect();
        let work = private_folder()?;

        // Stage every subject first; a stdio server only ever sees these copies.
        let mut staged: Vec<Result<PathBuf, &'static str>> = Vec::new();
        let mut pre_notes: Vec<Option<&'static str>> = Vec::new();
        for (i, s) in input.subjects.iter().enumerate() {
            let Some(dir) = s.dir.as_deref() else {
                staged.push(Err("No folder to analyse."));
                pre_notes.push(None);
                continue;
            };
            let to = work.path().join("subjects").join(i.to_string());
            match stage(dir, &to, self.limits) {
                Ok(st) => {
                    pre_notes.push(st.truncated.then_some(
                        "The folder is larger than the staging limit; only part of it was analysed.",
                    ));
                    staged.push(Ok(to));
                }
                Err(_) => {
                    staged.push(Err("The folder could not be read."));
                    pre_notes.push(None);
                }
            }
        }

        let timeout = Duration::from_secs(manifest.runner.timeout_secs);
        let deadline = Instant::now() + timeout;
        let cap = (manifest.runner.max_output_mb * 1024 * 1024) as usize;
        let all_failed = |note: &str| -> Vec<RawOutput> {
            input
                .subjects
                .iter()
                .map(|s| output(&s.subject, None, Some(note), true))
                .collect()
        };
        let mut session = match self.connect(manifest, cx, work.path(), timeout) {
            Ok(s) => s,
            Err(RunError::Unavailable(why)) => return Ok(all_failed(&why)),
            Err(other) => return Err(other),
        };
        let snapshot = ToolSnapshot::new(
            &endpoint_label(manifest),
            session.server(),
            session.tools(),
            now_ms(),
        );
        if snapshot.sha256 != pin {
            session.close();
            return Err(RunError::ToolsChanged(Box::new(ToolsChanged {
                snapshot,
                changes: Vec::new(),
            })));
        }

        let mut result = Vec::new();
        'subjects: for ((s, dir), pre) in input.subjects.iter().zip(staged).zip(pre_notes) {
            let dir = match dir {
                Ok(d) => d,
                Err(note) => {
                    result.push(output(&s.subject, None, Some(note), true));
                    continue;
                }
            };
            if let Some(note) = pre {
                result.push(output(&s.subject, None, Some(note), false));
            }
            let data = SubjectData::read(&dir, &s.subject.name, &s.subject.agents, &needs);
            for spec in tools {
                let mut out = output(&s.subject, Some(&spec.name), None, true);
                if cx.cancel.is_cancelled() {
                    out.notes.push(crate::runner::CANCELLED.into());
                    result.push(out);
                    continue;
                }
                let Some(tool) = snapshot.tool(&spec.name) else {
                    out.notes.push(format!(
                        "The server no longer offers the tool {:?}.",
                        spec.name
                    ));
                    result.push(out);
                    continue;
                };
                let args = mcp_map::build(&spec.args, &data, &tool.input_schema);
                let problems = mcp_map::check(&args, &tool.input_schema);
                if !problems.is_empty() {
                    out.notes.push(format!(
                        "{}: the mapped arguments don't fit its inputs ({}).",
                        spec.name,
                        problems.join("; ")
                    ));
                    result.push(out);
                    continue;
                }
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    out.notes.push(format!(
                        "Stopped after the {}-second time limit.",
                        manifest.runner.timeout_secs
                    ));
                    result.push(out);
                    continue 'subjects;
                }
                match session.call(&spec.name, args, left, cap, &cx.cancel) {
                    Ok(t) if t.is_error => out
                        .notes
                        .push(format!("{}: the tool reported an error.", spec.name)),
                    Ok(t) if t.truncated => out.notes.push(format!(
                        "{}: the output was larger than the output limit and was not read.",
                        spec.name
                    )),
                    Ok(t) if t.bytes.is_empty() => out
                        .notes
                        .push(format!("{}: the tool returned no report.", spec.name)),
                    Ok(t) => {
                        if let (true, Some(schema)) = (t.structured, tool.output_schema.as_ref()) {
                            let fits = serde_json::from_slice::<Value>(&t.bytes)
                                .ok()
                                .and_then(|v| v.as_object().cloned())
                                .is_some_and(|o| mcp_map::check(&o, schema).is_empty());
                            if !fits {
                                out.notes.push(format!(
                                    "{}: its output doesn't match the output schema it declares.",
                                    spec.name
                                ));
                            }
                        }
                        out.bytes = t.bytes;
                        out.failed = false;
                    }
                    Err(why) => out.notes.push(format!("{}: {why}", spec.name)),
                }
                result.push(out);
            }
        }
        session.close();
        Ok(result)
    }
}

#[cfg(feature = "mcp-runner")]
pub use rmcp_client::{RmcpConnector, RmcpSession};

#[cfg(feature = "mcp-runner")]
mod rmcp_client {
    use super::{McpConnector, McpEndpoint, McpSession, ToolOutput};
    use crate::mcp_tools::{ServerInfo, ToolInfo};
    use crate::runner::{CancelFlag, ContainGuard, ProcessRef, Sandbox};
    use rmcp::model::{
        CallToolRequest, CallToolRequestParams, CancelledNotificationParam, ClientRequest,
        ServerResult,
    };
    use rmcp::service::{PeerRequestOptions, RunningService};
    use rmcp::{RoleClient, ServiceExt};
    use serde_json::{Map, Value};
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::io::{AsyncRead, AsyncWrite};

    /// Starts a stdio server as a child process, or connects over Streamable HTTP (rmcp
    /// client). holdmap declares no client capabilities: no sampling, elicitation or roots.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct RmcpConnector;

    impl McpConnector for RmcpConnector {
        fn connect(
            &self,
            endpoint: McpEndpoint,
            sandbox: &dyn Sandbox,
            timeout: Duration,
        ) -> Result<Box<dyn McpSession>, String> {
            let rt = runtime()?;
            match endpoint {
                McpEndpoint::Stdio(cmd) => {
                    let mut cmd = tokio::process::Command::from(cmd);
                    cmd.stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .kill_on_drop(true);
                    #[cfg(unix)]
                    cmd.process_group(0);
                    let mut child = {
                        let _enter = rt.enter();
                        cmd.spawn()
                            .map_err(|_| "the program could not be started".to_string())?
                    };
                    let guard = sandbox.contain(ProcessRef {
                        id: child.id().unwrap_or(0),
                        #[cfg(windows)]
                        handle: child.raw_handle().unwrap_or(std::ptr::null_mut()),
                    });
                    let (Some(stdout), Some(stdin)) = (child.stdout.take(), child.stdin.take())
                    else {
                        return Err("its input and output could not be connected".into());
                    };
                    let mut session = RmcpSession::handshake(rt, stdout, stdin, timeout)?;
                    session.child = Some(child);
                    session._guard = guard;
                    Ok(Box::new(session))
                }
                #[cfg(feature = "mcp-http")]
                McpEndpoint::Http { url, bearer, cap } => {
                    // rmcp's HTTP worker spawns onto the runtime when it is created.
                    let _enter = rt.enter();
                    let transport = crate::mcp_http::transport(&url, bearer.as_ref(), cap, timeout);
                    let client = rt
                        .block_on(tokio::time::timeout(timeout, ().serve(transport)))
                        .map_err(|_| "it didn't answer within the time limit".to_string())?
                        .map_err(|_| "it didn't complete the MCP handshake".to_string())?;
                    drop(_enter);
                    Ok(Box::new(RmcpSession::listed(rt, client, timeout)?))
                }
                #[cfg(not(feature = "mcp-http"))]
                McpEndpoint::Http { .. } => {
                    let _ = (rt, sandbox);
                    Err("this build of holdmap can't reach MCP servers over HTTP".into())
                }
            }
        }
    }

    fn runtime() -> Result<tokio::runtime::Runtime, String> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "holdmap couldn't prepare the connection".to_string())
    }

    /// One connected server.
    pub struct RmcpSession {
        rt: tokio::runtime::Runtime,
        client: Option<RunningService<RoleClient, ()>>,
        server: Option<ServerInfo>,
        tools: Vec<ToolInfo>,
        child: Option<tokio::process::Child>,
        _guard: Option<ContainGuard>,
    }

    impl RmcpSession {
        /// Complete the MCP handshake over `read`/`write` on `rt` and list the tools. Used with
        /// a child's stdio in production and with an in-process fake in tests.
        pub fn handshake<R, W>(
            rt: tokio::runtime::Runtime,
            read: R,
            write: W,
            timeout: Duration,
        ) -> Result<Self, String>
        where
            R: AsyncRead + Send + Unpin + 'static,
            W: AsyncWrite + Send + Unpin + 'static,
        {
            let _enter = rt.enter();
            let client = rt
                .block_on(tokio::time::timeout(timeout, ().serve((read, write))))
                .map_err(|_| "it didn't finish starting within the time limit".to_string())?
                .map_err(|_| "it didn't complete the MCP handshake".to_string())?;
            drop(_enter);
            Self::listed(rt, client, timeout)
        }

        fn listed(
            rt: tokio::runtime::Runtime,
            client: RunningService<RoleClient, ()>,
            timeout: Duration,
        ) -> Result<Self, String> {
            let _enter = rt.enter();
            let tools = rt
                .block_on(tokio::time::timeout(timeout, client.list_all_tools()))
                .map_err(|_| "it didn't list its tools within the time limit".to_string())?
                .map_err(|_| "it didn't list its tools".to_string())?
                .iter()
                .filter_map(|t| serde_json::to_value(t).ok())
                .filter_map(|v| ToolInfo::from_json(&v))
                .collect();
            let server = client.peer_info().and_then(|info| {
                let v = serde_json::to_value(&*info).ok()?;
                let s = v.get("serverInfo")?;
                Some(ServerInfo {
                    name: s.get("name")?.as_str()?.to_string(),
                    version: s.get("version").and_then(Value::as_str).map(str::to_string),
                    instructions: v
                        .get("instructions")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
            });
            drop(_enter);
            Ok(Self {
                rt,
                client: Some(client),
                server,
                tools,
                child: None,
                _guard: None,
            })
        }

        /// The runtime this session runs on (tests spawn their fake server here).
        pub fn runtime() -> Result<tokio::runtime::Runtime, String> {
            runtime()
        }

        fn stop(&mut self) {
            let _enter = self.rt.enter();
            if let Some(client) = self.client.take() {
                let _ = self.rt.block_on(tokio::time::timeout(
                    Duration::from_secs(2),
                    client.cancel(),
                ));
            }
            if let Some(mut child) = self.child.take() {
                #[cfg(unix)]
                if let Some(pid) = child.id().and_then(|p| i32::try_from(p).ok()) {
                    // SAFETY: signals the process group created with process_group(0) above.
                    unsafe { libc::kill(-pid, libc::SIGKILL) };
                }
                let _ = child.start_kill();
                let _ = self
                    .rt
                    .block_on(tokio::time::timeout(Duration::from_secs(2), child.wait()));
            }
            self._guard = None;
        }
    }

    impl McpSession for RmcpSession {
        fn server(&self) -> Option<ServerInfo> {
            self.server.clone()
        }

        fn tools(&self) -> Vec<ToolInfo> {
            self.tools.clone()
        }

        fn call(
            &mut self,
            tool: &str,
            args: Map<String, Value>,
            timeout: Duration,
            cap: usize,
            cancel: &CancelFlag,
        ) -> Result<ToolOutput, String> {
            let client = self
                .client
                .as_ref()
                .ok_or_else(|| "The MCP server is no longer running.".to_string())?;
            let _enter = self.rt.enter();
            let mut params = CallToolRequestParams::new(tool.to_string());
            params.arguments = Some(args);
            let request = ClientRequest::CallToolRequest(CallToolRequest::new(params));
            let mut options = PeerRequestOptions::no_options();
            // rmcp tells the server (notifications/cancelled) when this expires.
            options.timeout = Some(timeout);
            let peer = client.peer().clone();
            let cancel = cancel.clone();
            let result = self.rt.block_on(async move {
                let handle = peer
                    .send_cancellable_request(request, options)
                    .await
                    .map_err(|_| "The MCP server failed to answer the tool call.".to_string())?;
                let id = handle.id.clone();
                let watch = async {
                    while !cancel.is_cancelled() {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                };
                tokio::select! {
                    r = handle.await_response() => r.map_err(|e| match e {
                        rmcp::ServiceError::Timeout { .. } => "Stopped at the time limit.".to_string(),
                        _ => "The MCP server failed to answer the tool call.".to_string(),
                    }),
                    () = watch => {
                        let _ = peer
                            .notify_cancelled(CancelledNotificationParam::new(
                                Some(id),
                                Some("cancelled in holdmap".into()),
                            ))
                            .await;
                        Err(crate::runner::CANCELLED.to_string())
                    }
                }
            })?;
            let ServerResult::CallToolResult(result) = result else {
                return Err(
                    "The MCP server answered with something other than a tool result.".into(),
                );
            };
            let (bytes, structured) = match &result.structured_content {
                Some(v) if !v.is_null() => (serde_json::to_vec(v).unwrap_or_default(), true),
                _ => (
                    result
                        .content
                        .iter()
                        .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                        .collect::<Vec<_>>()
                        .join("\n")
                        .into_bytes(),
                    false,
                ),
            };
            let truncated = bytes.len() > cap;
            Ok(ToolOutput {
                bytes: if truncated { Vec::new() } else { bytes },
                structured,
                is_error: result.is_error == Some(true),
                truncated,
            })
        }

        fn close(mut self: Box<Self>) {
            self.stop();
        }
    }

    impl Drop for RmcpSession {
        fn drop(&mut self) {
            self.stop();
        }
    }
}
