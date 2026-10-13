//! Project stacks: a `.holdmap.toml` at a project root names its services, their ports and how
//! to start them, so `holdmap up`, `down` and `status` can manage the whole stack.
//!
//! ```toml
//! name = "shop"            # optional; defaults to the directory name
//! protect = [5432]         # ports `holdmap stop` and `down` must never touch
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
pub const FILE_NAME: &str = ".holdmap.toml";
/// Pre-rename project file; still accepted when `.holdmap.toml` is absent.
pub const LEGACY_FILE_NAME: &str = ".portwise.toml";

/// Default time `up` waits for a service to accept connections.
pub const DEFAULT_READY_TIMEOUT_S: u64 = 60;
/// Maximum stack-file size; configuration should stay cheap to read and parse.
pub const MAX_FILE_BYTES: usize = 1024 * 1024;
/// Maximum number of services managed by one stack.
pub const MAX_SERVICES: usize = 512;
/// Maximum time a service may take to become ready.
pub const MAX_READY_TIMEOUT_S: u64 = 3600;

/// A problem with a `.holdmap.toml`.
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
    #[serde(serialize_with = "crate::redact::serialize_optional_line")]
    pub command: Option<String>,
    /// Absolute working directory.
    pub cwd: PathBuf,
    /// Extra environment variables (`PORT` is added automatically).
    #[serde(serialize_with = "crate::redact::serialize_env")]
    pub env: BTreeMap<String, String>,
    /// Services that must be up first.
    pub depends_on: Vec<String>,
    /// HTTP path probed by `status`, e.g. `/health`.
    pub health: Option<String>,
    /// How long `up` waits for the port to accept connections.
    pub ready_timeout_s: u64,
}

/// Find `.holdmap.toml` (or legacy `.portwise.toml`) in `start` or the nearest ancestor.
pub fn find(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        let preferred = dir.join(FILE_NAME);
        if preferred.is_file() {
            return Some(preferred);
        }
        let legacy = dir.join(LEGACY_FILE_NAME);
        if legacy.is_file() {
            return Some(legacy);
        }
    }
    None
}

/// A stack file runs its commands through the shell, so like git's `safe.directory` only
/// trust one the current user (or root) owns and other users can't write to. This stops a
/// `.holdmap.toml` dropped into a shared parent directory (`/tmp`, a shared checkout) from
/// running someone else's commands as you.
pub fn check_trusted(path: &Path) -> Result<(), StackError> {
    let file = open_stack_file(path)?;
    check_trusted_open(path, &file)
}

fn open_stack_file(path: &Path) -> Result<std::fs::File, StackError> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(not(unix))]
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(StackError::Unsafe {
            path: path.to_path_buf(),
            message: "stack files must not be symbolic links".into(),
        });
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|source| StackError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    #[cfg(windows)]
    crate::util::check_windows_private_file(&file).map_err(|source| StackError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(file)
}

fn check_trusted_open(path: &Path, file: &std::fs::File) -> Result<(), StackError> {
    let m = file.metadata().map_err(|source| StackError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !m.is_file() {
        return Err(StackError::Unsafe {
            path: path.to_path_buf(),
            message: "stack files must be regular files".into(),
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let unsafe_ = |message: String| StackError::Unsafe {
            path: path.to_path_buf(),
            message,
        };
        // SAFETY: geteuid has no preconditions and can't fail.
        let me = unsafe { libc::geteuid() };
        if m.uid() != me && m.uid() != 0 {
            return Err(unsafe_(format!(
                "it belongs to another user (uid {}); holdmap only runs stack files you own",
                m.uid()
            )));
        }
        if m.mode() & 0o022 != 0 {
            return Err(unsafe_(
                "another account can write to it; run `chmod go-w` (including `chmod o-w`) on it first".into(),
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Read and validate a stack file (refusing one that isn't [`check_trusted`]).
pub fn load(path: &Path) -> Result<Stack, StackError> {
    use std::io::Read;
    let file = open_stack_file(path)?;
    check_trusted_open(path, &file)?;
    let mut text = String::new();
    file.take(MAX_FILE_BYTES as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|source| StackError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let root = path
        .parent()
        .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()))
        .unwrap_or_default();
    let mut stack = parse(&text, &root, path)?;
    #[cfg(windows)]
    {
        let paths: Vec<_> = stack
            .services
            .iter()
            .map(|service| service.cwd.clone())
            .collect();
        let resolved = canonicalize_windows_dos_paths(paths);
        for service in &mut stack.services {
            if windows_dos_path(&service.cwd) {
                service.cwd =
                    resolved
                        .get(&service.cwd)
                        .cloned()
                        .ok_or_else(|| StackError::Invalid {
                            path: path.to_path_buf(),
                            message: format!(
                                "could not safely resolve Windows short-name working directory {}",
                                service.cwd.display()
                            ),
                        })?;
            }
        }
    }
    #[cfg(not(windows))]
    let _ = &mut stack;
    Ok(stack)
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
    if text.len() > MAX_FILE_BYTES {
        return Err(invalid(format!(
            "stack files must be at most {MAX_FILE_BYTES} bytes"
        )));
    }
    let raw: RawFile = toml::from_str(text).map_err(|e| StackError::Parse {
        path: file.to_path_buf(),
        message: e.message().to_string(),
    })?;
    if raw.services.len() > MAX_SERVICES {
        return Err(invalid(format!(
            "a stack may define at most {MAX_SERVICES} services"
        )));
    }
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
                "service `{name}` has a command but no port; holdmap needs the port to know when it's up and what to stop"
            )));
        }
        if let Some(h) = &s.health {
            if !crate::http::is_safe_path(h) {
                return Err(invalid(format!(
                    "service `{name}`: health must be a path like `/health`"
                )));
            }
        }
        if s.ready_timeout_s
            .is_some_and(|timeout| timeout == 0 || timeout > MAX_READY_TIMEOUT_S)
        {
            return Err(invalid(format!(
                "service `{name}`: ready_timeout_s must be 1–{MAX_READY_TIMEOUT_S} seconds"
            )));
        }
        for d in &s.depends_on {
            if !raw.services.contains_key(d) {
                return Err(invalid(format!(
                    "service `{name}` depends on unknown service `{d}`"
                )));
            }
        }
        for k in s.env.keys() {
            if k.is_empty() || k.contains(['=', '\0']) || s.env[k].contains('\0') {
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
    let mut remaining: BTreeMap<&str, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut ready = BTreeSet::new();
    for (name, service) in services {
        let dependencies: BTreeSet<&str> = service.depends_on.iter().map(String::as_str).collect();
        remaining.insert(name, dependencies.len());
        if dependencies.is_empty() {
            ready.insert(name.as_str());
        }
        for dependency in dependencies {
            dependents.entry(dependency).or_default().push(name);
        }
    }
    let mut out = Vec::new();
    while let Some(name) = ready.pop_first() {
        out.push(name.to_string());
        for dependent in dependents.get(name).into_iter().flatten() {
            let count = remaining.get_mut(dependent).expect("known dependent");
            *count -= 1;
            if *count == 0 {
                ready.insert(*dependent);
            }
        }
    }
    if out.len() != services.len() {
        let left: Vec<&str> = remaining
            .into_iter()
            .filter_map(|(name, count)| (count != 0).then_some(name))
            .collect();
        return Err(format!(
            "dependency cycle between services: {}",
            left.join(", ")
        ));
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
            path_is_under(p, &self.root) || self.services.iter().any(|s| path_is_under(p, &s.cwd))
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

/// Whether `path` is within `root`, including the root itself. Equivalent ordinary/verbatim
/// Windows drive and UNC prefixes match without filesystem IO; component names and directory
/// boundaries remain exact, including for Windows case-sensitive directories.
pub fn path_is_under(path: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    return windows_relative_components(path, root).is_some();
    #[cfg(not(windows))]
    path.starts_with(root)
}

/// The portion of `path` below `root`, or `None` when outside it. This uses the same
/// namespace-aware Windows comparison as [`path_is_under`] and performs no filesystem IO.
pub fn relative_to_root(path: &Path, root: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    return windows_relative_components(path, root).map(|parts| parts.as_path().to_path_buf());
    #[cfg(not(windows))]
    path.strip_prefix(root).ok().map(Path::to_path_buf)
}

#[cfg(windows)]
fn windows_relative_components<'a>(
    path: &'a Path,
    root: &Path,
) -> Option<std::path::Components<'a>> {
    use std::path::{Component, Prefix};
    let mut path = path.components();
    for root in root.components() {
        let matches = match (path.next(), root) {
            (Some(Component::Prefix(path)), Component::Prefix(root)) => {
                match (path.kind(), root.kind()) {
                    (
                        Prefix::Disk(a) | Prefix::VerbatimDisk(a),
                        Prefix::Disk(b) | Prefix::VerbatimDisk(b),
                    ) => a.eq_ignore_ascii_case(&b),
                    (
                        Prefix::UNC(a_server, a_share) | Prefix::VerbatimUNC(a_server, a_share),
                        Prefix::UNC(b_server, b_share) | Prefix::VerbatimUNC(b_server, b_share),
                    ) => a_server == b_server && a_share == b_share,
                    (a, b) => a == b,
                }
            }
            (Some(path), root) => path == root,
            (None, _) => false,
        };
        if !matches {
            return None;
        }
    }
    Some(path)
}

#[cfg(windows)]
pub(crate) fn windows_dos_path(path: &Path) -> bool {
    path.is_absolute()
        && path.components().any(|part| {
            let std::path::Component::Normal(name) = part else {
                return false;
            };
            let name = name.to_string_lossy();
            name.split_once('~').is_some_and(|(_, suffix)| {
                let number = suffix.split('.').next().unwrap_or_default();
                !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
}

/// Canonicalize unique DOS short-name paths under one operation budget, not one deadline per
/// service/process. Stalled OS calls cannot accumulate across scans; missing results fail closed.
#[cfg(windows)]
pub(crate) fn canonicalize_windows_dos_paths(
    paths: impl IntoIterator<Item = PathBuf>,
) -> std::collections::HashMap<PathBuf, PathBuf> {
    static ACTIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    resolve_windows_dos_paths(
        paths,
        std::time::Duration::from_millis(250),
        &ACTIVE,
        std::sync::Arc::new(|path| std::fs::canonicalize(path).ok()),
    )
}

#[cfg(windows)]
type WindowsPathResolver = std::sync::Arc<dyn Fn(&Path) -> Option<PathBuf> + Send + Sync>;

#[cfg(windows)]
fn resolve_windows_dos_paths(
    paths: impl IntoIterator<Item = PathBuf>,
    budget: std::time::Duration,
    active: &'static std::sync::atomic::AtomicUsize,
    resolve: WindowsPathResolver,
) -> std::collections::HashMap<PathBuf, PathBuf> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    const MAX_ACTIVE: usize = 4;
    struct Slot(&'static AtomicUsize);
    impl Drop for Slot {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let mut paths = paths
        .into_iter()
        .filter(|path| windows_dos_path(path))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .peekable();
    let mut resolved = std::collections::HashMap::new();
    let (send, receive) = std::sync::mpsc::sync_channel(MAX_ACTIVE);
    let deadline = std::time::Instant::now() + budget;
    let mut pending = 0;
    loop {
        while pending < MAX_ACTIVE && paths.peek().is_some() && std::time::Instant::now() < deadline
        {
            let mut count = active.load(Ordering::Acquire);
            let acquired = loop {
                if count >= MAX_ACTIVE {
                    break false;
                }
                match active.compare_exchange_weak(
                    count,
                    count + 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break true,
                    Err(current) => count = current,
                }
            };
            if !acquired {
                break;
            }
            let slot = Slot(active);
            let path = paths.next().expect("checked pending path");
            let send = send.clone();
            let resolve = resolve.clone();
            let worker = std::thread::Builder::new()
                .name("holdmap-path-alias".into())
                .spawn(move || {
                    let _slot = slot;
                    let canonical = resolve(&path);
                    let _ = send.send((path, canonical));
                });
            if worker.is_ok() {
                pending += 1;
            }
        }
        if pending == 0 {
            break;
        }
        let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
            break;
        };
        let Ok((path, canonical)) = receive.recv_timeout(remaining) else {
            break;
        };
        pending -= 1;
        if let Some(canonical) = canonical {
            resolved.insert(path, canonical);
        }
    }
    resolved
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

/// A starter `.holdmap.toml` for `services` (name, port, command, cwd relative to the root).
/// With no services it returns a commented example.
pub fn scaffold(name: &str, services: &[(String, u16, Option<String>, Option<String>)]) -> String {
    let mut s = format!(
        "# holdmap project file: `holdmap up`, `holdmap down`, `holdmap status`.\n\
         name = {}\n\
         # protect = [5432]   # ports holdmap must never stop\n",
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
            Path::new("/src/shop/.holdmap.toml"),
        )
        .unwrap()
    }

    #[test]
    fn security_stack_exports_redact_commands_and_environment_without_changing_launch_data() {
        let stack = parse("[services.web]\nport=3000\ncommand=\"node app.js --password 'fixture secret'\"\nenv={API_TOKEN=\"fixture-token\",DATABASE_URL=\"postgres://u:fixture-password@localhost/db\",MODE=\"dev\"}\n", Path::new("/tmp/fixture"), Path::new("/tmp/fixture/.holdmap.toml")).unwrap();
        let service = &stack.services[0];
        assert!(service.command.as_ref().unwrap().contains("fixture secret"));
        assert_eq!(service.env["API_TOKEN"], "fixture-token");
        let json = serde_json::to_string(&stack).unwrap();
        for secret in ["fixture secret", "fixture-token", "fixture-password"] {
            assert!(!json.contains(secret), "{json}");
        }
        assert!(json.contains("dev"));
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
            parse(t, Path::new("/r"), Path::new("/r/.holdmap.toml"))
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
        assert!(bad("port = ").contains(".holdmap.toml"));
        assert!(bad("[services.a]\nport = 70000").contains(".holdmap.toml"));
        assert!(bad("[services.a]\nport = 1\nready_timeout_s = 0").contains("1–3600"));
        assert!(
            bad("[services.a]\nport = 1\nready_timeout_s = 9223372036854775807").contains("1–3600")
        );
    }

    #[test]
    fn stack_input_and_service_counts_are_bounded() {
        let root = Path::new("/r");
        let file = Path::new("/r/.holdmap.toml");
        let oversized = format!("#{}", "x".repeat(MAX_FILE_BYTES));
        assert!(matches!(
            parse(&oversized, root, file),
            Err(StackError::Invalid { .. })
        ));
        let services: String = (0..=MAX_SERVICES)
            .map(|i| format!("[services.s{i}]\n"))
            .collect();
        assert!(matches!(
            parse(&services, root, file),
            Err(StackError::Invalid { .. })
        ));
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE_NAME);
        std::fs::write(&file, oversized).unwrap();
        assert!(matches!(load(&file), Err(StackError::Invalid { .. })));
    }

    #[test]
    fn dependency_order_handles_large_chains_and_duplicate_dependencies() {
        let mut text = String::new();
        for i in (0..MAX_SERVICES).rev() {
            text.push_str(&format!("[services.s{i:03}]\n"));
            if i > 0 {
                text.push_str(&format!(
                    "depends_on = ['s{:03}', 's{:03}']\n",
                    i - 1,
                    i - 1
                ));
            }
        }
        let stack = parse(&text, Path::new("/r"), Path::new("f")).unwrap();
        assert_eq!(stack.services.len(), MAX_SERVICES);
        for (index, service) in stack.services.iter().enumerate() {
            assert_eq!(service.name, format!("s{index:03}"));
        }
        let selected = stack
            .start_order(&[format!("s{:03}", MAX_SERVICES - 1)])
            .unwrap();
        assert_eq!(selected.len(), MAX_SERVICES);
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

    #[cfg(unix)]
    #[test]
    fn security_stack_rejects_group_writable_and_symlinked_commands() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE_NAME);
        std::fs::write(&file, "[services.web]\nport=3000\ncommand='echo fixture'\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o660)).unwrap();
        assert!(matches!(load(&file), Err(StackError::Unsafe { .. })));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.path().join("linked.toml");
        symlink(&file, &link).unwrap();
        assert!(load(&link).is_err());
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
            app_memory_bytes: 0,
            helper_count: 0,
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

    #[cfg(windows)]
    #[test]
    fn windows_ownership_matches_ordinary_and_verbatim_drive_and_unc_prefixes() {
        for (root, cwd, sibling) in [
            (
                r"\\?\C:\workspace\shop",
                r"C:\workspace\shop\apps\web",
                r"C:\workspace\shop-other\web",
            ),
            (
                r"C:\workspace\shop",
                r"\\?\c:\workspace\shop\apps\web",
                r"\\?\C:\workspace\shop-other\web",
            ),
            (
                r"\\?\UNC\server\share\shop",
                r"\\server\share\shop\web",
                r"\\server\other\shop\web",
            ),
        ] {
            let stack = parse(
                "[services.web]\nport=3000\ncommand='fixture'\n",
                Path::new(root),
                &Path::new(root).join(FILE_NAME),
            )
            .unwrap();
            assert!(stack.owns(&entry(3000, Some(cwd), None)), "{root} / {cwd}");
            let relative = relative_to_root(Path::new(cwd), Path::new(root)).unwrap();
            assert_eq!(
                relative,
                Path::new(if cwd.ends_with(r"apps\web") {
                    r"apps\web"
                } else {
                    "web"
                })
            );
            assert!(
                !stack.owns(&entry(3000, Some(sibling), None)),
                "{root} / {sibling}"
            );
            assert!(relative_to_root(Path::new(sibling), Path::new(root)).is_none());
            let snap = Snapshot {
                entries: vec![entry(3000, Some(cwd), None)],
                hidden_sockets: 0,
                platform: "windows".into(),
                taken_at_ms: 0,
                scan_ms: 0,
                docker_available: false,
                warnings: vec![],
            };
            assert_eq!(stack.status(&snap)[0].state, ServiceState::Running);
        }
        // The namespace correction must not broaden ownership to different filesystem
        // names, devices, shares or roots that merely have the same textual prefix.
        assert!(!path_is_under(
            Path::new(r"C:\workspace\Shop\web"),
            Path::new(r"\\?\C:\workspace\shop")
        ));
        assert!(!path_is_under(
            Path::new(r"D:\workspace\shop"),
            Path::new(r"\\?\C:\workspace\shop")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn a_loaded_windows_stack_owns_an_existing_cwd_without_a_verbatim_prefix() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join(FILE_NAME);
        std::fs::write(&file, "[services.web]\nport=3000\ncommand='fixture'\n").unwrap();
        let stack = load(&file).unwrap();
        let canonical = std::fs::canonicalize(directory.path()).unwrap();
        let ordinary = canonical.to_str().unwrap().strip_prefix(r"\\?\").unwrap();
        assert!(stack.owns(&entry(3000, Some(ordinary), None)));
    }

    #[cfg(windows)]
    #[test]
    fn windows_short_name_cwds_resolve_once_and_match_loaded_stack_ownership() {
        let directory = tempfile::tempdir().unwrap();
        let cwd = directory.path().join("apps/work space");
        std::fs::create_dir_all(&cwd).unwrap();
        let file = directory.path().join(FILE_NAME);
        let service_cwd = cwd.to_string_lossy();
        std::fs::write(
            &file,
            format!(
                "[services.web]\nport=3000\ncommand='fixture'\ncwd={}\n",
                toml_str(&service_cwd)
            ),
        )
        .unwrap();
        let stack = load(&file).unwrap();
        let resolved = canonicalize_windows_dos_paths([cwd.clone(), cwd.clone()]);
        let cwd = if windows_dos_path(&cwd) {
            assert_eq!(
                resolved.len(),
                1,
                "one resolution for a shared DOS alias cwd"
            );
            resolved.get(&cwd).unwrap().clone()
        } else {
            cwd
        };
        assert!(stack.owns(&entry(3000, cwd.to_str(), None)));
        assert_eq!(
            relative_to_root(&cwd, &stack.root),
            Some(PathBuf::from(r"apps\work space"))
        );
        assert_eq!(
            std::fs::canonicalize(&stack.services[0].cwd).unwrap(),
            std::fs::canonicalize(&cwd).unwrap()
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_alias_batch_deadline_caps_workers_discards_late_results_and_recovers() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Condvar, Mutex};
        use std::time::{Duration, Instant};
        static ACTIVE: AtomicUsize = AtomicUsize::new(0);
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        struct Release(Arc<(Mutex<bool>, Condvar)>);
        impl Drop for Release {
            fn drop(&mut self) {
                *self.0 .0.lock().unwrap() = true;
                self.0 .1.notify_all();
            }
        }
        let release = Release(gate.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let resolve = {
            let gate = gate.clone();
            let calls = calls.clone();
            Arc::new(move |path: &Path| {
                calls.fetch_add(1, Ordering::AcqRel);
                let mut released = gate.0.lock().unwrap();
                while !*released {
                    released = gate.1.wait(released).unwrap();
                }
                Some(path.to_path_buf())
            })
        };
        let paths: Vec<_> = (0..8)
            .map(|i| PathBuf::from(format!(r"C:\RUNNER~1\owned-{i}")))
            .collect();
        let started = Instant::now();
        assert!(resolve_windows_dos_paths(
            paths.clone(),
            Duration::from_millis(50),
            &ACTIVE,
            resolve.clone()
        )
        .is_empty());
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "one operation deadline, not eight path deadlines"
        );
        assert_eq!(ACTIVE.load(Ordering::Acquire), 4);
        assert!(resolve_windows_dos_paths(
            paths.clone(),
            Duration::from_millis(50),
            &ACTIVE,
            resolve
        )
        .is_empty());
        assert_eq!(
            ACTIVE.load(Ordering::Acquire),
            4,
            "stalled workers cannot accumulate across captures"
        );
        drop(release);
        let deadline = Instant::now() + Duration::from_secs(1);
        while ACTIVE.load(Ordering::Acquire) != 0 {
            assert!(
                Instant::now() < deadline,
                "late workers must release capacity"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(calls.load(Ordering::Acquire), 4);
        let resolved = resolve_windows_dos_paths(
            paths.clone(),
            Duration::from_secs(1),
            &ACTIVE,
            Arc::new(|path| Some(path.to_path_buf())),
        );
        assert_eq!(
            resolved.len(),
            paths.len(),
            "late results belong only to their discarded receiver"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_alias_batch_deduplicates_paths_without_querying_normal_names() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        use std::time::Duration;
        static ACTIVE: AtomicUsize = AtomicUsize::new(0);
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let alias = PathBuf::from(r"C:\RUNNER~1\owned");
        let paths = [
            alias.clone(),
            alias.clone(),
            PathBuf::from(r"C:\ordinary\owned"),
        ];
        let resolved = resolve_windows_dos_paths(
            paths,
            Duration::from_secs(1),
            &ACTIVE,
            Arc::new(move |path| {
                counted.fetch_add(1, Ordering::AcqRel);
                Some(path.to_path_buf())
            }),
        );
        assert_eq!(calls.load(Ordering::Acquire), 1);
        assert_eq!(resolved.len(), 1);
        assert!(resolved.contains_key(&alias));
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
