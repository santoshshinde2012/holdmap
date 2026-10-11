//! Opt-in projection and bounded delivery of this MCP server's tool metadata.
//!
//! The observer never writes protocol output or blocks dispatch on collector I/O.
//! Dropping it closes the bounded queue before joining its delivery worker.

use super::client_from_environment;
use anyhow::Result;
use holdmap_activity::client::LocalClient;
use holdmap_activity::{ActivityKind, ActivitySpan, ActivityStatus, Provider, Source};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::mpsc;

struct McpObserver {
    session_id: String,
    pending: HashMap<u64, ActivitySpan>,
    sender: Option<mpsc::SyncSender<ActivitySpan>>,
    worker: Option<std::thread::JoinHandle<()>>,
    warned: Cell<bool>,
}

impl McpObserver {
    fn new(client: LocalClient) -> Result<Self> {
        let mut entropy = [0; 32];
        getrandom::fill(&mut entropy)
            .map_err(|_| anyhow::anyhow!("Couldn't prepare activity correlation."))?;
        let raw_session: String = entropy.iter().map(|byte| format!("{byte:02x}")).collect();
        let session_id = holdmap_activity::pseudonymize_id("mcp-session", &raw_session)?;
        let (sender, receiver) = mpsc::sync_channel::<ActivitySpan>(32);
        let worker = std::thread::Builder::new()
            .name("holdmap-mcp-activity".into())
            .spawn(move || {
                let mut warned = false;
                while let Ok(span) = receiver.recv() {
                    let mut batch = vec![span];
                    batch.extend(receiver.try_iter().take(31));
                    if client.record(&batch).is_err() && !warned {
                        warned = true;
                        eprintln!("holdmap: MCP activity metadata was not recorded.");
                    }
                }
            })?;
        Ok(Self {
            session_id,
            pending: HashMap::new(),
            sender: Some(sender),
            worker: Some(worker),
            warned: Cell::new(false),
        })
    }

    fn send(&self, span: ActivitySpan) {
        if let Some(sender) = &self.sender {
            if sender.try_send(span).is_err() && !self.warned.replace(true) {
                eprintln!("holdmap: MCP activity queue is full; metadata was dropped.");
            }
        }
    }
}

impl holdmap_mcp::ToolCallObserver for McpObserver {
    fn started(&mut self, event: &holdmap_mcp::ToolCallStarted) {
        let Ok(id) = holdmap_activity::pseudonymize_id(
            "mcp-call",
            &format!("{}:{}", self.session_id, event.call_id),
        ) else {
            return;
        };
        let span = ActivitySpan {
            id,
            trace_id: String::new(),
            parent_id: None,
            session_id: Some(self.session_id.clone()),
            agent_id: None,
            parent_agent_id: None,
            provider: Provider::Holdmap,
            source: Source::Mcp,
            kind: ActivityKind::Tool,
            name: event.tool_name.clone(),
            start_at_ms: Some(event.started_at_ms),
            end_at_ms: None,
            duration_ms: None,
            status: ActivityStatus::Running,
            input_bytes: Some(event.input_bytes),
            output_bytes: None,
            pid: None,
            process_started_at: None,
        };
        self.send(span.clone());
        self.pending.insert(event.call_id, span);
    }

    fn completed(&mut self, event: &holdmap_mcp::ToolCallCompleted) {
        let Some(mut span) = self.pending.remove(&event.call_id) else {
            return;
        };
        span.end_at_ms = Some(event.finished_at_ms);
        span.duration_ms = Some(event.duration_ms);
        span.output_bytes = Some(event.output_bytes);
        span.status = match event.outcome {
            holdmap_mcp::ToolCallOutcome::Success => ActivityStatus::Ok,
            holdmap_mcp::ToolCallOutcome::ToolError
            | holdmap_mcp::ToolCallOutcome::ProtocolError => ActivityStatus::Error,
        };
        self.send(span);
    }
}

impl Drop for McpObserver {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub(super) fn run(record_activity: bool) -> Result<u8> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    if record_activity {
        match client_from_environment().and_then(McpObserver::new) {
            Ok(mut observer) => {
                holdmap_mcp::serve_with_observer(stdin.lock(), stdout.lock(), &mut observer)?
            }
            Err(_) => {
                eprintln!("holdmap: activity setup is unavailable; MCP tools remain available.");
                holdmap_mcp::serve(stdin.lock(), stdout.lock())?;
            }
        }
    } else {
        holdmap_mcp::serve(stdin.lock(), stdout.lock())?;
    }
    Ok(crate::exit::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use holdmap_mcp::{ToolCallCompleted, ToolCallObserver, ToolCallOutcome, ToolCallStarted};
    use std::time::Duration;

    fn observer(capacity: usize) -> (McpObserver, mpsc::Receiver<ActivitySpan>) {
        let (sender, receiver) = mpsc::sync_channel(capacity);
        (
            McpObserver {
                session_id: holdmap_activity::pseudonymize_id("mcp-session", "fixture-session")
                    .unwrap(),
                pending: HashMap::new(),
                sender: Some(sender),
                worker: None,
                warned: Cell::new(false),
            },
            receiver,
        )
    }

    fn start(call_id: u64) -> ToolCallStarted {
        ToolCallStarted {
            call_id,
            tool_name: "list_agents".into(),
            started_at_ms: 1_000,
            input_bytes: 128,
        }
    }

    fn finish(call_id: u64, outcome: ToolCallOutcome) -> ToolCallCompleted {
        ToolCallCompleted {
            call_id,
            finished_at_ms: 1_050,
            duration_ms: 50,
            output_bytes: 256,
            outcome,
            error_code: (outcome == ToolCallOutcome::ProtocolError).then_some(-32602),
        }
    }

    #[test]
    fn starts_project_only_metadata_without_claiming_process_or_trace_identity() {
        let (mut observer, receiver) = observer(1);
        observer.started(&start(7));
        let span = receiver.try_recv().unwrap();
        assert_eq!(span.status, ActivityStatus::Running);
        assert_eq!(span.provider, Provider::Holdmap);
        assert_eq!(span.source, Source::Mcp);
        assert_eq!(span.kind, ActivityKind::Tool);
        assert_eq!(span.name, "list_agents");
        assert_eq!(span.start_at_ms, Some(1_000));
        assert_eq!(span.input_bytes, Some(128));
        assert_eq!(
            span.session_id.as_deref(),
            Some(observer.session_id.as_str())
        );
        assert!(span.id.starts_with("hm1_"));
        assert!(!span.id.contains("fixture-session"));
        assert!(span.trace_id.is_empty());
        assert!(span.parent_id.is_none());
        assert!(span.agent_id.is_none());
        assert!(span.parent_agent_id.is_none());
        assert!(span.pid.is_none());
        assert!(span.process_started_at.is_none());
        assert!(span.end_at_ms.is_none());
        assert!(span.duration_ms.is_none());
        assert!(span.output_bytes.is_none());
        assert_eq!(observer.pending.get(&7), Some(&span));
    }

    #[test]
    fn completion_preserves_correlation_and_maps_success_and_both_error_kinds() {
        for (outcome, status) in [
            (ToolCallOutcome::Success, ActivityStatus::Ok),
            (ToolCallOutcome::ToolError, ActivityStatus::Error),
            (ToolCallOutcome::ProtocolError, ActivityStatus::Error),
        ] {
            let (mut observer, receiver) = observer(2);
            observer.started(&start(7));
            let running = receiver.try_recv().unwrap();
            observer.completed(&finish(7, outcome));
            let completed = receiver.try_recv().unwrap();
            assert_eq!(completed.id, running.id);
            assert_eq!(completed.session_id, running.session_id);
            assert_eq!(completed.name, running.name);
            assert_eq!(completed.start_at_ms, running.start_at_ms);
            assert_eq!(completed.input_bytes, running.input_bytes);
            assert_eq!(completed.end_at_ms, Some(1_050));
            assert_eq!(completed.duration_ms, Some(50));
            assert_eq!(completed.output_bytes, Some(256));
            assert_eq!(completed.status, status);
            assert!(observer.pending.is_empty());
            assert!(receiver.try_recv().is_err());
        }
    }

    #[test]
    fn an_unknown_completion_cannot_finish_another_pending_call() {
        let (mut observer, receiver) = observer(1);
        observer.started(&start(7));
        let running = receiver.try_recv().unwrap();
        observer.completed(&finish(8, ToolCallOutcome::Success));
        assert!(receiver.try_recv().is_err());
        assert_eq!(observer.pending.get(&7), Some(&running));
        observer.completed(&finish(7, ToolCallOutcome::Success));
        assert_eq!(receiver.try_recv().unwrap().id, running.id);
        assert!(observer.pending.is_empty());
    }

    #[test]
    fn a_full_queue_drops_updates_without_blocking_or_corrupting_call_tracking() {
        let (mut observer, receiver) = observer(1);
        observer.started(&start(1));
        observer.started(&start(2));
        observer.completed(&finish(2, ToolCallOutcome::Success));
        assert!(observer.warned.get());
        assert!(!observer.pending.contains_key(&2));
        assert_eq!(receiver.try_recv().unwrap().id, observer.pending[&1].id);
        assert!(receiver.try_recv().is_err());
        observer.completed(&finish(1, ToolCallOutcome::Success));
        assert_eq!(receiver.try_recv().unwrap().status, ActivityStatus::Ok);
        assert!(observer.pending.is_empty());
    }

    #[test]
    fn a_disconnected_queue_remains_advisory_and_cleans_up_pending_calls() {
        let (mut observer, receiver) = observer(1);
        drop(receiver);
        observer.started(&start(1));
        assert!(observer.warned.get());
        assert_eq!(observer.pending.len(), 1);
        observer.completed(&finish(1, ToolCallOutcome::ToolError));
        assert!(observer.pending.is_empty());
    }

    #[test]
    fn shutdown_closes_the_queue_before_joining_the_worker() {
        let (mut observer, receiver) = observer(1);
        let (finished, completion) = mpsc::channel();
        observer.worker = Some(std::thread::spawn(move || {
            while receiver.recv().is_ok() {}
            finished.send(()).unwrap();
        }));
        let dropping = std::thread::spawn(move || drop(observer));
        completion
            .recv_timeout(Duration::from_secs(3))
            .expect("worker must see the closed queue before its owner waits for it");
        dropping.join().unwrap();
    }

    #[test]
    fn a_stalled_recorder_cannot_block_protocol_dispatch_or_unboundedly_delay_shutdown() {
        use std::io::Read;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (accepted, acceptance) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let recorder = std::thread::spawn(move || {
            let (mut connection, _) = listener.accept().unwrap();
            connection
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            connection.read_exact(&mut [0; 1]).unwrap();
            accepted.send(()).unwrap();
            let _ = released.recv_timeout(Duration::from_secs(5));
        });
        let client = LocalClient::new(&endpoint, &"a".repeat(64)).unwrap();
        let mut observer = McpObserver::new(client).unwrap();
        let request = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"wait_for_port\",\"arguments\":{\"port\":0}}}\n";
        let mut output = Vec::new();
        holdmap_mcp::serve_with_observer(&request[..], &mut output, &mut observer).unwrap();
        let response: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["isError"], true);
        acceptance.recv_timeout(Duration::from_secs(3)).unwrap();
        let (finished, completion) = mpsc::channel();
        let dropping = std::thread::spawn(move || {
            drop(observer);
            finished.send(()).unwrap();
        });
        let shutdown = completion.recv_timeout(Duration::from_secs(3));
        let _ = release.send(());
        recorder.join().unwrap();
        shutdown.expect("bounded local delivery must not indefinitely hold MCP shutdown");
        dropping.join().unwrap();
    }
}
