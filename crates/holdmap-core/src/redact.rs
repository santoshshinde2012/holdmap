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
            Some((k, v)) if sensitive(k) && !v.is_empty() && v != MASK => {
                changed = true;
                format!("{k}={MASK}")
            }
            _ => kv.to_string(),
        })
        .collect();
    changed.then(|| format!("{}?{}", &s[..q], parts.join("&")))
}

/// Redact one argument on its own (`--password=x`, `TOKEN=x`, URLs).
pub fn arg(a: &str) -> String {
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
            let inner = arg(v);
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

/// A flag that takes the secret as the next argument: `--password x`, `-p x` for mysql-likes.
fn takes_secret_value(a: &str) -> bool {
    a.starts_with('-') && !a.contains('=') && a.len() > 2 && sensitive(a)
}

/// Redact a command line given as arguments.
pub fn args(cmd: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(cmd.len());
    let mut hide_next = false;
    let mut bearer = false;
    for a in cmd {
        if hide_next && !a.starts_with('-') {
            out.push(MASK.to_string());
            hide_next = false;
            continue;
        }
        if bearer {
            out.push(MASK.to_string());
            bearer = false;
            continue;
        }
        hide_next = takes_secret_value(a);
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
        bearer = lower == "bearer" || lower == "authorization: bearer";
        out.push(crate::util::printable(&arg(a)).into_owned());
    }
    out
}

/// Redact a command line given as one string (split on whitespace, spacing kept).
pub fn line(s: &str) -> String {
    let words: Vec<String> = s.split(' ').map(str::to_string).collect();
    args(&words).join(" ")
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
        // A flag with no value isn't followed by a secret.
        assert_eq!(
            args(&v(&["app", "--token", "--json"])),
            v(&["app", "--token", "--json"])
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
