//! `holdmap analyzers add --mcp`, `servers`, `tools` and `map`: plug any MCP server in as an
//! analyzer. Register it from a command (stdio), a URL (Streamable HTTP) or one of your agents'
//! configured servers; list its tools (this starts or contacts the server, after you confirm);
//! choose tools and map their inputs, guided by each tool's input schema; map where findings
//! sit in the output. Nothing is called until you approve it.

use crate::commands::confirm;
use crate::style::{self, bold, dim, paint, S};
use anyhow::{bail, Context, Result};
use clap::Args;
use holdmap_analyzers::manifest::{ToolSpec, Transport};
use holdmap_analyzers::mcp_map::OutputMapping;
use holdmap_analyzers::service::{ConfiguredServer, McpDraft, ServiceError, ToolReport};
use holdmap_analyzers::AnalyzerService;
use holdmap_core::agents::AgentsReport;
use holdmap_core::util::printable;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Flags for `holdmap analyzers add --mcp`.
#[derive(Args, Debug, Default)]
pub struct McpAddArgs {
    /// Register an MCP server (with --command, --url or --from) instead of a manifest file.
    #[arg(long, conflicts_with = "file")]
    pub mcp: bool,
    /// Id for it, e.g. local.my-scanner.
    #[arg(long, requires = "mcp")]
    pub id: Option<String>,
    /// Display name.
    #[arg(long, requires = "mcp")]
    pub name: Option<String>,
    /// Program that starts a stdio MCP server (holdmap starts it, sandboxed, only for runs).
    #[arg(long, requires = "mcp", conflicts_with_all = ["url", "from"])]
    pub command: Option<String>,
    /// An argument for --command (repeat for several).
    #[arg(long = "arg", requires = "command", allow_hyphen_values = true)]
    pub args: Vec<String>,
    /// A Streamable HTTP endpoint. Servers on another machine need --allow-remote when you
    /// approve and only get metadata (names, file names, agents).
    #[arg(long, requires = "mcp", conflicts_with = "from")]
    pub url: Option<String>,
    /// One of your agents' configured servers, as listed by `holdmap analyzers servers`.
    #[arg(long, requires = "mcp")]
    pub from: Option<String>,
    /// An API key name the server may get from your keychain (repeat for several).
    #[arg(long = "secret", requires = "mcp")]
    pub secrets: Vec<String>,
    /// The key sent as a bearer token to an HTTP server.
    #[arg(long, requires = "url")]
    pub auth: Option<String>,
}

/// Flags for `holdmap analyzers map`.
#[derive(Args, Debug)]
pub struct MapArgs {
    pub id: String,
    /// A tool to call (repeat to choose several, in call order).
    #[arg(long = "tool")]
    pub tools: Vec<String>,
    /// An input for the single --tool: NAME=VALUE. VALUE is a placeholder such as
    /// {subject.dir}, plain text, or JSON (numbers, true/false, lists).
    #[arg(long = "arg", requires = "tools")]
    pub args: Vec<String>,
    /// Use holdmap's suggested inputs for each --tool (without --tool: every tool it can fill
    /// in completely that has no warnings and isn't marked destructive).
    #[arg(long)]
    pub auto: bool,
    /// Remove the --tool(s) instead.
    #[arg(long, requires = "tools", conflicts_with_all = ["args", "auto"])]
    pub remove: bool,
    /// Where findings sit in JSON output, e.g. $.issues[*] (needs --rule-id).
    #[arg(long)]
    pub items: Option<String>,
    /// Rule id field inside one item.
    #[arg(long)]
    pub rule_id: Option<String>,
    /// Severity field.
    #[arg(long)]
    pub severity: Option<String>,
    /// Title field.
    #[arg(long)]
    pub title: Option<String>,
    /// File field.
    #[arg(long)]
    pub file: Option<String>,
    /// Line field.
    #[arg(long)]
    pub line: Option<String>,
    /// Use the output mapping suggested from a tool's output schema.
    #[arg(long, conflicts_with = "items")]
    pub auto_output: bool,
    /// Forget the output mapping (SARIF and holdmap JSON are still recognised; anything else
    /// becomes a short excerpt).
    #[arg(long, conflicts_with_all = ["items", "auto_output"])]
    pub clear_output: bool,
}

fn blocked(e: ServiceError) -> Result<u8> {
    match e {
        ServiceError::Manifest(_) | ServiceError::Run(_) | ServiceError::Unknown(_) => {
            eprintln!("{} {e}", style::err_mark());
            Ok(crate::exit::BLOCKED)
        }
        e => Err(e.into()),
    }
}

/// `holdmap analyzers add --mcp ...`.
pub fn add(
    svc: &AnalyzerService,
    a: &McpAddArgs,
    report: impl FnOnce() -> Result<AgentsReport>,
    json_out: bool,
) -> Result<u8> {
    let mut draft = if let Some(from) = &a.from {
        let servers = svc.configured_servers(&report()?);
        let s = find_server(&servers, from)?;
        let mut d = s.draft.clone().with_context(|| {
            format!(
                "{} can't be added from your agent's configuration: {}",
                s.name,
                s.note.as_deref().unwrap_or("unknown launch")
            )
        })?;
        if let Some(note) = &s.note {
            eprintln!("{} {note}", style::warn_mark());
        }
        eprintln!(
            "{} {} is also one of your agents' servers. As an analyzer it starts separately, with none of your variables, and only when you run it.",
            style::warn_mark(),
            printable(&s.name)
        );
        d.secrets.extend(a.secrets.iter().cloned());
        d
    } else if let Some(url) = &a.url {
        McpDraft {
            transport: Transport::Http,
            url: Some(url.clone()),
            auth_secret: a.auth.clone(),
            ..McpDraft::default()
        }
    } else if let Some(command) = &a.command {
        McpDraft {
            command: Some(command.clone()),
            args: a.args.clone(),
            secrets: a.secrets.clone(),
            ..McpDraft::default()
        }
    } else {
        bail!("--mcp needs --command, --url or --from");
    };
    if let Some(id) = &a.id {
        draft.id = id.clone();
    }
    if let Some(name) = &a.name {
        draft.name = name.clone();
    }
    if draft.id.is_empty() {
        bail!("--id is required, e.g. --id local.my-scanner");
    }
    if draft.name.is_empty() {
        draft.name = draft.id.clone();
    }
    let view = match svc.add_mcp(&draft) {
        Ok(v) => v,
        Err(e) => return blocked(e),
    };
    if json_out {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else {
        println!(
            "{} Registered {} ({}). It is off. Next:\n  holdmap analyzers tools {}   (lists its tools; starts or contacts the server, calls nothing)\n  holdmap analyzers map {} --auto\n  holdmap analyzers approve {}",
            style::ok_mark(),
            view.name,
            view.id,
            view.id,
            view.id,
            view.id
        );
    }
    Ok(crate::exit::OK)
}

fn find_server<'a>(servers: &'a [ConfiguredServer], key: &str) -> Result<&'a ConfiguredServer> {
    let k = key.to_ascii_lowercase();
    servers
        .iter()
        .find(|s| {
            s.id.eq_ignore_ascii_case(key)
                || format!("{}/{}", s.agent, s.name).to_ascii_lowercase() == k
        })
        .with_context(|| {
            format!("no configured MCP server {key:?}; see `holdmap analyzers servers`")
        })
}

/// `holdmap analyzers servers`.
pub fn servers(svc: &AnalyzerService, report: &AgentsReport, json_out: bool) -> Result<u8> {
    let list = svc.configured_servers(report);
    if json_out {
        println!("{}", serde_json::to_string_pretty(&list)?);
        return Ok(crate::exit::OK);
    }
    if list.is_empty() {
        println!("No MCP servers found in your running agents' configuration.");
    }
    for s in &list {
        let target = s
            .draft
            .as_ref()
            .map(|d| match d.transport {
                Transport::Stdio => {
                    format!(
                        "{} {}",
                        d.command.clone().unwrap_or_default(),
                        d.args.join(" ")
                    )
                }
                Transport::Http => d.url.clone().unwrap_or_default(),
            })
            .unwrap_or_else(|| dim("can't be added"));
        println!(
            "{}  {}  {}  {}",
            bold(printable(&s.name)),
            dim(format!("{}/{}", s.agent, s.name)),
            s.transport,
            printable(target.trim())
        );
        if let Some(note) = &s.note {
            println!("  {}", dim(note));
        }
    }
    println!(
        "{}",
        dim("Add one with: holdmap analyzers add --mcp --from AGENT/NAME --id local.NAME")
    );
    Ok(crate::exit::OK)
}

/// `holdmap analyzers tools ID [--refresh] [--yes]`.
pub fn tools(
    svc: &AnalyzerService,
    id: &str,
    refresh: bool,
    yes: bool,
    json_out: bool,
) -> Result<u8> {
    let saved = svc.tools(id).ok();
    let report = match saved {
        Some(r) if !refresh => r,
        _ => {
            let view = svc.get(id)?;
            let mcp = view
                .mcp
                .as_ref()
                .with_context(|| format!("{} is not an MCP analyzer", view.name))?;
            let what = match mcp.transport {
                Transport::Stdio => format!("start {} in the sandbox", mcp.endpoint),
                Transport::Http => format!("connect to {}", mcp.endpoint),
            };
            if !yes && !confirm(&format!("List its tools? holdmap will {what}, ask for the tool list, and stop. No tool is called."))? {
                eprintln!("Nothing started.");
                return Ok(crate::exit::BUSY);
            }
            match svc.list_tools(id) {
                Ok(r) => r,
                Err(e) => return blocked(e),
            }
        }
    };
    if json_out {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", style::terminal_text(&render_tools(id, &report)));
    }
    Ok(crate::exit::OK)
}

fn first_line(s: &str, max: usize) -> String {
    let line = s
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let mut out: String = printable(line).chars().take(max).collect();
    if line.chars().count() > max {
        out.push('…');
    }
    out
}

fn render_tools(id: &str, r: &ToolReport) -> String {
    let s = &r.snapshot;
    let mut out = String::new();
    if let Some(server) = &s.server {
        out.push_str(&format!(
            "{} {}  {}\n",
            bold(printable(&server.name)),
            server.version.as_deref().map(printable).unwrap_or_default(),
            dim(format!("pin {}", &s.sha256[..12.min(s.sha256.len())]))
        ));
    }
    out.push_str(&format!("{}\n", dim(printable(&s.endpoint))));
    for f in &r.server_flags {
        out.push_str(&format!(
            "{} server instructions: {f}\n",
            style::warn_mark()
        ));
    }
    if r.approved {
        out.push_str(&format!(
            "{} This is the tool list you approved.\n",
            style::ok_mark()
        ));
    } else if !r.changes.is_empty() {
        out.push_str(&format!(
            "{} Changed since you approved it (approve again to run):\n",
            style::warn_mark()
        ));
        for c in &r.changes {
            out.push_str(&format!("  {}\n", printable(c)));
        }
    }
    if s.truncated {
        out.push_str(&format!(
            "{} The server lists more tools than holdmap keeps.\n",
            style::warn_mark()
        ));
    }
    out.push('\n');
    for t in &s.tools {
        let chosen = r.chosen.iter().find(|c| c.name == t.name);
        out.push_str(&format!(
            "{} {}{}\n",
            if chosen.is_some() {
                paint("●", S::Green)
            } else {
                dim("○")
            },
            bold(printable(&t.name)),
            t.title
                .as_deref()
                .map(|x| format!("  {}", printable(x)))
                .unwrap_or_default()
        ));
        if let Some(d) = &t.description {
            out.push_str(&format!("  {}\n", dim(first_line(d, 100))));
        }
        for f in r.flags.get(&t.name).into_iter().flatten() {
            out.push_str(&format!(
                "  {} {}\n",
                style::warn_mark(),
                paint(f, S::Yellow)
            ));
        }
        let args = chosen.map(|c| &c.args).or_else(|| {
            r.suggestions
                .iter()
                .find(|x| x.tool == t.name)
                .map(|x| &x.args)
        });
        let label = if chosen.is_some() {
            "inputs"
        } else {
            "suggested"
        };
        if let Some(args) = args.filter(|a| !a.is_empty()) {
            let pairs: Vec<String> = args
                .iter()
                .map(|(k, v)| format!("{k}={}", show(v)))
                .collect();
            out.push_str(&format!("  {label}: {}\n", pairs.join("  ")));
        }
        if let Some(sug) = r.suggestions.iter().find(|x| x.tool == t.name) {
            if !sug.missing.is_empty() && chosen.is_none() {
                out.push_str(&format!(
                    "  {}\n",
                    dim(format!("needs: {}", sug.missing.join(", ")))
                ));
            }
        }
    }
    if !r.output.is_empty() {
        out.push_str(&format!(
            "\nOutput: findings at {} (rule id {})\n",
            r.output.items.as_deref().unwrap_or(""),
            r.output.rule_id.as_deref().unwrap_or("?")
        ));
    }
    out.push_str(&dim(format!(
        "\nPlaceholders: {}\nChoose tools: holdmap analyzers map {id} --tool NAME --auto  (or --arg NAME=VALUE)\n",
        r.placeholders.iter().map(|p| p.token.as_str()).collect::<Vec<_>>().join(" ")
    )));
    out
}

fn show(v: &Value) -> String {
    match v {
        Value::String(s) => printable(s).to_string(),
        other => printable(&other.to_string()).to_string(),
    }
}

/// Parse `NAME=VALUE`: JSON when it parses as a number, bool, list or object; text otherwise.
fn parse_arg(raw: &str) -> Result<(String, Value)> {
    let (k, v) = raw
        .split_once('=')
        .with_context(|| format!("--arg {raw:?} must be NAME=VALUE"))?;
    let value = match serde_json::from_str::<Value>(v) {
        Ok(j @ (Value::Number(_) | Value::Bool(_) | Value::Array(_) | Value::Object(_))) => j,
        _ => Value::String(v.to_string()),
    };
    Ok((k.trim().to_string(), value))
}

/// `holdmap analyzers map ID ...`.
pub fn map(svc: &AnalyzerService, m: &MapArgs, json_out: bool) -> Result<u8> {
    let report = match svc.tools(&m.id) {
        Ok(r) => r,
        Err(e) => return blocked(e),
    };
    let tool_flags = m.auto || m.remove || !m.args.is_empty() || !m.tools.is_empty();
    let mut view = None;
    if tool_flags {
        let mut chosen = report.chosen.clone();
        let names: Vec<String> = if m.tools.is_empty() && m.auto {
            // Every tool holdmap can fill in completely, minus flagged and destructive ones.
            report
                .snapshot
                .tools
                .iter()
                .filter(|t| !report.flags.contains_key(&t.name) && !t.destructive())
                .filter(|t| {
                    report
                        .suggestions
                        .iter()
                        .any(|s| s.tool == t.name && s.missing.is_empty() && !s.args.is_empty())
                })
                .map(|t| t.name.clone())
                .collect()
        } else {
            m.tools.clone()
        };
        if !m.args.is_empty() && names.len() != 1 {
            bail!("--arg applies to exactly one --tool");
        }
        for name in &names {
            if report.snapshot.tool(name).is_none() && !m.remove {
                bail!(
                    "{name:?} isn't in the server's tool list; see `holdmap analyzers tools {}`",
                    m.id
                );
            }
            if m.remove {
                chosen.retain(|c| &c.name != name);
                continue;
            }
            let mut args: BTreeMap<String, Value> = if m.auto {
                report
                    .suggestions
                    .iter()
                    .find(|s| &s.tool == name)
                    .map(|s| s.args.clone())
                    .unwrap_or_default()
            } else {
                chosen
                    .iter()
                    .find(|c| &c.name == name)
                    .map(|c| c.args.clone())
                    .unwrap_or_default()
            };
            for raw in &m.args {
                let (k, v) = parse_arg(raw)?;
                args.insert(k, v);
            }
            match chosen.iter_mut().find(|c| &c.name == name) {
                Some(c) => c.args = args,
                None => chosen.push(ToolSpec {
                    name: name.clone(),
                    args,
                }),
            }
        }
        view = Some(match svc.map_tools(&m.id, &chosen) {
            Ok(v) => v,
            Err(e) => return blocked(e),
        });
    }
    let output = if m.clear_output {
        Some(OutputMapping::default())
    } else if m.auto_output {
        let pick = m
            .tools
            .iter()
            .find_map(|t| report.output_suggestions.get(t))
            .or_else(|| report.output_suggestions.values().next())
            .cloned();
        Some(pick.context(
            "no tool's output schema says where findings are; use --items and --rule-id",
        )?)
    } else if m.items.is_some() || m.rule_id.is_some() {
        let mut o = report.output.clone();
        let set = |slot: &mut Option<String>, v: &Option<String>| {
            if v.is_some() {
                slot.clone_from(v);
            }
        };
        set(&mut o.items, &m.items);
        set(&mut o.rule_id, &m.rule_id);
        set(&mut o.severity, &m.severity);
        set(&mut o.title, &m.title);
        set(&mut o.file, &m.file);
        set(&mut o.line, &m.line);
        Some(o)
    } else {
        None
    };
    if let Some(o) = output {
        view = Some(match svc.map_output(&m.id, &o) {
            Ok(v) => v,
            Err(e) => return blocked(e),
        });
    }
    let Some(view) = view else {
        bail!("nothing to change: pass --tool, --auto, --items, --auto-output or --clear-output");
    };
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({ "analyzer": view, "tools": svc.tools(&m.id)?.chosen })
            )?
        );
    } else {
        let tools = svc.tools(&m.id)?;
        for c in &tools.chosen {
            let pairs: Vec<String> = c
                .args
                .iter()
                .map(|(k, v)| format!("{k}={}", show(v)))
                .collect();
            println!(
                "{} {}  {}",
                style::ok_mark(),
                bold(&c.name),
                pairs.join("  ")
            );
        }
        println!(
            "{}",
            dim(format!(
                "Saved. Review the exact calls with `holdmap analyzers show {0}`, then `holdmap analyzers approve {0}`.",
                view.id
            ))
        );
    }
    Ok(crate::exit::OK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_parse_as_json_scalars_and_lists_or_text() {
        assert_eq!(
            parse_arg("path={subject.dir}").unwrap(),
            ("path".into(), json!("{subject.dir}"))
        );
        assert_eq!(parse_arg("depth=2").unwrap().1, json!(2));
        assert_eq!(parse_arg("strict=true").unwrap().1, json!(true));
        assert_eq!(
            parse_arg("tags=[\"a\",\"b\"]").unwrap().1,
            json!(["a", "b"])
        );
        assert_eq!(parse_arg("q=a=b").unwrap(), ("q".into(), json!("a=b")));
        assert_eq!(parse_arg("s=\"quoted\"").unwrap().1, json!("\"quoted\""));
        assert!(parse_arg("novalue").is_err());
    }

    #[test]
    fn descriptions_are_one_printable_line() {
        assert_eq!(first_line("\n  Scan it.\nMore", 100), "Scan it.");
        assert_eq!(
            first_line(&"x".repeat(120), 10),
            format!("{}…", "x".repeat(10))
        );
    }
}
