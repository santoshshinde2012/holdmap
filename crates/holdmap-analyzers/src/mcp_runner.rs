//! Runs a user-registered local MCP analyzer: copy each skill into a private run folder,
//! start the approved server over stdio (cleared environment, same sandbox as CLI analyzers)
//! only for this run, call the declared tool once per skill, then stop the server.
//!
//! The MCP client sits behind [`McpConnector`] so the runner is tested with an in-process
//! fake; production uses [`RmcpConnector`] (feature `mcp-runner`). Server error text is never
//! kept: notes are written by holdmap.

use crate::cli_runner::{finish_env, stage, StagingLimits};
use crate::input::AnalyzerInput;
use crate::manifest::{AnalyzerManifest, SUBJECT_DIR};
use crate::runner::{AnalyzerRunner, Containment, RawOutput, RunContext, RunError, Sandbox};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// What one tool call returned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolOutput {
    /// Structured content as JSON, else the text blocks joined by newlines (capped).
    pub bytes: Vec<u8>,
    /// The tool flagged its result as an error.
    pub is_error: bool,
    /// The result was larger than the output limit and was dropped.
    pub truncated: bool,
}

/// A running MCP server, for the length of one analyzer run.
pub trait McpSession {
    /// Tool names the server lists.
    fn tools(&self) -> Vec<String>;
    /// Call `tool` with `args`, waiting at most `timeout` and keeping at most `cap` bytes.
    /// Errors are holdmap's own words, never server text.
    fn call(
        &mut self,
        tool: &str,
        args: Map<String, Value>,
        timeout: Duration,
        cap: usize,
    ) -> Result<ToolOutput, String>;
    /// Stop the server and everything it started.
    fn close(self: Box<Self>);
}

/// Starts an MCP server from a prepared command.
pub trait McpConnector: Send + Sync {
    /// Start `cmd` (already sandboxed, environment cleared) and complete the handshake within
    /// `timeout`. Errors are holdmap's own words.
    fn connect(
        &self,
        cmd: Command,
        sandbox: &dyn Sandbox,
        timeout: Duration,
    ) -> Result<Box<dyn McpSession>, String>;
}

/// The MCP runner.
pub struct McpRunner {
    sandbox: Box<dyn Sandbox>,
    connector: Box<dyn McpConnector>,
    limits: StagingLimits,
}

impl McpRunner {
    /// A runner starting servers through `connector`, contained by `sandbox`.
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
}

#[cfg(not(feature = "mcp-runner"))]
struct Unsupported;

#[cfg(not(feature = "mcp-runner"))]
impl McpConnector for Unsupported {
    fn connect(
        &self,
        _: Command,
        _: &dyn Sandbox,
        _: Duration,
    ) -> Result<Box<dyn McpSession>, String> {
        Err("this build of holdmap can't run MCP analyzers".into())
    }
}

/// The tool arguments for one staged subject.
pub fn tool_arguments(manifest: &AnalyzerManifest, staged: &Path) -> Map<String, Value> {
    manifest
        .runner
        .tool_args
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                Value::String(v.replace(SUBJECT_DIR, &staged.to_string_lossy())),
            )
        })
        .collect()
}

fn failed(subject: &crate::finding::Subject, note: &str) -> RawOutput {
    RawOutput {
        subjects: vec![subject.clone()],
        exit_code: None,
        bytes: Vec::new(),
        notes: vec![note.to_string()],
        failed: true,
    }
}

impl AnalyzerRunner for McpRunner {
    fn run(
        &self,
        manifest: &AnalyzerManifest,
        input: &AnalyzerInput,
        cx: &RunContext,
    ) -> Result<Vec<RawOutput>, RunError> {
        let binary = cx
            .binary
            .as_deref()
            .ok_or_else(|| RunError::Unavailable(format!("{} is not installed.", manifest.name)))?;
        if input.subjects.is_empty() {
            return Err(RunError::NoSubjects("no skills were found".into()));
        }
        let tool = manifest.runner.tool.as_deref().unwrap_or_default();
        let work = tempfile::Builder::new()
            .prefix("holdmap-analyzer-")
            .tempdir()
            .map_err(|_| {
                RunError::Unavailable("Could not create a private staging folder.".into())
            })?;
        let home = work.path().join("home");
        let _ = std::fs::create_dir_all(&home);

        // Stage every subject first; the server only ever sees these copies.
        let mut outputs: Vec<Option<RawOutput>> = Vec::new();
        let mut staged: Vec<Option<PathBuf>> = Vec::new();
        for (i, s) in input.subjects.iter().enumerate() {
            let Some(dir) = s.dir.as_deref() else {
                outputs.push(Some(failed(&s.subject, "No folder to analyse.")));
                staged.push(None);
                continue;
            };
            let to = work.path().join("subjects").join(i.to_string());
            match stage(dir, &to, self.limits) {
                Ok(st) => {
                    outputs.push(st.truncated.then(|| RawOutput {
                        subjects: vec![s.subject.clone()],
                        exit_code: None,
                        bytes: Vec::new(),
                        notes: vec!["The folder is larger than the staging limit; only part of it was analysed.".into()],
                        failed: false,
                    }));
                    staged.push(Some(to));
                }
                Err(_) => {
                    outputs.push(Some(failed(&s.subject, "The folder could not be read.")));
                    staged.push(None);
                }
            }
        }

        let mut cmd = self.sandbox.command(
            binary,
            &manifest.runner.args,
            &Containment {
                work: work.path(),
                home: &home,
                network: manifest.uses_network(),
            },
        );
        cmd.current_dir(work.path());
        finish_env(&mut cmd, manifest, binary, &cx.secrets);

        let timeout = Duration::from_secs(manifest.runner.timeout_secs);
        let deadline = Instant::now() + timeout;
        let cap = (manifest.runner.max_output_mb * 1024 * 1024) as usize;
        let all_failed = |note: &str| -> Vec<RawOutput> {
            input
                .subjects
                .iter()
                .map(|s| failed(&s.subject, note))
                .collect()
        };
        let mut session = match self.connector.connect(cmd, self.sandbox.as_ref(), timeout) {
            Ok(s) => s,
            Err(why) => {
                return Ok(all_failed(&format!(
                    "The MCP server could not be started: {why}."
                )))
            }
        };
        if !session.tools().iter().any(|t| t == tool) {
            session.close();
            return Ok(all_failed(&format!(
                "The MCP server doesn't offer the tool {tool:?}."
            )));
        }
        let mut result = Vec::new();
        for ((s, pre), dir) in input.subjects.iter().zip(outputs).zip(staged) {
            let mut out = pre.unwrap_or_else(|| RawOutput {
                subjects: vec![s.subject.clone()],
                exit_code: None,
                bytes: Vec::new(),
                notes: Vec::new(),
                failed: true,
            });
            let Some(dir) = dir else {
                result.push(out);
                continue;
            };
            out.failed = true;
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                out.notes.push(format!(
                    "Stopped after the {}-second time limit.",
                    manifest.runner.timeout_secs
                ));
                result.push(out);
                continue;
            }
            match session.call(tool, tool_arguments(manifest, &dir), left, cap) {
                Ok(t) if t.is_error => out.notes.push("The tool reported an error.".into()),
                Ok(t) if t.truncated => out
                    .notes
                    .push("The output was larger than the output limit and was not read.".into()),
                Ok(t) if t.bytes.is_empty() => {
                    out.notes.push("The tool returned no report.".into())
                }
                Ok(t) => {
                    out.bytes = t.bytes;
                    out.failed = false;
                }
                Err(why) => out.notes.push(why),
            }
            result.push(out);
        }
        session.close();
        Ok(result)
    }
}

#[cfg(feature = "mcp-runner")]
pub use rmcp_client::{RmcpConnector, RmcpSession};

#[cfg(feature = "mcp-runner")]
mod rmcp_client {
    use super::{McpConnector, McpSession, ToolOutput};
    use crate::runner::{ContainGuard, ProcessRef, Sandbox};
    use rmcp::model::CallToolRequestParams;
    use rmcp::service::RunningService;
    use rmcp::{RoleClient, ServiceExt};
    use serde_json::{Map, Value};
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use tokio::io::{AsyncRead, AsyncWrite};

    /// Starts the server as a child process speaking MCP over stdio (rmcp client).
    #[derive(Debug, Default, Clone, Copy)]
    pub struct RmcpConnector;

    impl McpConnector for RmcpConnector {
        fn connect(
            &self,
            cmd: Command,
            sandbox: &dyn Sandbox,
            timeout: Duration,
        ) -> Result<Box<dyn McpSession>, String> {
            let rt = runtime()?;
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
            let (Some(stdout), Some(stdin)) = (child.stdout.take(), child.stdin.take()) else {
                return Err("its input and output could not be connected".into());
            };
            let mut session = RmcpSession::handshake(rt, stdout, stdin, timeout)?;
            session.child = Some(child);
            session._guard = guard;
            Ok(Box::new(session))
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
        tools: Vec<String>,
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
            let tools = rt
                .block_on(tokio::time::timeout(timeout, client.list_all_tools()))
                .map_err(|_| "it didn't list its tools within the time limit".to_string())?
                .map_err(|_| "it didn't list its tools".to_string())?
                .into_iter()
                .map(|t| t.name.into_owned())
                .collect();
            drop(_enter);
            Ok(Self {
                rt,
                client: Some(client),
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
        fn tools(&self) -> Vec<String> {
            self.tools.clone()
        }

        fn call(
            &mut self,
            tool: &str,
            args: Map<String, Value>,
            timeout: Duration,
            cap: usize,
        ) -> Result<ToolOutput, String> {
            let client = self
                .client
                .as_ref()
                .ok_or_else(|| "The MCP server is no longer running.".to_string())?;
            let _enter = self.rt.enter();
            let mut params = CallToolRequestParams::new(tool.to_string());
            params.arguments = Some(args);
            let result = self
                .rt
                .block_on(tokio::time::timeout(timeout, client.call_tool(params)))
                .map_err(|_| "Stopped at the time limit.".to_string())?
                .map_err(|_| "The MCP server failed to answer the tool call.".to_string())?;
            let bytes = match &result.structured_content {
                Some(v) if !v.is_null() => serde_json::to_vec(v).unwrap_or_default(),
                _ => result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .into_bytes(),
            };
            let truncated = bytes.len() > cap;
            Ok(ToolOutput {
                bytes: if truncated { Vec::new() } else { bytes },
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
