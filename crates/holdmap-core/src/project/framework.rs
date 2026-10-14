//! Framework detection: command-line signatures ("next dev", "uvicorn", "postgres"), refined by
//! the project's dependencies.

use super::ProjectDetails;
use crate::model::{Framework, FrameworkCategory as C, ProcessInfo};

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
    sig(&["llama-server"], "llama.cpp", C::Tool),
    sig(&["lm-studio", "lmstudio"], "LM Studio", C::Tool),
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
    // An editor extension's service runs in the open workspace, but the workspace's
    // dependencies say nothing about it.
    if let Some(editor) = crate::safety::editor_extension(p) {
        return Some(Framework {
            name: format!("{editor} extension"),
            category: C::App,
        });
    }
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
    // After product signatures: a generic `node --inspect` is a debugger, not "Node.js".
    // Next.js/Vite already matched above and keep their product label; well-known inspector
    // ports are named in the agent's network fact.
    if node_inspector(p) {
        return Some(Framework {
            name: "Node inspector".into(),
            category: C::Tool,
        });
    }
    let runtime = RUNTIMES.iter().find(|(r, _)| {
        name == *r
            || name.starts_with(&format!("{r}3"))
            || name.starts_with(&format!("{r}."))
            || (*r == "node" && name.starts_with("node"))
    });
    // Only trust manifest dependencies that belong to the process's own ecosystem: a python
    // process in a directory that also has a Cargo.toml isn't an Axum server, and a native
    // helper started in a Next.js checkout isn't a Next.js server. Launchers (`npm run dev`,
    // `cargo run`) run the project's own code.
    let compatible = |project: &&ProjectDetails| match runtime {
        Some((r, _)) => manifest_matches_runtime(&project.info.kind, r),
        None => crate::engine::is_launcher(p) || runs_project_build(p, project),
    };
    if let Some(f) = project.filter(compatible).and_then(dep_framework) {
        return Some(f);
    }
    runtime.map(|(_, label)| Framework {
        name: (*label).into(),
        category: C::AppServer,
    })
}

/// A native process is the project's own build when the project compiles to a binary and the
/// executable lives in it (`target/debug/api`, `./bin/server`) or in Go's temporary `go run`
/// build directory.
fn runs_project_build(p: &ProcessInfo, project: &ProjectDetails) -> bool {
    if !matches!(project.info.kind.as_str(), "Cargo.toml" | "go.mod") {
        return false;
    }
    let argv0 = p.cmdline.first().map(std::path::Path::new);
    p.exe.as_deref().into_iter().chain(argv0).any(|path| {
        // `target/debug/api` is relative. `/Users/me/.cargo/bin/rust-analyzer` is absolute
        // on Unix but relative-with-root on Windows, so `is_relative()` alone is not enough.
        (path.is_relative() && !path.has_root() && path.components().count() > 1)
            || path.starts_with(&project.info.root)
            || path.components().any(|c| {
                c.as_os_str()
                    .to_str()
                    .is_some_and(|s| s.starts_with("go-build"))
            })
    })
}

fn node_inspector(p: &ProcessInfo) -> bool {
    let mut args = p
        .cmdline
        .iter()
        .take_while(|a| a.as_str() != "--")
        .map(String::as_str);
    while let Some(a) = args.next() {
        if matches!(a, "--inspect" | "--inspect-brk" | "--inspect-wait") {
            return true;
        }
        if let Some(v) = a
            .strip_prefix("--inspect=")
            .or_else(|| a.strip_prefix("--inspect-brk="))
            .or_else(|| a.strip_prefix("--inspect-wait="))
        {
            return inspect_bind_active(v);
        }
        if a == "--inspect-port" {
            return args.next().is_some_and(inspect_port_active);
        }
        if let Some(v) = a.strip_prefix("--inspect-port=") {
            return inspect_port_active(v);
        }
    }
    false
}

fn inspect_port_active(value: &str) -> bool {
    value != "0"
}

fn inspect_bind_active(value: &str) -> bool {
    inspect_port_active(
        value
            .rsplit_once(':')
            .map(|(_, port)| port)
            .unwrap_or(value),
    )
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
