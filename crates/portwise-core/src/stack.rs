//! Project stacks: a `.portwise.toml` at a project root names its services, their ports and how
//! to start them, so `portwise up`, `down` and `status` can manage the whole stack.
//!
//! ```toml
//! name = "shop"            # optional; defaults to the directory name
//! protect = [5432]         # ports `portwise stop` and `down` must never touch
//!
//! [services.api]
//! port = 4000
//! command = "npm run dev"  # run through the shell, with PORT set
//! cwd = "apps/api"         # relative to this file
//! health = "/health"       # HTTP path checked by `status` (optional)
//!
//! [services.web]
//! port = 3000
//! command = "npm run dev"
//! cwd = "apps/web"
//! depends_on = ["api"]     # started after api is accepting connections
//! env = { API_URL = "http://localhost:4000" }
//!
//! [services.db]
//! port = 5432              # no command: started elsewhere (e.g. docker compose), only checked
//! ```
//!
//! This module is pure (parsing, validation, ordering, status); starting and stopping processes
//! is left to the front-end, which goes through the normal [`crate::Engine`] safety checks.

use crate::model::{PortEntry, Protocol, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The file name looked up from the current directory upwards.
pub const FILE_NAME: &str = ".portwise.toml";

/// Default time `up` waits for a service to accept connections.
pub const DEFAULT_READY_TIMEOUT_S: u64 = 60;

/// A problem with a `.portwise.toml`.
#[derive(Debug, thiserror::Error)]
pub enum StackError {
    /// The file couldn't be read.
    #[error("can't read {path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// The file could be changed by someone else, so its commands won't be run.
    #[error("refusing {path}: {message}")]
    Unsafe {
        /// The file.
        path: PathBuf,
        /// Why it isn't trusted.
        message: String,
    },
    /// The file isn't valid TOML or has unknown keys.
    #[error("{path}: {message}")]
    Parse {
        /// The file.
        path: PathBuf,
        /// The parser's message.
        message: String,
    },
    /// The file parses but doesn't make sense (unknown dependency, cycle, duplicate port…).
    #[error("{path}: {message}")]
    Invalid {
        /// The file.
        path: PathBuf,
        /// What's wrong.
        message: String,
    },
    /// A service name given on the command line doesn't exist.
    #[error("no service named `{name}` (known: {known})")]
    UnknownService {
        /// The requested name.
        name: String,
        /// The services the file defines.
        known: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    name: Option<String>,
    #[serde(default)]
    protect: Vec<u16>,
    #[serde(default)]
    services: BTreeMap<String, RawService>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawService {
    port: Option<u16>,
    command: Option<String>,
    cwd: Option<PathBuf>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    depends_on: Vec<String>,
    health: Option<String>,
    ready_timeout_s: Option<u64>,
}

/// A parsed, validated stack.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Stack {
    /// Stack name (`name`, or the directory name).
    pub name: String,
    /// Directory containing the file.
    pub root: PathBuf,
    /// The file itself.
    pub file: PathBuf,
    /// Ports that must never be stopped.
    pub protect: Vec<u16>,
    /// Services in start order: every service comes after the ones it depends on.
    pub services: Vec<Service>,
}

/// One service of a stack.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Service {
    /// Service name (the `[services.NAME]` key).
    pub name: String,
    /// The TCP port it listens on.
    pub port: Option<u16>,
    /// Shell command that starts it; `None` for services started elsewhere.
    pub command: Option<String>,
    /// Absolute working directory.
    pub cwd: PathBuf,
    /// Extra environment variables (`PORT` is added automatically).
    pub env: BTreeMap<String, String>,
    /// Services that must be up first.
    pub depends_on: Vec<String>,
    /// HTTP path probed by `status`, e.g. `/health`.
    pub health: Option<String>,
    /// How long `up` waits for the port to accept connections.
    pub ready_timeout_s: u64,
}

/// Find `.portwise.toml` in `start` or the nearest ancestor.
pub fn find(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|d| d.join(FILE_NAME))
        .find(|f| f.is_file())
}

/// A stack file runs its commands through the shell, so like git's `safe.directory` only
/// trust one the current user (or root) owns and other users can't write to. This stops a
/// `.portwise.toml` dropped into a shared parent directory (`/tmp`, a shared checkout) from
/// running someone else's commands as you.
pub fn check_trusted(path: &Path) -> Result<(), StackError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let unsafe_ = |message: String| StackError::Unsafe {
            path: path.to_path_buf(),
            message,
        };
        let m = std::fs::metadata(path).map_err(|source| StackError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        // SAFETY: geteuid has no preconditions and can't fail.
        let me = unsafe { libc::geteuid() };
        if m.uid() != me && m.uid() != 0 {
            return Err(unsafe_(format!(
                "it belongs to another user (uid {}); portwise only runs stack files you own",
                m.uid()
            )));
        }
        if m.mode() & 0o002 != 0 {
            return Err(unsafe_(
                "anyone can write to it; run `chmod o-w` on it first".into(),
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Read and validate a stack file (refusing one that isn't [`check_trusted`]).
pub fn load(path: &Path) -> Result<Stack, StackError> {
    check_trusted(path)?;
    let text = std::fs::read_to_string(path).map_err(|source| StackError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let root = path
        .parent()
        .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()))
        .unwrap_or_default();
    parse(&text, &root, path)
}

fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Parse stack-file `text` whose directory is `root` (`file` is only used in messages).
pub fn parse(text: &str, root: &Path, file: &Path) -> Result<Stack, StackError> {
    let invalid = |message: String| StackError::Invalid {
        path: file.to_path_buf(),
        message,
    };
    let raw: RawFile = toml::from_str(text).map_err(|e| StackError::Parse {
        path: file.to_path_buf(),
        message: e.message().to_string(),
    })?;
    let mut ports: BTreeMap<u16, &str> = BTreeMap::new();
    for (name, s) in &raw.services {
        if !valid_name(name) {
            return Err(invalid(format!(
                "service name `{name}` must be letters, digits, `-` or `_`"
            )));
        }
        if let Some(p) = s.port {
            if p == 0 {
                return Err(invalid(format!("service `{name}`: port must be 1–65535")));
            }
            if let Some(other) = ports.insert(p, name) {
                return Err(invalid(format!(
                    "services `{other}` and `{name}` both use port {p}"
                )));
            }
        }
        if s.command.as_deref().is_some_and(|c| c.trim().is_empty()) {
            return Err(invalid(format!("service `{name}`: command is empty")));
        }
        if s.command.is_some() && s.port.is_none() {
            return Err(invalid(format!(
                "service `{name}` has a command but no port; portwise needs the port to know when it's up and what to stop"
            )));
        }
        if let Some(h) = &s.health {
            if !h.starts_with('/') {
                return Err(invalid(format!(
                    "service `{name}`: health must be a path like `/health`"
                )));
            }
        }
        for d in &s.depends_on {
            if !raw.services.contains_key(d) {
                return Err(invalid(format!(
                    "service `{name}` depends on unknown service `{d}`"
                )));
            }
        }
        for k in s.env.keys() {
            if k.is_empty() || k.contains('=') {
                return Err(invalid(format!(
                    "service `{name}`: invalid environment variable name `{k}`"
                )));
            }
        }
    }
    if let Some(p) = raw.protect.iter().find(|p| **p == 0) {
        return Err(invalid(format!("protect: invalid port {p}")));
    }
    let order = topo_order(&raw.services).map_err(invalid)?;
    let name = raw
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| {
            root.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "stack".into())
        });
    let mut services = Vec::new();
    for n in order {
        let s = &raw.services[&n];
        let cwd = match &s.cwd {
            Some(c) if c.is_absolute() => c.clone(),
            Some(c) => root.join(c),
            None => root.to_path_buf(),
        };
        services.push(Service {
            name: n.clone(),
            port: s.port,
            command: s.command.clone(),
            cwd,
            env: s.env.clone(),
            depends_on: s.depends_on.clone(),
            health: s.health.clone(),
            ready_timeout_s: s.ready_timeout_s.unwrap_or(DEFAULT_READY_TIMEOUT_S),
        });
    }
    Ok(Stack {
        name,
        root: root.to_path_buf(),
        file: file.to_path_buf(),
        protect: raw.protect,
        services,
    })
}

/// Deterministic topological order (dependencies first, then by name); errors on a cycle.
fn topo_order(services: &BTreeMap<String, RawService>) -> Result<Vec<String>, String> {
    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    while out.len() < services.len() {
        let ready = services
            .iter()
            .find(|(n, s)| !done.contains(*n) && s.depends_on.iter().all(|d| done.contains(d)));
        match ready {
            Some((n, _)) => {
                done.insert(n.clone());
                out.push(n.clone());
            }
            None => {
                let left: Vec<&str> = services
                    .keys()
                    .filter(|n| !done.contains(*n))
                    .map(String::as_str)
                    .collect();
                return Err(format!(
                    "dependency cycle between services: {}",
                    left.join(", ")
                ));
            }
        }
    }
    Ok(out)
}

impl Stack {
    /// Look a service up by name.
    pub fn service(&self, name: &str) -> Option<&Service> {
        self.services.iter().find(|s| s.name == name)
    }

    fn check(&self, names: &[String]) -> Result<(), StackError> {
        for n in names {
            if self.service(n).is_none() {
                return Err(StackError::UnknownService {
                    name: n.clone(),
                    known: self
                        .services
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                });
            }
        }
        Ok(())
    }

    /// Services to start for `names` (all when empty) plus everything they depend on, in start
    /// order.
    pub fn start_order(&self, names: &[String]) -> Result<Vec<&Service>, StackError> {
        self.check(names)?;
        if names.is_empty() {
            return Ok(self.services.iter().collect());
        }
        let mut want: BTreeSet<&str> = names.iter().map(String::as_str).collect();
        // Services are already in dependency order, so one backwards pass closes the set.
        for s in self.services.iter().rev() {
            if want.contains(s.name.as_str()) {
                want.extend(s.depends_on.iter().map(String::as_str));
            }
        }
        Ok(self
            .services
            .iter()
            .filter(|s| want.contains(s.name.as_str()))
            .collect())
    }

    /// Services to stop for `names` (all when empty) plus everything that depends on them, in
    /// stop order (dependents first).
    pub fn stop_order(&self, names: &[String]) -> Result<Vec<&Service>, StackError> {
        self.check(names)?;
        let mut want: BTreeSet<&str> = if names.is_empty() {
            self.services.iter().map(|s| s.name.as_str()).collect()
        } else {
            names.iter().map(String::as_str).collect()
        };
        for s in &self.services {
            if s.depends_on.iter().any(|d| want.contains(d.as_str())) {
                want.insert(s.name.as_str());
            }
        }
        Ok(self
            .services
            .iter()
            .rev()
            .filter(|s| want.contains(s.name.as_str()))
            .collect())
    }

    /// Does `entry` belong to this stack? Its process runs under the stack root (or a service's
    /// `cwd`), its project root is under it, or it's a container of the compose project named like the stack.
    pub fn owns(&self, entry: &PortEntry) -> bool {
        let under = |p: &Path| {
            p.starts_with(&self.root) || self.services.iter().any(|s| p.starts_with(&s.cwd))
        };
        entry
            .process
            .as_ref()
            .and_then(|p| p.cwd.as_deref())
            .is_some_and(under)
            || entry.project.as_ref().is_some_and(|p| under(&p.root))
            || entry
                .container
                .as_ref()
                .and_then(|c| c.compose_project.as_deref())
                .is_some_and(|c| c.eq_ignore_ascii_case(&self.name))
    }

    /// Current state of every service.
    pub fn status(&self, snap: &Snapshot) -> Vec<ServiceStatus> {
        self.services
            .iter()
            .map(|s| service_status(self, s, snap))
            .collect()
    }
}

/// Where a service stands right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    /// Its port is held by a process that belongs to this stack (or, for a service without a
    /// `command`, by anything).
    Running,
    /// Its port is free.
    Stopped,
    /// Its port is held by something else (another project, another user, a hidden owner).
    Conflict,
    /// The service has no port, so its state is unknown.
    Unknown,
}

/// A service and what holds its port.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServiceStatus {
    /// Service name.
    pub name: String,
    /// Its port.
    pub port: Option<u16>,
    /// The state.
    pub state: ServiceState,
    /// The listener on the port, if any.
    pub holder: Option<PortEntry>,
}

fn service_status(stack: &Stack, s: &Service, snap: &Snapshot) -> ServiceStatus {
    let Some(port) = s.port else {
        return ServiceStatus {
            name: s.name.clone(),
            port: None,
            state: ServiceState::Unknown,
            holder: None,
        };
    };
    let holders: Vec<&PortEntry> = snap
        .entries
        .iter()
        .filter(|e| e.port == port && e.protocol == Protocol::Tcp && e.state.is_listening())
        .collect();
    let mine = holders.iter().find(|e| stack.owns(e));
    let (state, holder) = match (mine, holders.first()) {
        (Some(e), _) => (ServiceState::Running, Some((*e).clone())),
        // No `command`: the project doesn't start it (a shared database, a container started
        // elsewhere), so whatever listens on the port satisfies it.
        (None, Some(e)) if s.command.is_none() => (ServiceState::Running, Some((*e).clone())),
        (None, Some(e)) => (ServiceState::Conflict, Some((*e).clone())),
        (None, None) => (ServiceState::Stopped, None),
    };
    ServiceStatus {
        name: s.name.clone(),
        port: Some(port),
        state,
        holder,
    }
}

/// A starter `.portwise.toml` for `services` (name, port, command, cwd relative to the root).
/// With no services it returns a commented example.
pub fn scaffold(name: &str, services: &[(String, u16, Option<String>, Option<String>)]) -> String {
    let mut s = format!(
        "# portwise project file: `portwise up`, `portwise down`, `portwise status`.\n\
         name = {}\n\
         # protect = [5432]   # ports portwise must never stop\n",
        toml_str(name)
    );
    if services.is_empty() {
        s.push_str(
            "\n[services.web]\nport = 3000\ncommand = \"npm run dev\"\n# cwd = \"apps/web\"\n# depends_on = [\"api\"]\n# health = \"/\"\n",
        );
        return s;
    }
    for (n, port, cmd, cwd) in services {
        s.push_str(&format!("\n[services.{n}]\nport = {port}\n"));
        match cmd {
            Some(c) => s.push_str(&format!("command = {}\n", toml_str(c))),
            None => s.push_str("# command = \"…\"   # how to start it\n"),
        }
        if let Some(c) = cwd.as_ref().filter(|c| !c.is_empty() && *c != ".") {
            s.push_str(&format!("cwd = {}\n", toml_str(c)));
        }
    }
    s
}

fn toml_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A service name derived from a label: lower-case, with anything else turned into `-`.
pub fn service_name(raw: &str) -> String {
    let mut out = String::new();
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    let out: String = out.chars().take(40).collect();
    if out.is_empty() {
        "service".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    const SHOP: &str = r#"
name = "shop"
protect = [5432]

[services.web]
port = 3000
command = "npm run dev"
cwd = "apps/web"
depends_on = ["api"]
env = { API_URL = "http://localhost:4000" }

[services.api]
port = 4000
command = "npm run dev"
cwd = "apps/api"
depends_on = ["db"]
health = "/health"

[services.db]
port = 5432
"#;

    fn shop() -> Stack {
        parse(
            SHOP,
            Path::new("/src/shop"),
            Path::new("/src/shop/.portwise.toml"),
        )
        .unwrap()
    }

    fn names(v: &[&Service]) -> Vec<String> {
        v.iter().map(|s| s.name.clone()).collect()
    }

    #[test]
    fn parses_and_orders_dependencies_first() {
        let s = shop();
        assert_eq!(s.name, "shop");
        assert_eq!(s.protect, vec![5432]);
        assert_eq!(
            names(&s.services.iter().collect::<Vec<_>>()),
            ["db", "api", "web"]
        );
        let web = s.service("web").unwrap();
        assert_eq!(web.cwd, Path::new("/src/shop/apps/web"));
        assert_eq!(web.env["API_URL"], "http://localhost:4000");
        assert_eq!(web.ready_timeout_s, DEFAULT_READY_TIMEOUT_S);
        assert_eq!(s.service("db").unwrap().cwd, Path::new("/src/shop"));
    }

    #[test]
    fn start_and_stop_closures() {
        let s = shop();
        assert_eq!(names(&s.start_order(&[]).unwrap()), ["db", "api", "web"]);
        assert_eq!(
            names(&s.start_order(&["api".into()]).unwrap()),
            ["db", "api"]
        );
        assert_eq!(names(&s.stop_order(&[]).unwrap()), ["web", "api", "db"]);
        assert_eq!(
            names(&s.stop_order(&["api".into()]).unwrap()),
            ["web", "api"]
        );
        assert_eq!(names(&s.stop_order(&["web".into()]).unwrap()), ["web"]);
        let err = s.start_order(&["nope".into()]).unwrap_err().to_string();
        assert!(
            err.contains("nope") && err.contains("db, api, web"),
            "{err}"
        );
    }

    #[test]
    fn name_defaults_to_the_directory() {
        let s = parse("", Path::new("/src/my-app"), Path::new("x")).unwrap();
        assert_eq!(s.name, "my-app");
        assert!(s.services.is_empty());
    }

    #[test]
    fn rejects_bad_files() {
        let bad = |t: &str| {
            parse(t, Path::new("/r"), Path::new("/r/.portwise.toml"))
                .unwrap_err()
                .to_string()
        };
        assert!(bad("[services.a]\nport = 1\n[services.b]\nport = 1").contains("both use port 1"));
        assert!(bad("[services.a]\nport = 0").contains("1–65535"));
        assert!(bad("[services.a]\ncommand = \"x\"").contains("no port"));
        assert!(
            bad("[services.a]\nport = 1\ndepends_on = [\"zz\"]").contains("unknown service `zz`")
        );
        assert!(bad("[services.a]\nport = 1\ndepends_on = [\"b\"]\n[services.b]\nport = 2\ndepends_on = [\"a\"]").contains("cycle"));
        assert!(bad("[services.a]\nport = 1\nhealth = \"health\"").contains("path like"));
        assert!(bad("[services.\"a b\"]\nport = 1").contains("letters, digits"));
        assert!(bad("[services.a]\nport = 1\ncommand = \"  \"").contains("empty"));
        assert!(bad("[services.a]\nport = 1\nprot = 2").contains("unknown field"));
        assert!(bad("port = ").contains(".portwise.toml"));
        assert!(bad("[services.a]\nport = 70000").contains(".portwise.toml"));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_world_writable_stack_file() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join(FILE_NAME);
        std::fs::write(
            &f,
            "[services.web]\nport = 3000\ncommand = \"npm run dev\"\n",
        )
        .unwrap();
        assert!(load(&f).is_ok());
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o666)).unwrap();
        let err = load(&f).unwrap_err();
        assert!(matches!(err, StackError::Unsafe { .. }), "{err}");
        assert!(err.to_string().contains("chmod o-w"));
    }

    #[test]
    fn finds_the_nearest_file() {
        let tmp = tempfile::tempdir().unwrap();
        let deep = tmp.path().join("a/b/c");
        std::fs::create_dir_all(&deep).unwrap();
        assert!(find(&deep).is_none() || !find(&deep).unwrap().starts_with(tmp.path()));
        std::fs::write(tmp.path().join("a").join(FILE_NAME), "name = \"x\"").unwrap();
        assert_eq!(find(&deep).unwrap(), tmp.path().join("a").join(FILE_NAME));
        let s = load(&find(&deep).unwrap()).unwrap();
        assert_eq!(s.name, "x");
        assert!(matches!(
            load(&tmp.path().join("missing")),
            Err(StackError::Io { .. })
        ));
    }

    fn entry(port: u16, cwd: Option<&str>, compose: Option<&str>) -> PortEntry {
        let mut e = PortEntry {
            id: format!("tcp:{port}"),
            port,
            protocol: Protocol::Tcp,
            state: SocketState::Listen,
            addresses: vec!["127.0.0.1".into()],
            families: vec![Family::V4],
            remote: None,
            exposure: Exposure::Loopback,
            pid: Some(100),
            pids: vec![100],
            uid: Some(1000),
            user: Some("me".into()),
            process: Some(ProcessInfo {
                pid: 100,
                ppid: Some(1),
                name: "node".into(),
                exe: None,
                cmdline: vec!["node".into()],
                cwd: cwd.map(PathBuf::from),
                uid: Some(1000),
                user: Some("me".into()),
                start_time: 0,
                start_token: 0,
                memory_bytes: 0,
                cpu_percent: 0.0,
            }),
            project: None,
            framework: None,
            container: None,
            label: "node".into(),
            is_dev: true,
            is_mine: true,
            protected: false,
            tunnel: None,
        };
        if let Some(c) = compose {
            e.container = Some(ContainerInfo {
                id: "c1".into(),
                name: format!("{c}-db-1"),
                image: "postgres:16".into(),
                runtime: "Docker".into(),
                compose_project: Some(c.into()),
                compose_service: Some("db".into()),
                private_port: 5432,
            });
            e.process = None;
        }
        e
    }

    #[test]
    fn status_tells_running_from_conflicts() {
        let s = shop();
        let snap = Snapshot {
            entries: vec![
                entry(3000, Some("/src/shop/apps/web"), None),
                entry(4000, Some("/src/other-project"), None),
                entry(5432, None, Some("shop")),
            ],
            hidden_sockets: 0,
            platform: "linux".into(),
            taken_at_ms: 0,
            scan_ms: 0,
            docker_available: true,
            warnings: vec![],
        };
        let st: BTreeMap<String, ServiceState> = s
            .status(&snap)
            .into_iter()
            .map(|x| (x.name, x.state))
            .collect();
        assert_eq!(st["web"], ServiceState::Running);
        assert_eq!(st["api"], ServiceState::Conflict);
        assert_eq!(st["db"], ServiceState::Running);
        // `db` has no command: a database started elsewhere still satisfies it.
        let foreign_db = Snapshot {
            entries: vec![entry(5432, Some("/opt/postgres"), None)],
            ..snap.clone()
        };
        let db = s.status(&foreign_db).into_iter().find(|x| x.name == "db");
        assert_eq!(db.map(|x| x.state), Some(ServiceState::Running));
        let empty = Snapshot {
            entries: vec![],
            ..snap
        };
        assert!(s
            .status(&empty)
            .iter()
            .all(|x| x.state == ServiceState::Stopped));
    }

    #[test]
    fn scaffold_round_trips() {
        let text = scaffold(
            "my \"app\"",
            &[
                (
                    "web".into(),
                    3000,
                    Some("npm run dev -- --host \"0.0.0.0\"".into()),
                    Some("apps/web".into()),
                ),
                ("db".into(), 5432, None, None),
            ],
        );
        let s = parse(&text, Path::new("/r"), Path::new("f")).unwrap();
        assert_eq!(s.name, "my \"app\"");
        assert_eq!(
            s.service("web").unwrap().command.as_deref(),
            Some("npm run dev -- --host \"0.0.0.0\"")
        );
        assert_eq!(s.service("web").unwrap().cwd, Path::new("/r/apps/web"));
        assert_eq!(s.service("db").unwrap().command, None);
        let example = parse(&scaffold("x", &[]), Path::new("/r"), Path::new("f")).unwrap();
        assert_eq!(example.services.len(), 1);
    }

    #[test]
    fn service_names() {
        assert_eq!(service_name("Next.js · shop-web"), "next-js-shop-web");
        assert_eq!(service_name("…"), "service");
        assert_eq!(service_name("api_v2"), "api_v2");
    }
}
