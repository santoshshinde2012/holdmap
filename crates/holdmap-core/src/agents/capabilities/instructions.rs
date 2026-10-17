//! Instruction files an agent loads: rules, slash commands, prompts, subagents and memory
//! files such as `AGENTS.md`. Only the file name and an optional frontmatter description
//! are kept; bodies are never decoded past the bounded header.

use super::{frontmatter, CapabilityScope, Evidence, Inventory};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_INSTRUCTIONS: usize = 64;

/// What an instruction file is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionKind {
    /// Always-on or glob-scoped rules (Cursor rules, Copilot instructions, Windsurf/Cline rules).
    Rule,
    /// A slash command.
    Command,
    /// A reusable prompt (Codex prompts, Copilot prompt files).
    Prompt,
    /// A subagent definition.
    Subagent,
    /// A project or account memory file (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`).
    Memory,
}

impl InstructionKind {
    /// Short label shared by text adapters.
    pub fn label(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Command => "command",
            Self::Prompt => "prompt",
            Self::Subagent => "subagent",
            Self::Memory => "memory",
        }
    }
}

/// One instruction file found in a supported location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstructionFile {
    /// Stable display identity; not an authorization handle.
    pub id: String,
    /// What it is for.
    pub kind: InstructionKind,
    /// File name without its extension (memory files keep the full name).
    pub name: String,
    /// Optional sanitized one-line purpose from frontmatter. Never the body.
    pub description: Option<String>,
    /// Account or observed project scope.
    pub scope: CapabilityScope,
    /// Exact file location.
    pub source_path: PathBuf,
    /// Presence was observed; whether the agent loaded it is not established.
    pub evidence: Evidence,
}

/// (relative directory or file, kind, accepted extension; `None` = a single memory file).
type Location = (&'static str, InstructionKind, Option<&'static str>);

fn locations(product: &str, scope: CapabilityScope) -> &'static [Location] {
    use InstructionKind::*;
    let user = scope == CapabilityScope::User;
    match (product, user) {
        ("claude-code", true) => &[
            (".claude/CLAUDE.md", Memory, None),
            (".claude/commands", Command, Some("md")),
            (".claude/agents", Subagent, Some("md")),
        ],
        ("claude-code", false) => &[
            ("CLAUDE.md", Memory, None),
            (".claude/CLAUDE.md", Memory, None),
            (".claude/commands", Command, Some("md")),
            (".claude/agents", Subagent, Some("md")),
        ],
        ("codex" | "codex-desktop", true) => &[
            (".codex/AGENTS.md", Memory, None),
            (".codex/prompts", Prompt, Some("md")),
        ],
        ("codex" | "codex-desktop", false) => &[("AGENTS.md", Memory, None)],
        ("cursor" | "cursor-agent", true) => &[(".cursor/commands", Command, Some("md"))],
        ("cursor" | "cursor-agent", false) => &[
            ("AGENTS.md", Memory, None),
            (".cursor/rules", Rule, Some("mdc")),
            (".cursor/rules", Rule, Some("md")),
            (".cursor/commands", Command, Some("md")),
        ],
        ("gemini", true) => &[
            (".gemini/GEMINI.md", Memory, None),
            (".gemini/commands", Command, Some("toml")),
        ],
        ("gemini", false) => &[
            ("GEMINI.md", Memory, None),
            (".gemini/commands", Command, Some("toml")),
        ],
        ("copilot" | "copilot-cli", false) => &[
            ("AGENTS.md", Memory, None),
            (".github/copilot-instructions.md", Rule, None),
            (".github/instructions", Rule, Some("instructions.md")),
            (".github/prompts", Prompt, Some("prompt.md")),
        ],
        ("windsurf", false) => &[(".windsurf/rules", Rule, Some("md"))],
        ("cline", false) => &[(".clinerules", Rule, Some("md"))],
        _ => &[],
    }
}

impl Inventory<'_> {
    pub(super) fn instruction_location(&mut self, base: &Path, scope: CapabilityScope) {
        if !self.bounded() || !self.safe_path(base) {
            return;
        }
        for (relative, kind, extension) in locations(self.product, scope) {
            let path = base.join(relative);
            match extension {
                None => self.memory_file(&path, *kind, scope),
                Some(extension) => self.instruction_directory(&path, *kind, extension, scope),
            }
        }
    }

    fn memory_file(&mut self, path: &Path, kind: InstructionKind, scope: CapabilityScope) {
        if !self.safe_path(path)
            || !std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
        {
            return;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_owned();
        self.instruction(kind, name, None, scope, path);
    }

    fn instruction_directory(
        &mut self,
        directory: &Path,
        kind: InstructionKind,
        extension: &str,
        scope: CapabilityScope,
    ) {
        if !self.bounded() || !self.safe_path(directory) {
            return;
        }
        self.sources += 1;
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        let remaining = super::MAX_ENTRIES.saturating_sub(self.entries);
        let mut entries: Vec<_> = entries.take(remaining + 1).flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        let suffix = format!(".{extension}");
        for entry in entries {
            if !self.bounded() {
                break;
            }
            self.entries += 1;
            if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Some(stem) = file_name.strip_suffix(&suffix) else {
                continue;
            };
            if !super::safe_name(stem) {
                self.out.limit("Aliases and folder names with unsupported or recognized sensitive text are omitted.");
                continue;
            }
            let path = entry.path();
            let description = if extension == "toml" {
                self.toml_description(&path)
            } else {
                self.read_bytes(&path, frontmatter::MAX_HEADER_BYTES, false)
                    .and_then(|prefix| frontmatter::parse(&prefix.bytes, prefix.at_eof))
                    .and_then(|(_, description)| description)
            };
            self.instruction(kind, stem.to_owned(), description, scope, &path);
        }
    }

    /// Gemini command files are TOML; only the top-level `description` string is kept.
    fn toml_description(&mut self, path: &Path) -> Option<String> {
        let text = self.read(path)?;
        let table = text.parse::<toml::Table>().ok()?;
        table
            .get("description")
            .and_then(toml::Value::as_str)
            .and_then(frontmatter::safe_description)
    }

    fn instruction(
        &mut self,
        kind: InstructionKind,
        name: String,
        description: Option<String>,
        scope: CapabilityScope,
        path: &Path,
    ) {
        if self
            .out
            .instructions
            .iter()
            .any(|entry| entry.source_path == path)
        {
            return;
        }
        if self.out.instructions.len() >= MAX_INSTRUCTIONS {
            self.out
                .limit("Instruction files are limited to 64 per agent context.");
            return;
        }
        self.out.instructions.push(InstructionFile {
            id: super::identity(self.product, kind.label(), scope, path, None, &name),
            kind,
            name,
            description,
            scope,
            source_path: path.to_owned(),
            evidence: Evidence::Observed,
        });
    }
}
