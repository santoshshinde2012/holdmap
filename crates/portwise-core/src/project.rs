//! Project & framework detection: cwd → project root (manifest) → git branch, plus command-line
//! signatures ("next dev", "uvicorn", "postgres") refined by the project's dependencies.

use crate::model::{Framework, FrameworkCategory as C, ProcessInfo, ProjectInfo};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const MARKERS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "go.mod",
    "Gemfile",
    "composer.json",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "mix.exs",
    "deno.json",
    "manage.py",
    "requirements.txt",
];

/// A detected project plus the dependency names used to refine framework detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDetails {
    pub info: ProjectInfo,
    pub deps: Vec<String>,
}

/// Caches project lookups per directory for one scan.
#[derive(Debug, Default)]
pub struct ProjectDetector {
    cache: HashMap<PathBuf, Option<ProjectDetails>>,
    home: Option<PathBuf>,
}

impl ProjectDetector {
    pub fn new() -> Self {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from);
        Self {
            cache: HashMap::new(),
            home,
        }
    }

    pub fn with_home(home: Option<PathBuf>) -> Self {
        Self {
            cache: HashMap::new(),
            home,
        }
    }

    pub fn detect(&mut self, cwd: &Path) -> Option<ProjectDetails> {
        if let Some(hit) = self.cache.get(cwd) {
            return hit.clone();
        }
        let found = detect_project(cwd, self.home.as_deref());
        self.cache.insert(cwd.to_path_buf(), found.clone());
        found
    }
}

/// Walk up from `cwd` (max 8 levels) looking for a project manifest. The home directory and the
/// filesystem root are never treated as projects.
pub fn detect_project(cwd: &Path, home: Option<&Path>) -> Option<ProjectDetails> {
    let mut dir = Some(cwd);
    let mut git_only: Option<PathBuf> = None;
    for _ in 0..8 {
        let d = dir?;
        if d.parent().is_none() || Some(d) == home {
            break;
        }
        for m in MARKERS {
            let path = d.join(m);
            if path.is_file() {
                return Some(build(d, m, &path));
            }
        }
        if git_only.is_none() && d.join(".git").exists() {
            git_only = Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    git_only.map(|root| ProjectDetails {
        info: ProjectInfo {
            name: dir_name(&root),
            git_branch: git_branch(&root),
            root,
            kind: "git".into(),
        },
        deps: Vec::new(),
    })
}

fn dir_name(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

fn build(root: &Path, marker: &str, path: &Path) -> ProjectDetails {
    let text = fs::read_to_string(path).unwrap_or_default();
    let (name, mut deps) = match marker {
        "package.json" => parse_package_json(&text),
        "Cargo.toml" => (toml_name(&text, "[package]"), toml_deps(&text)),
        "pyproject.toml" => (
            toml_name(&text, "[project]").or_else(|| toml_name(&text, "[tool.poetry]")),
            text_deps(&text),
        ),
        "go.mod" => (
            text.lines()
                .find_map(|l| l.strip_prefix("module "))
                .map(|m| m.trim().rsplit('/').next().unwrap_or(m).to_string()),
            text_deps(&text),
        ),
        "composer.json" => {
            let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            (
                v["name"]
                    .as_str()
                    .map(|n| n.rsplit('/').next().unwrap_or(n).to_string()),
                json_keys(&v, &["require", "require-dev"]),
            )
        }
        _ => (None, text_deps(&text)),
    };
    // Python projects often have requirements.txt next to manage.py / pyproject.
    if matches!(marker, "manage.py" | "pyproject.toml") {
        if let Ok(req) = fs::read_to_string(root.join("requirements.txt")) {
            deps.extend(text_deps(&req));
        }
        if marker == "manage.py" {
            deps.push("django".into());
        }
    }
    ProjectDetails {
        info: ProjectInfo {
            name: name
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| dir_name(root)),
            root: root.to_path_buf(),
            kind: marker.to_string(),
            git_branch: git_branch(root),
        },
        deps,
    }
}

fn json_keys(v: &serde_json::Value, sections: &[&str]) -> Vec<String> {
    sections
        .iter()
        .filter_map(|s| v[*s].as_object())
        .flat_map(|o| o.keys().cloned())
        .collect()
}

fn parse_package_json(text: &str) -> (Option<String>, Vec<String>) {
    let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
    let name = v["name"]
        .as_str()
        .map(|n| n.rsplit('/').next().unwrap_or(n).to_string());
    let mut deps = json_keys(&v, &["dependencies", "devDependencies"]);
    // Scripts reveal the dev command (e.g. "dev": "vite").
    if let Some(scripts) = v["scripts"].as_object() {
        for s in scripts.values().filter_map(|s| s.as_str()) {
            deps.extend(s.split_whitespace().map(|w| format!("script:{w}")));
        }
    }
    (name, deps)
}

fn toml_name(text: &str, section: &str) -> Option<String> {
    let mut in_section = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_section = l == section;
            continue;
        }
        if in_section {
            if let Some(rest) = l.strip_prefix("name") {
                let v = rest.trim_start().strip_prefix('=')?.trim();
                return Some(v.trim_matches(|c| c == '"' || c == '\'').to_string());
            }
        }
    }
    None
}

fn toml_deps(text: &str) -> Vec<String> {
    let mut in_deps = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_deps = l.contains("dependencies");
            continue;
        }
        if in_deps {
            if let Some((k, _)) = l.split_once('=') {
                out.push(k.trim().to_string());
            }
        }
    }
    out
}

/// Loose dependency extraction: lowercase words of a manifest (good enough for signatures).
fn text_deps(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.'))
        .filter(|w| w.len() > 2)
        .map(|w| w.to_ascii_lowercase())
        .collect()
}

/// Current git branch for a project root (searches upwards for `.git`).
pub fn git_branch(root: &Path) -> Option<String> {
    let mut dir = Some(root);
    for _ in 0..6 {
        let d = dir?;
        let git = d.join(".git");
        let git_dir = if git.is_dir() {
            git
        } else if git.is_file() {
            // Worktrees / submodules: "gitdir: <path>"
            let content = fs::read_to_string(&git).ok()?;
            let p = PathBuf::from(content.trim().strip_prefix("gitdir:")?.trim());
            if p.is_absolute() {
                p
            } else {
                d.join(p)
            }
        } else {
            dir = d.parent();
            continue;
        };
        let head = fs::read_to_string(git_dir.join("HEAD")).ok()?;
        let head = head.trim();
        return Some(match head.strip_prefix("ref: refs/heads/") {
            Some(b) => b.to_string(),
            None => head.chars().take(7).collect(),
        });
    }
    None
}

struct Sig {
    needles: &'static [&'static str],
    name: &'static str,
    cat: C,
}

const fn sig(needles: &'static [&'static str], name: &'static str, cat: C) -> Sig {
    Sig { needles, name, cat }
}

/// Command-line signatures, most specific first. Matched against the lowercase
/// "name + full command line".
const SIGNATURES: &[Sig] = &[
    // Containers & forwarders
    sig(&["docker-proxy"], "Docker", C::Container),
    sig(
        &[
            "com.docker.backend",
            "com.docker.vpnkit",
            "vpnkit",
            "docker desktop",
        ],
        "Docker Desktop",
        C::Container,
    ),
    sig(&["orbstack helper", "orbstack"], "OrbStack", C::Container),
    sig(
        &["rootlessport", "gvproxy", "conmon"],
        "Podman",
        C::Container,
    ),
    sig(&["wslrelay"], "WSL relay", C::System),
    sig(
        &["kubectl port-forward", "kubectl  port-forward"],
        "kubectl port-forward",
        C::Tool,
    ),
    // JS dev servers
    sig(
        &[
            "next-server",
            "next dev",
            "next start",
            "/next/dist/",
            "next/dist/bin",
        ],
        "Next.js",
        C::DevServer,
    ),
    sig(&["nuxt", "nuxi"], "Nuxt", C::DevServer),
    sig(
        &["astro dev", "astro preview", "/astro/"],
        "Astro",
        C::DevServer,
    ),
    sig(&["svelte-kit", "@sveltejs/kit"], "SvelteKit", C::DevServer),
    sig(
        &["remix dev", "remix-serve", "@remix-run"],
        "Remix",
        C::DevServer,
    ),
    sig(
        &["ng serve", "@angular/cli", "@angular-devkit"],
        "Angular",
        C::DevServer,
    ),
    sig(&["react-scripts"], "Create React App", C::DevServer),
    sig(&["gatsby develop", "gatsby serve"], "Gatsby", C::DevServer),
    sig(&["storybook"], "Storybook", C::DevServer),
    sig(
        &["expo start", "@expo/cli", "metro"],
        "Expo / Metro",
        C::DevServer,
    ),
    sig(
        &["webpack-dev-server", "webpack serve"],
        "webpack",
        C::DevServer,
    ),
    sig(&["vite"], "Vite", C::DevServer),
    sig(&["parcel"], "Parcel", C::DevServer),
    sig(&["turbo run", "turbo dev"], "Turborepo", C::DevServer),
    sig(&["nodemon"], "nodemon", C::DevServer),
    sig(
        &["live-server", "http-server", "serve -s", "browser-sync"],
        "Static server",
        C::DevServer,
    ),
    sig(&["esbuild"], "esbuild", C::DevServer),
    sig(&["wrangler"], "Wrangler", C::DevServer),
    sig(&["vercel dev"], "Vercel CLI", C::DevServer),
    sig(
        &["firebase emulators", "cloud-firestore-emulator"],
        "Firebase emulator",
        C::DevServer,
    ),
    // Rust
    sig(&["cargo-tauri", "tauri dev"], "Tauri dev", C::DevServer),
    sig(&["trunk serve"], "Trunk", C::DevServer),
    sig(&["cargo-leptos"], "Leptos", C::DevServer),
    // Python
    sig(&["manage.py runserver", "django"], "Django", C::DevServer),
    sig(&["uvicorn"], "Uvicorn", C::AppServer),
    sig(&["gunicorn"], "Gunicorn", C::AppServer),
    sig(&["hypercorn"], "Hypercorn", C::AppServer),
    sig(&["flask run", "flask"], "Flask", C::DevServer),
    sig(&["streamlit"], "Streamlit", C::DevServer),
    sig(&["jupyter", "ipykernel"], "Jupyter", C::Tool),
    sig(
        &["http.server", "simplehttpserver"],
        "Python http.server",
        C::DevServer,
    ),
    sig(&["mkdocs serve"], "MkDocs", C::DevServer),
    // Ruby / PHP / others
    sig(
        &["rails server", "rails s", "bin/rails"],
        "Rails",
        C::DevServer,
    ),
    sig(&["puma"], "Puma", C::AppServer),
    sig(&["jekyll serve"], "Jekyll", C::DevServer),
    sig(&["artisan serve"], "Laravel", C::DevServer),
    sig(&["php -s", "php-fpm"], "PHP", C::AppServer),
    sig(&["hugo server"], "Hugo", C::DevServer),
    sig(&["mix phx.server", "phoenix"], "Phoenix", C::DevServer),
    sig(
        &["spring-boot", "springframework", "bootrun"],
        "Spring Boot",
        C::AppServer,
    ),
    sig(&["dotnet watch", "dotnet run"], ".NET", C::DevServer),
    // Data stores, queues, infra
    sig(&["postgres", "postmaster"], "PostgreSQL", C::Database),
    sig(&["mysqld", "mariadbd"], "MySQL", C::Database),
    sig(&["mongod"], "MongoDB", C::Database),
    sig(&["redis-server"], "Redis", C::Cache),
    sig(&["valkey-server"], "Valkey", C::Cache),
    sig(&["memcached"], "Memcached", C::Cache),
    sig(&["rabbitmq", "rabbit@"], "RabbitMQ", C::Queue),
    sig(&["kafka"], "Kafka", C::Queue),
    sig(&["nats-server"], "NATS", C::Queue),
    sig(&["elasticsearch"], "Elasticsearch", C::Database),
    sig(&["opensearch"], "OpenSearch", C::Database),
    sig(&["clickhouse"], "ClickHouse", C::Database),
    sig(&["influxd"], "InfluxDB", C::Database),
    sig(&["cockroach"], "CockroachDB", C::Database),
    sig(&["minio"], "MinIO", C::Database),
    sig(&["etcd"], "etcd", C::Database),
    sig(&["ollama"], "Ollama", C::Tool),
    sig(&["mailpit", "mailhog"], "Mail catcher", C::Tool),
    sig(&["ngrok"], "ngrok", C::Tool),
    sig(&["cloudflared"], "cloudflared", C::Tool),
    sig(&["ssh -l", "ssh -n -l"], "SSH tunnel", C::Tool),
    sig(&["nginx"], "nginx", C::WebServer),
    sig(&["caddy"], "Caddy", C::WebServer),
    sig(&["httpd", "apache2"], "Apache httpd", C::WebServer),
    sig(&["traefik"], "Traefik", C::WebServer),
    sig(&["lighttpd"], "lighttpd", C::WebServer),
    // Desktop apps (not dev servers)
    sig(
        &["code helper", "code-insiders", "cursor helper", "electron"],
        "Editor / Electron app",
        C::App,
    ),
    sig(
        &[
            "jetbrains",
            "intellij",
            "webstorm",
            "pycharm",
            "goland",
            "rustrover",
        ],
        "JetBrains IDE",
        C::App,
    ),
    sig(&["spotify"], "Spotify", C::App),
    sig(&["slack"], "Slack", C::App),
    sig(&["discord"], "Discord", C::App),
    sig(&["zoom"], "Zoom", C::App),
    sig(
        &["chrome", "chromium", "firefox", "safari", "msedge"],
        "Browser",
        C::App,
    ),
    sig(&["figma_agent", "figma"], "Figma", C::App),
    sig(&["dropbox"], "Dropbox", C::App),
    sig(&["syncthing"], "Syncthing", C::App),
    // OS services
    sig(&["controlcenter"], "AirPlay Receiver", C::System),
    sig(
        &[
            "rapportd",
            "sharingd",
            "mdnsresponder",
            "remoted",
            "launchd",
        ],
        "macOS service",
        C::System,
    ),
    sig(&["sshd"], "OpenSSH server", C::System),
    sig(
        &["systemd-resolve", "systemd-network", "systemd"],
        "systemd",
        C::System,
    ),
    sig(
        &[
            "avahi-daemon",
            "cupsd",
            "chronyd",
            "dnsmasq",
            "rpcbind",
            "exim",
            "postfix",
            "master -w",
        ],
        "System service",
        C::System,
    ),
    sig(
        &[
            "svchost.exe",
            "lsass.exe",
            "wininit.exe",
            "services.exe",
            "spoolsv.exe",
        ],
        "Windows service",
        C::System,
    ),
];

/// Runtime fallbacks when no signature matches (matched on the process name only).
const RUNTIMES: &[(&str, &str)] = &[
    ("node", "Node.js"),
    ("bun", "Bun"),
    ("deno", "Deno"),
    ("python", "Python"),
    ("ruby", "Ruby"),
    ("java", "Java"),
    ("dotnet", ".NET"),
    ("beam.smp", "Erlang/Elixir"),
    ("php", "PHP"),
    ("go", "Go"),
];

/// Dependency-based refinement for generic runtimes (node/python/…).
const DEP_FRAMEWORKS: &[(&str, &str, C)] = &[
    ("next", "Next.js", C::DevServer),
    ("nuxt", "Nuxt", C::DevServer),
    ("@sveltejs/kit", "SvelteKit", C::DevServer),
    ("astro", "Astro", C::DevServer),
    ("@remix-run/dev", "Remix", C::DevServer),
    ("@angular/core", "Angular", C::DevServer),
    ("vite", "Vite", C::DevServer),
    ("react-scripts", "Create React App", C::DevServer),
    ("@nestjs/core", "NestJS", C::AppServer),
    ("fastify", "Fastify", C::AppServer),
    ("express", "Express", C::AppServer),
    ("hono", "Hono", C::AppServer),
    ("koa", "Koa", C::AppServer),
    ("fastapi", "FastAPI", C::AppServer),
    ("django", "Django", C::DevServer),
    ("flask", "Flask", C::DevServer),
    ("axum", "Axum", C::AppServer),
    ("actix-web", "Actix Web", C::AppServer),
    ("rocket", "Rocket", C::AppServer),
    ("tauri", "Tauri", C::DevServer),
    ("rails", "Rails", C::DevServer),
    ("laravel/framework", "Laravel", C::DevServer),
    ("phoenix", "Phoenix", C::DevServer),
    ("github.com/gin-gonic/gin", "Gin", C::AppServer),
    ("github.com/labstack/echo", "Echo", C::AppServer),
    ("github.com/gofiber/fiber", "Fiber", C::AppServer),
];

/// `needle` occurs in `hay` starting at a word boundary (so "vite" doesn't match "invite").
fn contains_word(hay: &str, needle: &str) -> bool {
    let bytes = hay.as_bytes();
    hay.match_indices(needle)
        .any(|(i, _)| i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
}

fn haystack(p: &ProcessInfo) -> String {
    let mut s = p.name.to_ascii_lowercase();
    for a in &p.cmdline {
        s.push(' ');
        s.push_str(&a.to_ascii_lowercase());
    }
    s
}

/// Detect the framework for a process, optionally refined by its project's dependencies.
pub fn detect_framework(p: &ProcessInfo, project: Option<&ProjectDetails>) -> Option<Framework> {
    let hay = haystack(p);
    let name = p.name.to_ascii_lowercase();
    let generic_vite = |f: &Framework| f.name == "Vite";
    if let Some(s) = SIGNATURES
        .iter()
        .find(|s| s.needles.iter().any(|n| contains_word(&hay, n)))
    {
        let f = Framework {
            name: s.name.into(),
            category: s.cat,
        };
        // "vite" inside a SvelteKit/Astro/Remix project is really that framework.
        if generic_vite(&f) {
            if let Some(better) = project.and_then(dep_framework) {
                if matches!(
                    better.name.as_str(),
                    "SvelteKit" | "Astro" | "Remix" | "Nuxt"
                ) {
                    return Some(better);
                }
            }
        }
        return Some(f);
    }
    let runtime = RUNTIMES.iter().find(|(r, _)| {
        name == *r
            || name.starts_with(&format!("{r}3"))
            || name.starts_with(&format!("{r}."))
            || (*r == "node" && name.starts_with("node"))
    });
    // Only trust manifest dependencies that belong to the process's own ecosystem: a python
    // process in a directory that also has a Cargo.toml isn't an Axum server.
    let compatible = |p: &&ProjectDetails| match runtime {
        Some((r, _)) => manifest_matches_runtime(&p.info.kind, r),
        None => true,
    };
    if let Some(f) = project.filter(compatible).and_then(dep_framework) {
        return Some(f);
    }
    runtime.map(|(_, label)| Framework {
        name: (*label).into(),
        category: C::AppServer,
    })
}

fn manifest_matches_runtime(kind: &str, runtime: &str) -> bool {
    match runtime {
        "node" | "bun" | "deno" => matches!(kind, "package.json" | "deno.json"),
        "python" => matches!(kind, "pyproject.toml" | "manage.py" | "requirements.txt"),
        "ruby" => kind == "Gemfile",
        "php" => kind == "composer.json",
        "java" => matches!(kind, "pom.xml" | "build.gradle" | "build.gradle.kts"),
        "beam.smp" => kind == "mix.exs",
        "go" => kind == "go.mod",
        _ => true,
    }
}

fn dep_framework(project: &ProjectDetails) -> Option<Framework> {
    DEP_FRAMEWORKS.iter().find_map(|(dep, name, cat)| {
        project
            .deps
            .iter()
            .any(|d| d == dep || d == &format!("script:{dep}"))
            .then(|| Framework {
                name: (*name).into(),
                category: *cat,
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::tests::proc;

    #[test]
    fn detects_package_json_project_with_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("shop-web");
        fs::create_dir_all(root.join("src/app")).unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"name":"@acme/shop-web","scripts":{"dev":"next dev"},"dependencies":{"next":"15","react":"19"}}"#,
        )
        .unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/feat/checkout\n").unwrap();
        let d = detect_project(&root.join("src/app"), None).unwrap();
        assert_eq!(d.info.name, "shop-web");
        assert_eq!(d.info.kind, "package.json");
        assert_eq!(d.info.root, root);
        assert_eq!(d.info.git_branch.as_deref(), Some("feat/checkout"));
        let mut p = proc(5, 1, "node", &["node", "server.js"]);
        p.cwd = Some(root.clone());
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "Next.js");
    }

    #[test]
    fn detects_cargo_and_python_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let rs = tmp.path().join("api");
        fs::create_dir_all(&rs).unwrap();
        fs::write(
            rs.join("Cargo.toml"),
            "[package]\nname = \"orders-api\"\n\n[dependencies]\naxum = \"0.8\"\n",
        )
        .unwrap();
        let d = detect_project(&rs, None).unwrap();
        assert_eq!(d.info.name, "orders-api");
        assert!(d.deps.contains(&"axum".to_string()));
        let p = proc(5, 1, "orders-api", &["target/debug/orders-api"]);
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "Axum");
        // A python process that happens to run inside a Rust project is not Axum.
        let p = proc(7, 1, "python3", &["python3", "serve.py"]);
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "Python");

        let py = tmp.path().join("ml");
        fs::create_dir_all(&py).unwrap();
        fs::write(
            py.join("pyproject.toml"),
            "[project]\nname = \"ml-svc\"\ndependencies = [\"fastapi>=0.1\"]\n",
        )
        .unwrap();
        let d = detect_project(&py, None).unwrap();
        assert_eq!(d.info.name, "ml-svc");
        let p = proc(6, 1, "python3", &["python3", "-m", "uvicorn", "main:app"]);
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "Uvicorn");
        let p = proc(6, 1, "python3", &["python3", "main.py"]);
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "FastAPI");
    }

    #[test]
    fn home_is_not_a_project() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("package.json"), "{}").unwrap();
        assert!(detect_project(tmp.path(), Some(tmp.path())).is_none());
    }

    #[test]
    fn signatures() {
        let cases = [
            (
                proc(1, 0, "postgres", &["postgres", "-D", "/var/lib/pg"]),
                "PostgreSQL",
            ),
            (
                proc(1, 0, "redis-server", &["redis-server *:6379"]),
                "Redis",
            ),
            (
                proc(1, 0, "node", &["node", "/app/node_modules/.bin/vite"]),
                "Vite",
            ),
            (
                proc(
                    1,
                    0,
                    "docker-proxy",
                    &["/usr/bin/docker-proxy", "-proto", "tcp"],
                ),
                "Docker",
            ),
            (
                proc(
                    1,
                    0,
                    "ControlCenter",
                    &["/System/Library/CoreServices/ControlCenter.app"],
                ),
                "AirPlay Receiver",
            ),
            (
                proc(1, 0, "python3", &["python3", "manage.py", "runserver"]),
                "Django",
            ),
            (
                proc(1, 0, "python3", &["python3", "-m", "http.server", "8000"]),
                "Python http.server",
            ),
            (proc(1, 0, "node", &["node", "index.js"]), "Node.js"),
        ];
        for (p, want) in cases {
            assert_eq!(
                detect_framework(&p, None).map(|f| f.name),
                Some(want.to_string()),
                "{:?}",
                p.cmdline
            );
        }
        assert!(detect_framework(&proc(1, 0, "mystery", &["mystery"]), None).is_none());
        assert!(detect_framework(&proc(1, 0, "invite-svc", &["invite-svc"]), None).is_none());
    }

    #[test]
    fn vite_in_sveltekit_project() {
        let d = ProjectDetails {
            info: ProjectInfo {
                name: "x".into(),
                root: "/x".into(),
                kind: "package.json".into(),
                git_branch: None,
            },
            deps: vec!["@sveltejs/kit".into(), "vite".into()],
        };
        let p = proc(1, 0, "node", &["node", "node_modules/.bin/vite", "dev"]);
        assert_eq!(detect_framework(&p, Some(&d)).unwrap().name, "SvelteKit");
    }
}
