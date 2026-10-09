//! Which ports a shell command line is likely to bind, so a shell hook can explain an
//! `EADDRINUSE` right after a dev server fails to start (`holdmap init zsh`).
//!
//! Only commands that start servers are considered (`npm run dev`, `vite`, `rails s`,
//! `python manage.py runserver`, `docker run -p`…): a failing `curl localhost:3000` must not
//! produce a hint. Explicit ports win (`--port 4000`, `-p 4000`, `PORT=4000`, `:4000`); otherwise
//! the tool's well-known default port is used.

/// Ports `cmdline` will probably listen on, explicit ones first, without duplicates.
pub fn ports_in_command(cmdline: &str) -> Vec<u16> {
    let words = split(cmdline);
    let mut env_ports = Vec::new();
    let mut i = 0;
    // Leading `VAR=value` assignments and `env`/`sudo`-style wrappers.
    while i < words.len() {
        let w = &words[i];
        if let Some((k, v)) = w.split_once('=') {
            if is_env_name(k) {
                if is_port_var(k) {
                    env_ports.extend(port(v));
                }
                i += 1;
                continue;
            }
        }
        if matches!(
            w.as_str(),
            "env" | "sudo" | "exec" | "command" | "time" | "nohup"
        ) {
            i += 1;
            continue;
        }
        break;
    }
    let words = &words[i..];
    let Some(tool) = words.first().map(|w| basename(w)) else {
        return Vec::new(); // only assignments: nothing runs
    };
    let Some(default) = server_default(&tool, &words[1..]) else {
        return Vec::new();
    };
    let mut out = env_ports;
    out.extend(explicit_ports(&tool, &words[1..]));
    if out.is_empty() {
        out.extend(default);
    }
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|p| seen.insert(*p));
    out
}

fn is_env_name(k: &str) -> bool {
    !k.is_empty()
        && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !k.starts_with(|c: char| c.is_ascii_digit())
}

fn is_port_var(k: &str) -> bool {
    k == "PORT" || k.ends_with("_PORT")
}

fn basename(w: &str) -> String {
    let b = w.rsplit(['/', '\\']).next().unwrap_or(w);
    b.strip_suffix(".exe").unwrap_or(b).to_ascii_lowercase()
}

fn port(s: &str) -> Option<u16> {
    s.trim_matches(|c| c == '"' || c == '\'')
        .parse::<u16>()
        .ok()
        .filter(|p| *p >= 1)
}

/// `Some(default ports)` when the command starts a server (possibly with no known default),
/// `None` when it doesn't look like a server at all.
fn server_default(tool: &str, rest: &[String]) -> Option<Vec<u16>> {
    let sub = rest
        .iter()
        .map(String::as_str)
        .find(|w| !w.starts_with('-'));
    let has = |w: &str| rest.iter().any(|x| x == w);
    let d = |p: &[u16]| Some(p.to_vec());
    match tool {
        // Package-manager scripts: `npm run dev`, `pnpm dev`, `yarn start`, `bun run serve`.
        "npm" | "pnpm" | "yarn" | "bun" | "deno" => {
            let script = if matches!(sub, Some("run") | Some("run-script") | Some("task")) {
                rest.iter()
                    .map(String::as_str)
                    .filter(|w| !w.starts_with('-'))
                    .nth(1)
            } else {
                sub
            };
            match script {
                Some(s) if is_server_script(s) => d(&[]),
                _ => None,
            }
        }
        "npx" | "pnpx" | "bunx" => {
            let inner: Vec<String> = rest
                .iter()
                .skip_while(|w| w.starts_with('-'))
                .cloned()
                .collect();
            let (t, r) = inner.split_first()?;
            server_default(&basename(t), r)
        }
        "next" => matches!(sub, Some("dev") | Some("start")).then(|| vec![3000]),
        "vite" => match sub {
            Some("build") | Some("optimize") => None,
            Some("preview") => d(&[4173]),
            _ => d(&[5173]),
        },
        "astro" => matches!(sub, Some("dev") | Some("preview")).then(|| vec![4321]),
        "nuxt" | "nuxi" => matches!(sub, Some("dev") | Some("preview")).then(|| vec![3000]),
        "remix" | "react-router" => (sub == Some("dev")).then(|| vec![5173]),
        "ng" => matches!(sub, Some("serve") | Some("s")).then(|| vec![4200]),
        "svelte-kit" => (sub == Some("dev")).then(|| vec![5173]),
        "webpack-dev-server" => d(&[8080]),
        "webpack" => (sub == Some("serve")).then(|| vec![8080]),
        "storybook" => (sub == Some("dev")).then(|| vec![6006]),
        "serve" | "http-server" | "live-server" => d(&[3000, 8080]),
        "rails" => matches!(sub, Some("s") | Some("server")).then(|| vec![3000]),
        "bundle" => {
            (sub == Some("exec")).then_some(())?;
            let inner: Vec<String> = rest.iter().skip(1).cloned().collect();
            let (t, r) = inner.split_first()?;
            server_default(&basename(t), r)
        }
        "puma" | "rackup" => d(&[9292]),
        "uvicorn" | "hypercorn" | "daphne" | "fastapi" => d(&[8000]),
        "gunicorn" | "waitress-serve" => d(&[8000]),
        "flask" => (sub == Some("run")).then(|| vec![5000]),
        "python" | "python3" | "py" => {
            if has("-m") {
                let m = rest.iter().skip_while(|w| *w != "-m").nth(1)?;
                let after: Vec<String> = rest
                    .iter()
                    .skip_while(|w| *w != m)
                    .skip(1)
                    .cloned()
                    .collect();
                return match m.as_str() {
                    "http.server" => Some(
                        after
                            .iter()
                            .find_map(|w| port(w))
                            .into_iter()
                            .chain([8000])
                            .take(1)
                            .collect(),
                    ),
                    "flask" => server_default("flask", &after),
                    "uvicorn" | "gunicorn" | "hypercorn" | "fastapi" => server_default(m, &after),
                    "django" => {
                        (after.first().map(String::as_str) == Some("runserver")).then(|| vec![8000])
                    }
                    _ => None,
                };
            }
            let script = sub?;
            if basename(script) == "manage.py" && rest.iter().any(|w| w == "runserver") {
                return Some(runserver_port(rest).map(|p| vec![p]).unwrap_or(vec![8000]));
            }
            basename(script).ends_with(".py").then(Vec::new)
        }
        "django-admin" => rest
            .iter()
            .any(|w| w == "runserver")
            .then(|| runserver_port(rest).map(|p| vec![p]).unwrap_or(vec![8000])),
        "php" => {
            if has("-S") {
                return Some(Vec::new());
            }
            (sub == Some("artisan") && has("serve")).then(|| vec![8000])
        }
        "hugo" => (sub == Some("server") || sub == Some("serve")).then(|| vec![1313]),
        "jekyll" => matches!(sub, Some("serve") | Some("s")).then(|| vec![4000]),
        "mix" => (sub == Some("phx.server")).then(|| vec![4000]),
        "iex" => rest.iter().any(|w| w == "phx.server").then(|| vec![4000]),
        "node" | "tsx" | "ts-node" | "nodemon" | "bun-run" | "air" | "go" | "cargo" => match tool {
            "go" | "cargo" => (sub == Some("run")).then(Vec::new),
            _ => Some(Vec::new()),
        },
        "dotnet" => matches!(sub, Some("run") | Some("watch")).then(|| vec![5000]),
        "docker" | "podman" => (sub == Some("run")).then(Vec::new),
        "caddy" => matches!(
            sub,
            Some("run") | Some("file-server") | Some("reverse-proxy")
        )
        .then(|| vec![80]),
        "redis-server" => d(&[6379]),
        "postgres" => d(&[5432]),
        "mongod" => d(&[27017]),
        "turbo" | "nx" | "make" | "just" | "task" => match sub {
            Some(s) if is_server_script(s) || s == "run" => Some(Vec::new()),
            _ => None,
        },
        _ => None,
    }
}

fn is_server_script(s: &str) -> bool {
    let s = s.split(':').next().unwrap_or(s);
    matches!(
        s,
        "dev" | "start" | "serve" | "server" | "preview" | "develop" | "watch" | "up"
    )
}

fn runserver_port(rest: &[String]) -> Option<u16> {
    let after = rest.iter().skip_while(|w| *w != "runserver").nth(1)?;
    port(after.rsplit(':').next()?)
}

/// `--port 4000`, `--port=4000`, `-p 4000`, `-p4000`, `--listen 0.0.0.0:4000`, `-b :4000`,
/// `localhost:4000`, and `docker run -p 8080:80` (the host side).
fn explicit_ports(tool: &str, rest: &[String]) -> Vec<u16> {
    let docker = matches!(tool, "docker" | "podman");
    let mut out = Vec::new();
    let mut it = rest.iter().peekable();
    while let Some(w) = it.next() {
        let (flag, inline) = match w.split_once('=') {
            Some((f, v)) if f.starts_with('-') => (f, Some(v.to_string())),
            _ => (w.as_str(), None),
        };
        let takes_port = matches!(flag, "--port" | "-p" | "--http-port" | "--dev-port" | "-P");
        let takes_addr = matches!(
            flag,
            "--listen" | "--bind" | "-b" | "--host" | "--addr" | "-l"
        );
        if takes_port || takes_addr {
            let v = inline.or_else(|| it.next().cloned());
            if let Some(v) = v {
                out.extend(value_port(&v, docker));
            }
            continue;
        }
        // `-p4000` (but not `-p` + something that isn't a number).
        if let Some(v) = w
            .strip_prefix("-p")
            .filter(|v| !v.is_empty() && !w.starts_with("--"))
        {
            out.extend(value_port(v, docker));
            continue;
        }
        if w.starts_with('-') {
            continue;
        }
        // `localhost:4000`, `0.0.0.0:4000`, `:4000`, `http://127.0.0.1:4000/`.
        let bare = w
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split('/')
            .next()
            .unwrap_or("");
        if let Some((host, p)) = bare.rsplit_once(':') {
            if host.is_empty()
                || matches!(
                    host,
                    "localhost" | "0.0.0.0" | "127.0.0.1" | "[::]" | "[::1]" | "::"
                )
            {
                out.extend(port(p));
            }
        }
    }
    out
}

/// A port value: `4000`, `:4000`, `host:4000`, or for docker `8080:80` / `127.0.0.1:8080:80`.
fn value_port(v: &str, docker: bool) -> Option<u16> {
    let parts: Vec<&str> = v.split(':').collect();
    if docker && parts.len() >= 2 {
        let host_part = parts[parts.len() - 2];
        return port(host_part.split('-').next().unwrap_or(host_part));
    }
    port(parts.last()?)
}

/// Split a shell command line into words: honours single and double quotes and backslash
/// escapes; stops at the first `;`, `&&`, `||` or `|` (only the first command matters).
pub fn split(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut have = false;
    let mut chars = s.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                have = true;
            }
            (None, '\\') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                    have = true;
                }
            }
            (None, ';' | '|' | '&') => break,
            (None, c) if c.is_whitespace() => {
                if have || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    have = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if have || !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Vec<u16> {
        ports_in_command(s)
    }

    #[test]
    fn package_manager_scripts() {
        assert_eq!(p("npm run dev"), Vec::<u16>::new());
        assert_eq!(p("npm run dev -- --port 4000"), vec![4000]);
        assert_eq!(p("pnpm dev --port=4001"), vec![4001]);
        assert_eq!(p("PORT=3001 yarn start"), vec![3001]);
        assert_eq!(p("bun run serve -p 8081"), vec![8081]);
        assert_eq!(p("npm install"), Vec::<u16>::new());
        assert_eq!(p("npm test -- --port 4000"), Vec::<u16>::new());
        assert_eq!(p("pnpm run dev:web"), Vec::<u16>::new());
    }

    #[test]
    fn framework_defaults() {
        assert_eq!(p("npx next dev"), vec![3000]);
        assert_eq!(p("next dev -p 3005"), vec![3005]);
        assert_eq!(p("vite"), vec![5173]);
        assert_eq!(p("./node_modules/.bin/vite --port 5180"), vec![5180]);
        assert_eq!(p("vite preview"), vec![4173]);
        assert_eq!(p("vite build"), Vec::<u16>::new());
        assert_eq!(p("ng serve"), vec![4200]);
        assert_eq!(p("astro dev"), vec![4321]);
        assert_eq!(p("rails s"), vec![3000]);
        assert_eq!(p("bundle exec rails server -p 3002"), vec![3002]);
        assert_eq!(p("hugo server"), vec![1313]);
        assert_eq!(p("mix phx.server"), vec![4000]);
        assert_eq!(p("dotnet run"), vec![5000]);
    }

    #[test]
    fn python_servers() {
        assert_eq!(p("python manage.py runserver"), vec![8000]);
        assert_eq!(p("python3 manage.py runserver 0.0.0.0:8001"), vec![8001]);
        assert_eq!(p("uvicorn app:main --reload"), vec![8000]);
        assert_eq!(p("uvicorn app:main --port 9000"), vec![9000]);
        assert_eq!(p("gunicorn -b 0.0.0.0:8002 app:app"), vec![8002]);
        assert_eq!(p("flask run"), vec![5000]);
        assert_eq!(p("python -m flask run --port 5001"), vec![5001]);
        assert_eq!(p("python -m http.server 9001"), vec![9001]);
        assert_eq!(p("python -m http.server"), vec![8000]);
        assert_eq!(p("python -m pytest"), Vec::<u16>::new());
        assert_eq!(p("python3 server.py --port 7000"), vec![7000]);
    }

    #[test]
    fn containers_and_misc() {
        assert_eq!(p("docker run -p 8080:80 nginx"), vec![8080]);
        assert_eq!(p("docker run -p 127.0.0.1:5433:5432 postgres"), vec![5433]);
        assert_eq!(p("docker ps"), Vec::<u16>::new());
        assert_eq!(p("php -S localhost:8003"), vec![8003]);
        assert_eq!(p("node server.js"), Vec::<u16>::new());
        assert_eq!(p("node server.js --port 3333"), vec![3333]);
        assert_eq!(p("go run . -addr :8085"), vec![8085]);
        assert_eq!(p("redis-server"), vec![6379]);
        assert_eq!(p("env PORT=4444 node app.js"), vec![4444]);
    }

    #[test]
    fn clients_never_hint() {
        assert_eq!(p("curl localhost:3000"), Vec::<u16>::new());
        assert_eq!(p("ssh -p 2222 host"), Vec::<u16>::new());
        assert_eq!(p("psql -p 5432"), Vec::<u16>::new());
        assert_eq!(p("git push"), Vec::<u16>::new());
        assert_eq!(p(""), Vec::<u16>::new());
        assert_eq!(p("PORT=3000"), Vec::<u16>::new());
    }

    #[test]
    fn splitting() {
        assert_eq!(split(r#"a "b c" 'd e' f\ g"#), ["a", "b c", "d e", "f g"]);
        assert_eq!(split("npm run dev && echo done"), ["npm", "run", "dev"]);
        assert_eq!(split("a ''"), ["a", ""]);
        assert!(split("   ").is_empty());
    }

    #[test]
    fn never_panics_on_odd_input() {
        for s in [
            "-p",
            "--port",
            "-p=",
            "docker run -p :",
            "\"unterminated",
            "python -m",
            "npx",
            "bundle exec",
            ":::",
            "a=b=c vite",
        ] {
            let _ = p(s);
        }
    }
}
