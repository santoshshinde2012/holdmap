//! Hide secrets in command lines before they are shown or exported.
//!
//! Process command lines often carry credentials: `--password=hunter2`, `API_TOKEN=… node`,
//! `postgres://app:secret@db`, `?access_token=…`. holdmap shows command lines in the CLI, the
//! TUI, JSON, MCP results and the app, so every one of those goes through [`args()`] / [`line()`].
//! The raw command is kept only where it is needed to work (stopped-port history, so a restart
//! runs the real command), and that file is private to the user (0600).

/// What a hidden value is replaced with.
pub const MASK: &str = "••••";

/// Key names whose values are secrets (`--db-password`, `GITHUB_TOKEN`, `apiKey`, …).
fn sensitive(key: &str) -> bool {
    let k: String = key
        .trim_start_matches('-')
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    if k.is_empty() {
        return false;
    }
    const WORDS: [&str; 13] = [
        "password",
        "passwd",
        "passphrase",
        "secret",
        "token",
        "apikey",
        "accesskey",
        "privatekey",
        "credential",
        "credentials",
        "authorization",
        "cookie",
        "sessionid",
    ];
    k == "pass"
        || k == "auth"
        || k.ends_with("auth")
        || k == "pwd" && key.starts_with('-')
        || WORDS.iter().any(|w| k.contains(w))
        || k.ends_with("key")
            && (k.contains("api") || k.contains("secret") || k.contains("signing"))
        || k == "dsn"
}

/// `scheme://user:password@host` → `scheme://user:••••@host`.
fn url_credentials(s: &str) -> Option<String> {
    let start = s.find("://")? + 3;
    let rest = &s[start..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let at = authority.rfind('@')?;
    let userinfo = &authority[..at];
    let colon = userinfo.find(':')?;
    if colon + 1 == userinfo.len() {
        return None;
    }
    Some(format!(
        "{}{}:{MASK}{}",
        &s[..start],
        &userinfo[..colon],
        &s[start + at..]
    ))
}

/// `?token=abc&page=2` → `?token=••••&page=2`.
fn query_secrets(s: &str) -> Option<String> {
    let q = s.find('?')?;
    let mut changed = false;
    let parts: Vec<String> = s[q + 1..]
        .split('&')
        .map(|kv| match kv.split_once('=') {
            Some((k, v)) if sensitive(&decode_key(k)) && !v.is_empty() && v != MASK => {
                changed = true;
                format!("{k}={MASK}")
            }
            _ => kv.to_string(),
        })
        .collect();
    changed.then(|| format!("{}?{}", &s[..q], parts.join("&")))
}

fn decode_key(key: &str) -> String {
    let bytes = key.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                decoded.push((a * 16 + b) as u8);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn secret_header(value: &str) -> Option<String> {
    let (key, data) = value.split_once(':')?;
    if !sensitive(key) || key.contains(['/', '=', ' ']) || data.trim().is_empty() {
        return None;
    }
    let start = key.len() + 1 + data.len() - data.trim_start().len();
    let lower = value[start..].to_ascii_lowercase();
    let scheme = ["bearer ", "basic "]
        .iter()
        .find(|scheme| lower.starts_with(**scheme));
    let end = start + scheme.map_or(0, |scheme| scheme.len());
    Some(format!("{}{MASK}", &value[..end]))
}

/// Redact one argument on its own (`--password=x`, `TOKEN=x`, URLs).
pub fn arg(a: &str) -> String {
    redact_arg(a, 0)
}

fn redact_arg(a: &str, depth: usize) -> String {
    if depth >= 32 {
        return MASK.into();
    }
    if let Some(json) = json_secrets(a, depth + 1) {
        return json;
    }
    if let Some(header) = secret_header(a) {
        return header;
    }
    if let Some((k, v)) = a.split_once('=') {
        let key_like = !k.is_empty()
            && !k.contains(['/', ':', '?', ' '])
            && k.trim_start_matches('-')
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.');
        if key_like && sensitive(k) && !v.is_empty() {
            return format!("{k}={MASK}");
        }
        if key_like && !v.is_empty() {
            let inner = redact_arg(v, depth + 1);
            if inner != v {
                return format!("{k}={inner}");
            }
        }
    }
    let mut out = a.to_string();
    if let Some(r) = url_credentials(&out) {
        out = r;
    }
    if let Some(r) = query_secrets(&out) {
        out = r;
    }
    out
}

fn json_secrets(input: &str, depth: usize) -> Option<String> {
    let trimmed = input.trim();
    let (text, quote) = if trimmed.len() >= 2
        && ((trimmed.starts_with('\'') && trimmed.ends_with('\''))
            || (trimmed.starts_with('"') && trimmed.ends_with('"')))
    {
        (&trimmed[1..trimmed.len() - 1], &trimmed[..1])
    } else {
        (trimmed, "")
    };
    if !text.starts_with(['{', '[']) {
        return None;
    }
    // Display strings are untrusted. Bound parsing/allocation and fail closed for oversized
    // JSON-shaped values rather than attempting to inspect a potentially huge config.
    if text.len() > 64 * 1024 {
        return Some(MASK.into());
    }
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Some(MASK.into());
    };
    fn visit(value: &mut serde_json::Value, depth: usize) -> bool {
        if depth >= 32 {
            *value = serde_json::Value::String(MASK.into());
            return true;
        }
        match value {
            serde_json::Value::Object(entries) => {
                let mut changed = false;
                for (key, value) in entries {
                    if sensitive(key) {
                        *value = serde_json::Value::String(MASK.into());
                        changed = true;
                    } else {
                        changed |= visit(value, depth + 1);
                    }
                }
                changed
            }
            serde_json::Value::Array(values) => {
                let mut changed = false;
                for value in values {
                    changed |= visit(value, depth + 1);
                }
                changed
            }
            serde_json::Value::String(text) => {
                let redacted = redact_arg(text, depth + 1);
                if *text != redacted {
                    *text = redacted;
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
    visit(&mut value, depth).then(|| format!("{quote}{value}{quote}"))
}

/// A flag that takes the secret as the next argument: `--password x`, `-p x` for mysql-likes.
fn takes_secret_value(a: &str) -> bool {
    a.starts_with('-') && !a.contains('=') && a.len() > 2 && sensitive(a)
}

/// Redact a command line given as arguments.
pub fn args(cmd: &[String]) -> Vec<String> {
    args_inner(cmd, 0, false)
}

const MAX_SCRIPT_BYTES: usize = 64 * 1024;
const MAX_SCRIPT_DEPTH: usize = 8;

#[derive(Clone, Copy)]
struct ScriptPayload {
    at: usize,
    rest: bool,
    prefix: usize,
    encoded: bool,
}

/// Recognize interpreter invocation only. Positional text for ordinary programs stays argv.
fn script_payload(cmd: &[String]) -> Option<ScriptPayload> {
    let program = cmd
        .first()?
        .rsplit(['/', '\\'])
        .next()?
        .to_ascii_lowercase();
    let program = program.strip_suffix(".exe").unwrap_or(&program);
    let posix = matches!(
        program,
        "sh" | "bash" | "dash" | "zsh" | "ksh" | "mksh" | "ash" | "fish" | "csh" | "tcsh"
    );
    let powershell = matches!(program, "powershell" | "pwsh");
    if !posix && !powershell && program != "cmd" {
        return None;
    }
    let mut index = 1;
    while let Some(argument) = cmd.get(index) {
        let lower = argument.to_ascii_lowercase();
        if posix {
            if argument == "--" || !argument.starts_with('-') {
                return None;
            }
            if matches!(argument.as_str(), "--rcfile" | "--init-file") {
                index += 2;
                continue;
            }
            let short = argument.strip_prefix('-').unwrap_or("");
            if argument == "--command"
                || !short.starts_with('-')
                    && short.contains('c')
                    && short.chars().all(|c| c.is_ascii_alphabetic())
            {
                return Some(ScriptPayload {
                    at: index + 1,
                    rest: false,
                    prefix: 0,
                    encoded: false,
                });
            }
        } else if program == "cmd" {
            if lower.starts_with("/c") || lower.starts_with("/k") {
                return Some(ScriptPayload {
                    at: if argument.len() == 2 {
                        index + 1
                    } else {
                        index
                    },
                    rest: true,
                    prefix: if argument.len() == 2 { 0 } else { 2 },
                    encoded: false,
                });
            }
        } else {
            let flag = lower.trim_start_matches(['-', '/']);
            if lower == flag || flag.is_empty() {
                return None;
            }
            if "file".starts_with(flag) {
                return None;
            }
            // These abbreviations are accepted by PowerShell's own host parser. Opaque
            // UTF-16/base64 payloads can contain credentials, so never export their bytes.
            if "encodedcommand".starts_with(flag)
                || matches!(flag, "ec" | "ea")
                || "encodedarguments".starts_with(flag)
            {
                return Some(ScriptPayload {
                    at: index + 1,
                    rest: false,
                    prefix: 0,
                    encoded: true,
                });
            }
            if "command".starts_with(flag) || matches!(flag, "commandwithargs" | "cwa") {
                return Some(ScriptPayload {
                    at: index + 1,
                    rest: flag != "commandwithargs" && flag != "cwa",
                    prefix: 0,
                    encoded: false,
                });
            }
            if matches!(
                flag,
                "executionpolicy"
                    | "ep"
                    | "ex"
                    | "windowstyle"
                    | "w"
                    | "inputformat"
                    | "outputformat"
                    | "workingdirectory"
                    | "wd"
            ) {
                index += 2;
                continue;
            }
        }
        index += 1;
    }
    None
}

fn quoted_word(value: &str) -> Option<(&str, &str)> {
    let quote = value.get(..1)?;
    if !matches!(quote, "'" | "\"") || value.len() < 2 || !value.ends_with(quote) {
        return None;
    }
    let inner = &value[1..value.len() - 1];
    // Nested/escaped quote grammar is interpreter-specific. Only unwrap one plain pair.
    (!inner.contains(quote)).then_some((quote, inner))
}

fn secret_syntax(value: &str) -> bool {
    value.split_whitespace().any(|word| {
        let word = word.trim_matches(['\'', '"']);
        takes_secret_value(word) || word.split_once('=').is_some_and(|(key, _)| sensitive(key))
    })
}

fn script_word(value: &str, depth: usize) -> String {
    if depth >= MAX_SCRIPT_DEPTH || value.len() > MAX_SCRIPT_BYTES {
        return MASK.into();
    }
    if let Some((quote, inner)) = quoted_word(value) {
        let redacted = arg(inner);
        if redacted != inner {
            return format!("{quote}{redacted}{quote}");
        }
        return format!("{quote}{}{quote}", line_inner(inner, depth + 1, true));
    }
    if value.starts_with(['\'', '"']) && secret_syntax(value) {
        return MASK.into();
    }
    if let Some((key, data)) = value.split_once('=') {
        // PowerShell variables are assignment targets too; only inspect them inside a
        // recognized script, leaving ordinary positional strings untouched.
        if key.starts_with('$') && sensitive(key) && !data.is_empty() {
            return format!("{key}={MASK}");
        }
    }
    arg(value)
}

fn args_inner(cmd: &[String], depth: usize, script: bool) -> Vec<String> {
    let payload = script_payload(cmd);
    let mut out = Vec::with_capacity(cmd.len());
    let mut hide_next = false;
    let mut bearer = false;
    let mut pending_header = false;
    let mut assignment_equals = false;
    for (index, a) in cmd.iter().enumerate() {
        if assignment_equals {
            assignment_equals = false;
            out.push(a.clone());
            hide_next = true;
            continue;
        }
        if hide_next {
            out.push(MASK.to_string());
            hide_next = false;
            continue;
        }
        if pending_header {
            pending_header = false;
            if a.eq_ignore_ascii_case("basic") || a.eq_ignore_ascii_case("bearer") {
                out.push(a.clone());
                hide_next = true;
            } else {
                out.push(MASK.to_string());
            }
            continue;
        }
        if bearer {
            out.push(MASK.to_string());
            bearer = false;
            continue;
        }
        let active_payload = payload.filter(|p| index == p.at || p.rest && index > p.at);
        let script_argument = script || active_payload.is_some_and(|p| !p.encoded);
        let semantic = if script_argument {
            quoted_word(a).map_or(a.as_str(), |(_, inner)| inner)
        } else {
            a.as_str()
        };
        hide_next = takes_secret_value(semantic);
        assignment_equals = (script || active_payload.is_some_and(|p| p.rest && !p.encoded))
            && semantic.starts_with('$')
            && sensitive(semantic)
            && cmd.get(index + 1).is_some_and(|next| next == "=");
        let redacted = if let Some(payload) = active_payload {
            if payload.encoded || depth >= MAX_SCRIPT_DEPTH || a.len() > MAX_SCRIPT_BYTES {
                MASK.into()
            } else {
                let prefix = if index == payload.at {
                    payload.prefix
                } else {
                    0
                };
                format!(
                    "{}{}",
                    &a[..prefix],
                    line_inner(&a[prefix..], depth + 1, true)
                )
            }
        } else if script {
            script_word(a, depth)
        } else {
            arg(a)
        };
        if redacted != *a {
            out.push(crate::util::printable(&redacted).into_owned());
            continue;
        }
        let lower = a.to_ascii_lowercase();
        // `-H "Authorization: Bearer abc"` (one argument) or `Bearer abc` split in two.
        let header = lower.contains("authorization")
            || lower.starts_with("bearer ")
            || lower.starts_with("basic ");
        if let Some(i) = header
            .then(|| lower.find("bearer ").or_else(|| lower.find("basic ")))
            .flatten()
        {
            let cut = i + lower[i..].find(' ').unwrap_or(0) + 1;
            out.push(format!("{}{MASK}", &a[..cut]));
            continue;
        }
        bearer = lower == "bearer"
            || lower == "basic"
            || lower == "authorization: bearer"
            || lower == "authorization: basic";
        pending_header = a.strip_suffix(':').is_some_and(sensitive);
        out.push(crate::util::printable(&redacted).into_owned());
    }
    out.into_iter()
        .map(|value| crate::util::printable(&value).into_owned())
        .collect()
}

/// Redact a command string, preserving spacing and keeping quoted multiword values together.
pub fn line(s: &str) -> String {
    line_inner(s, 0, false)
}

fn line_inner(s: &str, depth: usize, script: bool) -> String {
    if script && (depth >= MAX_SCRIPT_DEPTH || s.len() > MAX_SCRIPT_BYTES) {
        return MASK.into();
    }
    let mut spans = Vec::new();
    let mut start = None;
    let mut quote = None;
    let mut escaped = false;
    for (i, character) in s.char_indices() {
        if start.is_none() && !character.is_whitespace() {
            start = Some(i);
        }
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote != Some('\'') {
            escaped = true;
        } else if Some(character) == quote {
            quote = None;
        } else if quote.is_none() && matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if quote.is_none() && character.is_whitespace() {
            if let Some(start) = start.take() {
                spans.push((start, i));
            }
        }
    }
    if let Some(start) = start {
        spans.push((start, s.len()));
    }
    let words: Vec<String> = spans
        .iter()
        .map(|(start, end)| s[*start..*end].to_string())
        .collect();
    if script
        && words.iter().any(|word| {
            word.contains(char::is_whitespace) && secret_syntax(word) && quoted_word(word).is_none()
        })
    {
        // Mixed/unclosed interpreter quotes can merge a credential flag with its value.
        // Preserve privacy rather than guessing a cmd/PowerShell-specific grammar.
        return MASK.into();
    }
    let redacted = args_inner(&words, depth, script);
    let mut out = String::new();
    let mut cursor = 0;
    for ((start, end), word) in spans.iter().zip(redacted) {
        out.push_str(&s[cursor..*start]);
        out.push_str(&word);
        cursor = *end;
    }
    out.push_str(&s[cursor..]);
    crate::util::printable(&out).into_owned()
}

/// Serialize an optional shell command for display while keeping the in-memory command raw.
pub fn serialize_optional_line<S: serde::Serializer>(
    value: &Option<String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(&value.as_deref().map(line), serializer)
}

/// Serialize environment values without exporting credential keys or credential-bearing URLs.
pub fn serialize_env<S: serde::Serializer>(
    values: &std::collections::BTreeMap<String, String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let redacted: std::collections::BTreeMap<_, _> = values
        .iter()
        .map(|(key, value)| {
            (
                key,
                if sensitive(key) {
                    MASK.to_string()
                } else {
                    arg(value)
                },
            )
        })
        .collect();
    serde::Serialize::serialize(&redacted, serializer)
}

/// Serde helper: serialize a command line redacted.
pub fn serialize_args<S: serde::Serializer>(v: &[String], s: S) -> Result<S::Ok, S::Error> {
    serde::Serialize::serialize(&args(v), s)
}

/// Serde helper: serialize a command string redacted.
pub fn serialize_line<S: serde::Serializer>(v: &str, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&line(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_shell_script_arguments_hide_credentials_without_mutating_argv() {
        for (program, flag) in [
            ("sh", "-c"),
            ("/bin/bash", "-lc"),
            ("zsh", "-c"),
            ("dash", "-c"),
            (r"C:\Windows\System32\cmd.exe", "/C"),
            ("cmd", "/K"),
            ("powershell.exe", "-Command"),
            ("pwsh", "-c"),
        ] {
            let original = vec![
                program.into(),
                flag.into(),
                "echo --token fixture-shell-secret --password 'two words'".into(),
            ];
            let displayed = args(&original);
            assert_eq!(displayed.len(), original.len());
            assert!(original[2].contains("fixture-shell-secret"));
            assert!(
                !displayed[2].contains("fixture-shell-secret"),
                "{program}: {displayed:?}"
            );
            assert!(
                !displayed[2].contains("two words"),
                "{program}: {displayed:?}"
            );
            assert!(displayed[2].contains("--token"));
            assert!(displayed[2].contains(MASK));
        }
        assert!(!line("cmd /Cecho --password inline-secret").contains("inline-secret"));
        assert!(!line("sh -c 'echo --token quoted-secret'").contains("quoted-secret"));
        assert!(!args(&[
            "pwsh".into(),
            "-Command".into(),
            "$env:TOKEN".into(),
            "=".into(),
            "split-secret".into()
        ])
        .join(" ")
        .contains("split-secret"));
    }

    #[test]
    fn security_nested_scripts_and_quoted_assignments_fail_closed() {
        for script in [
            "sh -c 'echo --password \"nested secret\"'",
            "set \"TOKEN=assignment secret\" && node worker.js",
            "$env:TOKEN='assignment secret'; Invoke-Worker",
            "$env:TOKEN = 'spaced assignment secret'; Invoke-Worker",
            "\"\"quoted worker.exe\" --token \"complex secret\"\"",
            "'echo --token unclosed-secret",
        ] {
            let command = vec!["sh".into(), "-c".into(), script.into()];
            let displayed = args(&command);
            assert!(!displayed[2].contains("secret"), "{script}: {displayed:?}");
            assert!(displayed[2].contains(MASK), "{script}: {displayed:?}");
        }
    }

    #[test]
    fn security_powershell_encoded_payloads_are_private_on_all_exports() {
        // UTF-16LE/base64 of `echo --password encoded-secret`, reversible credential data.
        let encoded =
            "ZQBjAGgAbwAgAC0ALQBwAGEAcwBzAHcAbwByAGQAIABlAG4AYwBvAGQAZQBkAC0AcwBlAGMAcgBlAHQA";
        for flag in [
            "-EncodedCommand",
            "-e",
            "-ec",
            "-enc",
            "-ENCO",
            "-EncodedArguments",
        ] {
            let original = vec![
                "pwsh.exe".into(),
                "-NoProfile".into(),
                flag.into(),
                encoded.into(),
            ];
            let displayed = args(&original);
            assert_eq!(displayed.len(), original.len());
            assert_eq!(displayed[3], MASK);
            assert_eq!(original[3], encoded);
            assert!(!line(&original.join(" ")).contains(encoded));
        }
    }

    #[test]
    fn security_shell_redaction_bounds_scripts_and_preserves_unrelated_positional_text() {
        let body = "x".repeat(MAX_SCRIPT_BYTES + 1);
        assert_eq!(args(&["sh".into(), "-c".into(), body])[2], MASK);
        assert_eq!(
            args_inner(
                &["sh".into(), "-c".into(), "echo --token secret".into()],
                MAX_SCRIPT_DEPTH,
                false
            )[2],
            MASK
        );
        for original in [
            vec!["echo".into(), "sh -c 'literal --token explanation'".into()],
            vec![
                "python".into(),
                "-c".into(),
                "print('literal --token explanation')".into(),
            ],
            vec![
                "sh".into(),
                "script.sh".into(),
                "-c".into(),
                "echo --token explanation".into(),
            ],
            vec![
                "pwsh".into(),
                "-File".into(),
                "script.ps1".into(),
                "-Command".into(),
                "literal --token explanation".into(),
            ],
            vec![
                "node".into(),
                "-EncodedCommand".into(),
                "opaque positional text".into(),
            ],
        ] {
            assert_eq!(args(&original), original);
        }
    }

    #[test]
    fn security_process_serialization_hides_shell_body_while_keeping_native_arguments() {
        let original = vec![
            "sh".into(),
            "-c".into(),
            "node worker --password process-secret".into(),
        ];
        let process = crate::model::ProcessInfo {
            pid: 1,
            ppid: None,
            name: "sh".into(),
            exe: None,
            cmdline: original.clone(),
            cwd: None,
            uid: None,
            user: None,
            start_time: 1,
            start_token: 1,
            memory_bytes: 0,
            cpu_percent: 0.0,
        };
        let exported = serde_json::to_string(&process).unwrap();
        assert!(!exported.contains("process-secret"));
        assert!(exported.contains(MASK));
        assert_eq!(process.cmdline, original);
        assert!(!process.command().contains("process-secret"));
    }

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn hides_flag_values() {
        assert_eq!(
            args(&v(&["server", "--password=hunter2", "--port=3000"])),
            v(&["server", "--password=••••", "--port=3000"])
        );
        assert_eq!(
            args(&v(&["mysqld", "--db-password", "hunter2", "--verbose"])),
            v(&["mysqld", "--db-password", "••••", "--verbose"])
        );
        assert_eq!(
            args(&v(&["app", "--api-key", "k"])),
            v(&["app", "--api-key", "••••"])
        );
        assert_eq!(
            args(&v(&["app", "--client-secret=s"])),
            v(&["app", "--client-secret=••••"])
        );
        // Without a command-specific parser, a dash-prefixed value may itself be a secret.
        assert_eq!(
            args(&v(&["app", "--token", "--json"])),
            v(&["app", "--token", "••••"])
        );
    }

    #[test]
    fn hides_env_assignments_and_urls() {
        assert_eq!(arg("GITHUB_TOKEN=ghp_abc"), "GITHUB_TOKEN=••••");
        assert_eq!(arg("AWS_SECRET_ACCESS_KEY=x"), "AWS_SECRET_ACCESS_KEY=••••");
        assert_eq!(arg("PORT=3000"), "PORT=3000");
        assert_eq!(
            arg("postgres://app:s3cret@localhost:5432/db"),
            "postgres://app:••••@localhost:5432/db"
        );
        assert_eq!(
            arg("--url=redis://:pw@127.0.0.1:6379"),
            "--url=redis://:••••@127.0.0.1:6379"
        );
        assert_eq!(
            arg("DATABASE_URL=postgres://u:p@h/db"),
            "DATABASE_URL=postgres://u:••••@h/db"
        );
        assert_eq!(
            arg("http://localhost:3000/a?token=abc&x=1"),
            "http://localhost:3000/a?token=••••&x=1"
        );
        assert_eq!(arg("http://user@host/x"), "http://user@host/x");
    }

    #[test]
    fn hides_auth_headers() {
        assert_eq!(
            args(&v(&["curl", "-H", "Authorization: Bearer abc.def"])),
            v(&["curl", "-H", "Authorization: Bearer ••••"])
        );
        assert_eq!(
            line("curl -H Authorization: Bearer abc"),
            "curl -H Authorization: Bearer ••••"
        );
    }

    #[test]
    fn security_headers_and_whitespace_credentials_never_export_values() {
        for command in [
            v(&[
                "curl",
                "-H",
                "Cookie: session=fixture-secret; other=another-secret",
            ]),
            v(&["curl", "-H", "X-Api-Key: fixture-secret"]),
            v(&["curl", "-H", "Authorization:", "Basic", "fixture-secret"]),
            v(&["server", "--password", "-fixture-secret"]),
            v(&["server", "--password", "secret phrase"]),
        ] {
            let output = args(&command).join(" ");
            assert!(!output.contains("fixture-secret"), "{output}");
            assert!(!output.contains("another-secret"), "{output}");
            assert!(!output.contains("secret phrase"), "{output}");
        }
        for command in [
            "server\t--password\tfixture-secret",
            "server --password 'secret phrase' --port 3000",
            "server --password=\"secret phrase\" --port 3000",
        ] {
            let output = line(command);
            assert!(!output.contains("fixture-secret"), "{output}");
            assert!(
                !output.contains("secret") && !output.contains("phrase"),
                "{output}"
            );
            assert!(output.contains(MASK), "{output}");
        }
    }

    #[test]
    fn security_inline_mcp_json_redacts_nested_values_and_encoded_query_keys() {
        let json = r#"{"mcpServers":{"fixture":{"env":{"API_KEY":"security-fixture-secret","MODE":"dev"},"headers":{"Authorization":"Bearer security-fixture"},"args":["--url=https://u:fixture-password@host"]}}}"#;
        for input in [
            json.to_string(),
            format!("--mcp-config={json}"),
            format!("agent --mcp-config '{json}'"),
        ] {
            let result = line(&input);
            for secret in [
                "security-fixture-secret",
                "Bearer security-fixture",
                "fixture-password",
            ] {
                assert!(!result.contains(secret), "{result}");
            }
            assert!(result.contains("dev"));
        }
        assert!(
            !arg("https://localhost/?%61pi_key=fixture-secret&port=3000")
                .contains("fixture-secret")
        );
        assert_eq!(
            arg(r#"{"port":3000,"name":"fixture"}"#),
            r#"{"port":3000,"name":"fixture"}"#
        );
    }

    #[test]
    fn security_redaction_bounds_nested_wrappers_and_json_string_reentry() {
        let wrapped = format!("{}fixture-secret", "a=".repeat(20_000));
        let output = arg(&wrapped);
        assert!(output.len() < 256 && output.contains(MASK));
        assert!(!output.contains("fixture-secret"));
        let json = serde_json::json!({"nested": wrapped}).to_string();
        let output = arg(&json);
        assert!(output.len() < 512 && output.contains(MASK));
        assert!(!output.contains("fixture-secret"));
        let deep = format!("{}0{}", "[".repeat(100), "]".repeat(100));
        assert!(arg(&deep).contains(MASK));
    }

    #[test]
    fn leaves_ordinary_commands_alone() {
        for c in [
            "npm run dev",
            "node /Users/me/app/node_modules/.bin/next dev -p 3000",
            "python -m http.server 8000 --bind 127.0.0.1",
            "postgres -D /usr/local/var/postgres",
            "vite --host 0.0.0.0 --port 5173",
            "ssh -L 8080:localhost:80 devbox",
            "cargo run -- --key-file ./k.pem",
        ] {
            assert_eq!(line(c), c, "{c}");
        }
    }
}
