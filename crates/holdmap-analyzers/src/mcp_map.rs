//! Input mapping for MCP tools: the placeholders an argument may use (projected from the
//! sanitized inventory only), how a mapping is suggested from a tool's input JSON Schema, how
//! arguments are built for one subject, and the schema checks run before every call.

use crate::mcp_tools::ToolInfo;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// Arguments one tool may take.
pub const MAX_ARGS: usize = 16;
/// Entries in a constant list.
const MAX_LIST: usize = 16;
/// `SKILL.md` text passed to a tool, at most.
pub const MAX_SKILL_MD: usize = 256 * 1024;
/// File names listed for `{subject.files}`, at most.
pub const MAX_FILES: usize = 500;

/// Something from the inventory a tool argument can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placeholder {
    /// `{subject.dir}`: the path of holdmap's private copy of the skill folder.
    Dir,
    /// `{subject.skill_md}`: the text of that copy's `SKILL.md`.
    SkillMd,
    /// `{subject.files}`: relative file names in that copy (a list).
    Files,
    /// `{subject.name}`: the skill's folder name.
    Name,
    /// `{subject.agents}`: ids of the agents that have the skill (a list).
    Agents,
}

impl Placeholder {
    /// Every placeholder.
    pub const ALL: [Self; 5] = [
        Self::Dir,
        Self::SkillMd,
        Self::Files,
        Self::Name,
        Self::Agents,
    ];

    /// The token used in a manifest.
    pub fn token(self) -> &'static str {
        match self {
            Self::Dir => "{subject.dir}",
            Self::SkillMd => "{subject.skill_md}",
            Self::Files => "{subject.files}",
            Self::Name => "{subject.name}",
            Self::Agents => "{subject.agents}",
        }
    }

    /// What the user sees on the permissions sheet.
    pub fn label(self) -> &'static str {
        match self {
            Self::Dir => "path of a private copy of the skill folder",
            Self::SkillMd => "text of the skill's SKILL.md",
            Self::Files => "file names in the skill folder",
            Self::Name => "skill name",
            Self::Agents => "agent ids that have the skill",
        }
    }

    /// Whether it carries file contents or a path to them (never sent to another machine).
    pub fn is_content(self) -> bool {
        matches!(self, Self::Dir | Self::SkillMd)
    }

    /// Whether it expands to a list.
    pub fn is_list(self) -> bool {
        matches!(self, Self::Files | Self::Agents)
    }

    fn from_token(token: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.token() == token)
    }
}

/// The placeholders inside one template string. Unknown `{...}` tokens are an error.
pub fn placeholders(text: &str) -> Result<Vec<Placeholder>, String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let Some(len) = rest[open..].find('}') else {
            return Err("has an unclosed {".into());
        };
        let token = &rest[open..open + len + 1];
        let p =
            Placeholder::from_token(token).ok_or_else(|| format!("unknown placeholder {token}"))?;
        if !out.contains(&p) {
            out.push(p);
        }
        rest = &rest[open + len + 1..];
    }
    if rest.contains('}') {
        return Err("has a stray }".into());
    }
    Ok(out)
}

/// Every placeholder a value uses.
pub fn used(value: &Value) -> Vec<Placeholder> {
    let mut out = Vec::new();
    let mut add = |s: &str| {
        for p in placeholders(s).unwrap_or_default() {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    };
    match value {
        Value::String(s) => add(s),
        Value::Array(items) => items.iter().filter_map(Value::as_str).for_each(add),
        _ => {}
    }
    out
}

/// Check one manifest argument value. `remote` analyzers get metadata only.
pub fn check_template(value: &Value, remote: bool) -> Result<(), String> {
    let check_str = |s: &str, whole_list_ok: bool| -> Result<(), String> {
        let found = placeholders(s)?;
        for p in &found {
            if remote && p.is_content() {
                return Err(format!(
                    "{} is not sent to a server on another machine (metadata only)",
                    p.token()
                ));
            }
            if p.is_list() && !(whole_list_ok && s == p.token()) {
                return Err(format!("{} must be the whole value", p.token()));
            }
        }
        if s.len() > 1024 {
            return Err("is longer than 1024 characters".into());
        }
        Ok(())
    };
    match value {
        Value::String(s) => check_str(s, true),
        Value::Number(_) | Value::Bool(_) => Ok(()),
        Value::Array(items) if items.len() <= MAX_LIST => {
            for item in items {
                match item {
                    Value::String(s) => check_str(s, false)?,
                    Value::Number(_) | Value::Bool(_) => {}
                    _ => return Err("lists may hold only text, numbers and booleans".into()),
                }
            }
            Ok(())
        }
        Value::Array(_) => Err(format!("lists may have at most {MAX_LIST} entries")),
        _ => Err("must be text, a number, a boolean or a list".into()),
    }
}

/// What one subject offers to the placeholders.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubjectData {
    /// Staged folder path.
    pub dir: Option<String>,
    /// `SKILL.md` text (capped).
    pub skill_md: Option<String>,
    /// Relative file names (capped).
    pub files: Vec<String>,
    /// Skill name.
    pub name: String,
    /// Agent ids.
    pub agents: Vec<String>,
}

impl SubjectData {
    /// Read what `needs` asks for from the staged copy at `dir`.
    pub fn read(dir: &Path, name: &str, agents: &[String], needs: &[Placeholder]) -> Self {
        let mut data = Self {
            dir: Some(dir.to_string_lossy().into_owned()),
            name: name.to_string(),
            agents: agents.to_vec(),
            ..Self::default()
        };
        if needs.contains(&Placeholder::SkillMd) {
            data.skill_md = read_capped(&dir.join("SKILL.md"), MAX_SKILL_MD);
        }
        if needs.contains(&Placeholder::Files) {
            data.files = list_files(dir, MAX_FILES);
        }
        data
    }

    fn scalar(&self, p: Placeholder) -> String {
        match p {
            Placeholder::Dir => self.dir.clone().unwrap_or_default(),
            Placeholder::SkillMd => self.skill_md.clone().unwrap_or_default(),
            Placeholder::Name => self.name.clone(),
            Placeholder::Files => self.files.join("\n"),
            Placeholder::Agents => self.agents.join(", "),
        }
    }

    fn list(&self, p: Placeholder) -> Vec<String> {
        match p {
            Placeholder::Files => self.files.clone(),
            Placeholder::Agents => self.agents.clone(),
            other => vec![self.scalar(other)],
        }
    }
}

fn read_capped(path: &Path, cap: usize) -> Option<String> {
    use std::io::Read;
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(cap as u64)
        .read_to_end(&mut buf)
        .ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn list_files(root: &Path, cap: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let Ok(meta) = e.path().symlink_metadata() else {
                continue;
            };
            if meta.is_dir() && e.file_name() != ".git" {
                stack.push(e.path());
            } else if meta.is_file() {
                if let Ok(rel) = e.path().strip_prefix(root) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
            if out.len() >= cap {
                out.sort();
                return out;
            }
        }
    }
    out.sort();
    out
}

/// Expand one template value for a subject.
pub fn resolve(value: &Value, data: &SubjectData) -> Value {
    let fill = |s: &str| {
        let mut text = s.to_string();
        for p in Placeholder::ALL {
            if text.contains(p.token()) {
                text = text.replace(p.token(), &data.scalar(p));
            }
        }
        text
    };
    match value {
        Value::String(s) => match Placeholder::from_token(s) {
            Some(p) if p.is_list() => {
                Value::Array(data.list(p).into_iter().map(Value::String).collect())
            }
            _ => Value::String(fill(s)),
        },
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| match v {
                    Value::String(s) => Value::String(fill(s)),
                    other => other.clone(),
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Build the arguments for one call: expand every template and shape each value to the type
/// the tool's schema declares for it.
pub fn build(
    args: &BTreeMap<String, Value>,
    data: &SubjectData,
    schema: &Value,
) -> Map<String, Value> {
    args.iter()
        .map(|(k, v)| {
            let value = resolve(v, data);
            let prop = schema.get("properties").and_then(|p| p.get(k));
            (k.clone(), coerce(value, prop))
        })
        .collect()
}

fn types(schema: Option<&Value>) -> Vec<&str> {
    match schema.and_then(|s| s.get("type")) {
        Some(Value::String(t)) => vec![t.as_str()],
        Some(Value::Array(ts)) => ts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

fn coerce(value: Value, schema: Option<&Value>) -> Value {
    let ts = types(schema);
    if ts.is_empty() || ts.iter().any(|t| type_matches(&value, t)) {
        return value;
    }
    match (&value, ts[0]) {
        (Value::Array(items), "string") => Value::String(
            items
                .iter()
                .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        (Value::String(_) | Value::Number(_) | Value::Bool(_), "array") => {
            Value::Array(vec![value])
        }
        (Value::String(s), "integer") => s.trim().parse::<i64>().map_or(value.clone(), Value::from),
        (Value::String(s), "number") => s
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map_or(value.clone(), Value::Number),
        (Value::String(s), "boolean") => match s.trim() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => value,
        },
        _ => value,
    }
}

fn type_matches(value: &Value, t: &str) -> bool {
    match t {
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true,
    }
}

/// Problems with `args` under `schema` (required, types, enums, unknown properties). A
/// small, predictable subset of JSON Schema; the server still validates its own input.
pub fn check(args: &Map<String, Value>, schema: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    let props = schema.get("properties").and_then(Value::as_object);
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for name in required.iter().filter_map(Value::as_str) {
            if !args.contains_key(name) {
                problems.push(format!("{name} is required"));
            }
        }
    }
    let closed = schema.get("additionalProperties") == Some(&Value::Bool(false));
    for (k, v) in args {
        let prop = props.and_then(|p| p.get(k));
        if prop.is_none() {
            if closed {
                problems.push(format!("{k} is not an argument of this tool"));
            }
            continue;
        }
        let ts = types(prop);
        if !ts.is_empty() && !ts.iter().any(|t| type_matches(v, t)) {
            problems.push(format!("{k} should be {}", ts.join(" or ")));
        }
        if let Some(Value::Array(allowed)) = prop.and_then(|p| p.get("enum")) {
            if !allowed.contains(v) {
                problems.push(format!("{k} must be one of the listed values"));
            }
        }
        if let (Some(Value::Array(items)), Some(item_schema)) =
            (Some(v), prop.and_then(|p| p.get("items")))
        {
            let its = types(Some(item_schema));
            if !its.is_empty()
                && items
                    .iter()
                    .any(|i| !its.iter().any(|t| type_matches(i, t)))
            {
                problems.push(format!("{k} items should be {}", its.join(" or ")));
            }
        }
    }
    problems
}

/// A suggested mapping for one tool.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    /// Tool name.
    pub tool: String,
    /// Suggested arguments.
    pub args: BTreeMap<String, Value>,
    /// Required arguments holdmap couldn't fill.
    pub missing: Vec<String>,
}

/// Suggest a mapping from a tool's input schema: property names (and descriptions) that look
/// like a folder path, skill text, a file list, a name or agents. `remote` servers only get
/// metadata.
pub fn suggest(tool: &ToolInfo, remote: bool) -> Suggestion {
    let schema = &tool.input_schema;
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut args = BTreeMap::new();
    let mut taken: Vec<Placeholder> = Vec::new();
    if let Some(props) = schema.get("properties").and_then(Value::as_object) {
        for (name, prop) in props {
            let ts = types(Some(prop));
            let is_array = ts.contains(&"array");
            let is_text = ts.is_empty() || ts.contains(&"string");
            let guess = guess(
                name,
                prop.get("description").and_then(Value::as_str),
                is_array,
                is_text,
            );
            if let Some(p) = guess.filter(|p| !(remote && p.is_content()) && !taken.contains(p)) {
                taken.push(p);
                args.insert(name.clone(), Value::String(p.token().into()));
            }
        }
    }
    let missing = required
        .iter()
        .filter(|r| !args.contains_key(**r))
        .map(|r| r.to_string())
        .collect();
    Suggestion {
        tool: tool.name.clone(),
        args,
        missing,
    }
}

/// Where findings sit in a tool's JSON output (JSONPath-style; `format = "auto"`). SARIF and
/// holdmap JSON are recognised without it; anything else unmapped becomes a short detail.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputMapping {
    /// The finding list, e.g. `$.issues[*]`.
    #[serde(default)]
    pub items: Option<String>,
    /// Rule id field inside one item (required with `items`).
    #[serde(default)]
    pub rule_id: Option<String>,
    /// Severity field.
    #[serde(default)]
    pub severity: Option<String>,
    /// Title field.
    #[serde(default)]
    pub title: Option<String>,
    /// File field.
    #[serde(default)]
    pub file: Option<String>,
    /// Line field.
    #[serde(default)]
    pub line: Option<String>,
    /// Fix field.
    #[serde(default)]
    pub fix: Option<String>,
}

impl OutputMapping {
    /// Nothing mapped.
    pub fn is_empty(&self) -> bool {
        self.items.is_none()
    }
}

/// Suggest an output mapping from a tool's output schema: the first array of objects, and
/// item fields named like a rule id, severity, title, file and line. `None` when the schema
/// doesn't say enough (holdmap then auto-detects SARIF or holdmap JSON, or keeps an excerpt).
pub fn suggest_output(tool: &ToolInfo) -> Option<OutputMapping> {
    let props = tool
        .output_schema
        .as_ref()?
        .get("properties")?
        .as_object()?;
    for (list, prop) in props {
        if !types(Some(prop)).contains(&"array") || list == "runs" || list == "findings" {
            continue;
        }
        let Some(fields) = prop.pointer("/items/properties").and_then(Value::as_object) else {
            continue;
        };
        let pick = |words: &[&str]| {
            words
                .iter()
                .find(|w| fields.contains_key(**w))
                .map(|w| w.to_string())
        };
        let rule_id = pick(&["rule_id", "ruleId", "rule", "code", "check", "id", "type"])?;
        if !valid_key(list) {
            continue;
        }
        return Some(OutputMapping {
            items: Some(format!("$.{list}[*]")),
            rule_id: Some(rule_id),
            severity: pick(&["severity", "level", "risk", "priority"]),
            title: pick(&["title", "message", "msg", "summary", "description"]),
            file: pick(&["file", "path", "filename", "uri"]),
            line: pick(&["line", "start_line", "startLine", "line_number"]),
            fix: pick(&["fix", "remediation", "recommendation"]),
        });
    }
    None
}

fn valid_key(k: &str) -> bool {
    !k.is_empty()
        && k.len() <= 64
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn guess(
    name: &str,
    description: Option<&str>,
    is_array: bool,
    is_text: bool,
) -> Option<Placeholder> {
    let n = name.to_ascii_lowercase().replace(['-', ' '], "_");
    let d = description.unwrap_or("").to_ascii_lowercase();
    let any = |words: &[&str]| {
        words
            .iter()
            .any(|w| n == *w || n.ends_with(&format!("_{w}")))
    };
    if is_array {
        if any(&["files", "paths", "file_list", "filenames", "file_names"]) {
            return Some(Placeholder::Files);
        }
        if any(&["agents", "clients", "hosts"]) {
            return Some(Placeholder::Agents);
        }
        return None;
    }
    if !is_text {
        return None;
    }
    if any(&[
        "path",
        "dir",
        "directory",
        "folder",
        "root",
        "target",
        "skill_path",
        "skill_dir",
        "location",
    ]) || d.contains("path to") && (d.contains("folder") || d.contains("directory"))
    {
        return Some(Placeholder::Dir);
    }
    if any(&[
        "content", "text", "markdown", "skill_md", "source", "code", "body", "document",
    ]) {
        return Some(Placeholder::SkillMd);
    }
    if any(&["name", "skill", "skill_name", "title"]) {
        return Some(Placeholder::Name);
    }
    if any(&["agent", "client"]) {
        return Some(Placeholder::Agents);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(schema: Value) -> ToolInfo {
        ToolInfo {
            name: "scan".into(),
            title: None,
            description: None,
            input_schema: schema,
            output_schema: None,
            annotations: None,
        }
    }

    #[test]
    fn placeholders_are_closed_and_lists_must_be_whole_values() {
        assert_eq!(placeholders("{subject.dir}/x").unwrap(), [Placeholder::Dir]);
        assert!(placeholders("{env.HOME}").is_err());
        assert!(placeholders("a}").is_err());
        assert!(check_template(&json!("{subject.files}"), false).is_ok());
        assert!(check_template(&json!("x {subject.files}"), false).is_err());
        assert!(check_template(&json!(["{subject.agents}"]), false).is_err());
        assert!(check_template(&json!({"a": 1}), false).is_err());
        assert!(check_template(&json!(3), false).is_ok());
    }

    #[test]
    fn remote_servers_get_metadata_only() {
        assert!(check_template(&json!("{subject.dir}"), true).is_err());
        assert!(check_template(&json!("{subject.skill_md}"), true).is_err());
        assert!(check_template(&json!("{subject.name}"), true).is_ok());
        assert!(check_template(&json!("{subject.files}"), true).is_ok());
    }

    #[test]
    fn arguments_follow_the_schema_types() {
        let schema = json!({"type": "object", "properties": {
            "files": {"type": "array", "items": {"type": "string"}},
            "agents": {"type": "string"},
            "depth": {"type": "integer"},
            "path": {"type": "string"}
        }});
        let data = SubjectData {
            dir: Some("/tmp/s".into()),
            files: vec!["SKILL.md".into(), "run.sh".into()],
            agents: vec!["claude-code".into(), "cursor".into()],
            name: "deploy".into(),
            ..SubjectData::default()
        };
        let args: BTreeMap<String, Value> = [
            ("files", json!("{subject.files}")),
            ("agents", json!("{subject.agents}")),
            ("depth", json!("3")),
            ("path", json!("{subject.dir}/SKILL.md")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let built = build(&args, &data, &schema);
        assert_eq!(built["files"], json!(["SKILL.md", "run.sh"]));
        assert_eq!(built["agents"], json!("claude-code\ncursor"));
        assert_eq!(built["depth"], json!(3));
        assert_eq!(built["path"], json!("/tmp/s/SKILL.md"));
        assert!(check(&built, &schema).is_empty());
    }

    #[test]
    fn schema_checks_catch_missing_wrong_and_unknown_arguments() {
        let schema = json!({"type": "object", "additionalProperties": false,
            "required": ["path"],
            "properties": {"path": {"type": "string"}, "mode": {"enum": ["fast", "deep"]}}});
        let args: Map<String, Value> =
            serde_json::from_value(json!({"mode": "slow", "extra": 1})).unwrap();
        let problems = check(&args, &schema);
        assert!(
            problems.iter().any(|p| p.contains("path is required")),
            "{problems:?}"
        );
        assert!(problems.iter().any(|p| p.contains("mode")), "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("extra")), "{problems:?}");
    }

    #[test]
    fn suggestions_map_common_property_names_and_report_gaps() {
        let t = tool(json!({"type": "object", "required": ["path", "ruleset"],
        "properties": {
            "path": {"type": "string", "description": "Folder to scan"},
            "files": {"type": "array", "items": {"type": "string"}},
            "skill_name": {"type": "string"},
            "ruleset": {"type": "string"}
        }}));
        let s = suggest(&t, false);
        assert_eq!(s.args["path"], json!("{subject.dir}"));
        assert_eq!(s.args["files"], json!("{subject.files}"));
        assert_eq!(s.args["skill_name"], json!("{subject.name}"));
        assert_eq!(s.missing, ["ruleset"]);
        let remote = suggest(&t, true);
        assert!(!remote.args.contains_key("path"));
        assert!(remote.missing.contains(&"path".to_string()));
    }

    #[test]
    fn subject_data_reads_only_what_is_needed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "# hi").unwrap();
        std::fs::create_dir(dir.path().join("scripts")).unwrap();
        std::fs::write(dir.path().join("scripts/run.sh"), "echo").unwrap();
        let none = SubjectData::read(dir.path(), "s", &[], &[]);
        assert!(none.skill_md.is_none() && none.files.is_empty());
        let all = SubjectData::read(
            dir.path(),
            "s",
            &[],
            &[Placeholder::SkillMd, Placeholder::Files],
        );
        assert_eq!(all.skill_md.as_deref(), Some("# hi"));
        assert_eq!(all.files, ["SKILL.md", "scripts/run.sh"]);
    }

    #[test]
    fn output_mappings_are_suggested_from_output_schemas() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/lint_tool.json")).unwrap();
        let tool = ToolInfo::from_json(&fixture).unwrap();
        let s = suggest_output(&tool).unwrap();
        assert_eq!(s.items.as_deref(), Some("$.issues[*]"));
        assert_eq!(s.rule_id.as_deref(), Some("code"));
        assert_eq!(s.severity.as_deref(), Some("level"));
        assert_eq!(s.title.as_deref(), Some("msg"));
        assert_eq!(
            (s.file.as_deref(), s.line.as_deref(), s.fix),
            (Some("file"), Some("line"), None)
        );
        let no_rule = ToolInfo::from_json(&json!({
            "name": "x", "inputSchema": {"type": "object"},
            "outputSchema": {"type": "object", "properties": {"rows": {"type": "array", "items": {"type": "object", "properties": {"text": {"type": "string"}}}}}}
        }))
        .unwrap();
        assert_eq!(suggest_output(&no_rule), None);
        let sarif = ToolInfo::from_json(&json!({"name": "x", "inputSchema": {"type": "object"},
            "outputSchema": {"type": "object", "properties": {"runs": {"type": "array", "items": {"type": "object", "properties": {"id": {}}}}}}})).unwrap();
        assert_eq!(suggest_output(&sarif), None, "SARIF is detected on its own");
    }
}
