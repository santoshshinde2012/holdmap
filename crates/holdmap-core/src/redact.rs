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
    let mut out = Vec::with_capacity(cmd.len());
    let mut hide_next = false;
    let mut bearer = false;
    let mut pending_header = false;
    for a in cmd {
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
        hide_next = takes_secret_value(a);
        let redacted = arg(a);
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
    let redacted = args(&words);
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
