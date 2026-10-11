//! Activity commands, hook forwarding and snapshot presentation.
//!
//! Hook observation is advisory: no stdout, approval decisions or blocking exit codes.

mod mcp;

use anyhow::{bail, Result};
use clap::{Args, Subcommand, ValueEnum};
use holdmap_activity::client::LocalClient;
use holdmap_activity::{
    ActivityCollector, ActivitySnapshot, ActivityStatus, CollectorCredentials, Provider,
};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

const MAX_HOOK_INPUT: u64 = 1024 * 1024;

#[derive(Args, Debug)]
pub struct ActivityArgs {
    #[command(subcommand)]
    command: ActivityCommand,
}

#[derive(Subcommand, Debug)]
enum ActivityCommand {
    /// Start an authenticated loopback recorder; metadata stays in memory until this exits.
    Serve {
        /// Print endpoint and secret token as JSON for explicit integration setup.
        #[arg(long)]
        json: bool,
        /// Stop automatically after this duration (1 second to 24 hours).
        #[arg(long, value_parser = humantime::parse_duration)]
        duration: Option<Duration>,
    },
    /// Read metadata from the recorder named by HOLDMAP_ACTIVITY_ENDPOINT and TOKEN.
    Snapshot {
        /// Machine-readable normalized metadata.
        #[arg(long)]
        json: bool,
    },
    /// Print a reviewable exporter example; never edit agent configuration automatically.
    Setup {
        #[arg(long, value_enum)]
        provider: HookProvider,
        /// Generate PowerShell environment assignments instead of shell exports.
        #[arg(long)]
        windows: bool,
    },
    /// Project one Codex/Claude hook from stdin to metadata and forward it locally.
    Hook {
        #[arg(long, value_enum)]
        provider: HookProvider,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum HookProvider {
    Codex,
    Claude,
}

impl HookProvider {
    fn provider(self) -> Provider {
        match self {
            Self::Codex => Provider::Codex,
            Self::Claude => Provider::Claude,
        }
    }
}

fn credentials_from_environment() -> Result<CollectorCredentials> {
    let credentials = CollectorCredentials {
        endpoint: std::env::var("HOLDMAP_ACTIVITY_ENDPOINT").map_err(|_| {
            anyhow::anyhow!("Set HOLDMAP_ACTIVITY_ENDPOINT to the active local recorder.")
        })?,
        token: std::env::var("HOLDMAP_ACTIVITY_TOKEN").map_err(|_| {
            anyhow::anyhow!("Set HOLDMAP_ACTIVITY_TOKEN to the active recording token.")
        })?,
    };
    LocalClient::from_credentials(&credentials)?;
    Ok(credentials)
}

fn client_from_environment() -> Result<LocalClient> {
    Ok(LocalClient::from_credentials(
        &credentials_from_environment()?,
    )?)
}

pub fn run(arguments: &ActivityArgs) -> Result<u8> {
    match &arguments.command {
        ActivityCommand::Serve { json, duration } => serve(*json, *duration),
        ActivityCommand::Snapshot { json } => {
            let snapshot = client_from_environment()?.snapshot()?;
            if *json {
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            } else {
                print!("{}", render(&snapshot));
            }
            Ok(crate::exit::OK)
        }
        ActivityCommand::Setup { provider, windows } => {
            let credentials = credentials_from_environment()?;
            print!(
                "{}",
                holdmap_activity::integrations::setup(
                    provider.provider(),
                    &credentials,
                    *windows || cfg!(windows),
                )?
            );
            Ok(crate::exit::OK)
        }
        ActivityCommand::Hook { provider } => {
            let result = record_hook(std::io::stdin().lock(), provider.provider());
            if result.is_err() {
                // In particular never exit 2, which agent hooks can treat as a policy block.
                eprintln!("holdmap: activity metadata was not recorded; check the local recorder.");
            }
            Ok(crate::exit::OK)
        }
    }
}

fn serve(json: bool, duration: Option<Duration>) -> Result<u8> {
    if duration.is_some_and(|duration| {
        !(Duration::from_secs(1)..=Duration::from_secs(24 * 60 * 60)).contains(&duration)
    }) {
        bail!("Recording duration must be between 1 second and 24 hours.");
    }
    let collector = ActivityCollector::new();
    let credentials = collector.start()?;
    if json {
        println!("{}", serde_json::to_string(&credentials)?);
    } else {
        println!("Local activity recording started. Metadata stays in memory.");
        println!("Endpoint: {}", credentials.endpoint);
        println!("Token: {}", credentials.token);
        println!("Use these credentials in agent setup. Press Ctrl+C to stop.");
    }
    std::io::stdout().flush()?;
    let started = Instant::now();
    loop {
        let wait = duration
            .map(|duration| duration.saturating_sub(started.elapsed()))
            .unwrap_or(Duration::from_secs(1));
        if wait.is_zero() {
            break;
        }
        std::thread::park_timeout(wait.min(Duration::from_secs(1)));
    }
    let status = collector.stop()?;
    eprintln!(
        "holdmap: recording stopped ({} metadata updates; {} records dropped).",
        status.received, status.dropped
    );
    Ok(crate::exit::OK)
}

fn record_hook(reader: impl Read, provider: Provider) -> Result<()> {
    let mut input = Vec::new();
    reader.take(MAX_HOOK_INPUT + 1).read_to_end(&mut input)?;
    if input.len() as u64 > MAX_HOOK_INPUT {
        bail!("Hook input exceeded its limit.");
    }
    let value: serde_json::Value =
        serde_json::from_slice(&input).map_err(|_| anyhow::anyhow!("Hook input was invalid."))?;
    let spans = holdmap_activity::normalize_hook(provider, &value, now_ms())?;
    if !spans.is_empty() {
        client_from_environment()?.record(&spans)?;
    }
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn render(snapshot: &ActivitySnapshot) -> String {
    let mut output = format!(
        "Recorded agent activity · {} records\nMetadata from configured integrations; process ownership is not verified.\n\n",
        snapshot.spans.len()
    );
    for span in &snapshot.spans {
        let status = match span.status {
            ActivityStatus::Requested => "requested; execution not confirmed",
            ActivityStatus::Running => "running",
            ActivityStatus::Ok => "completed",
            ActivityStatus::Error => "failed",
            ActivityStatus::Cancelled => "cancelled",
            ActivityStatus::Unknown => "status unknown",
        };
        output.push_str(&format!(
            "{} · {status}{}{}{}\n",
            holdmap_core::util::printable(&span.name),
            span.duration_ms
                .map_or(String::new(), |duration| format!(" · {duration} ms")),
            span.input_bytes.map_or(String::new(), |bytes| format!(
                " · input {bytes} serialized bytes"
            )),
            span.output_bytes.map_or(String::new(), |bytes| format!(
                " · output {bytes} serialized bytes"
            )),
        ));
        if let Some(parent) = &span.parent_id {
            output.push_str(&format!(
                "  reported parent span: {}\n",
                holdmap_core::util::printable(parent)
            ));
        }
    }
    if snapshot.spans.is_empty() {
        output
            .push_str("No recorded activity yet. Connect an integration to the active recorder.\n");
    }
    if snapshot.dropped > 0 {
        output.push_str(&format!(
            "\n{} records were rejected or evicted.\n",
            snapshot.dropped
        ));
    }
    for limit in &snapshot.limits {
        output.push_str(&format!("  · {}\n", holdmap_core::util::printable(limit)));
    }
    output
}

pub fn mcp(record_activity: bool) -> Result<u8> {
    mcp::run(record_activity)
}
