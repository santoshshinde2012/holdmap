//! AI coding agents and developer tools on this machine, and their footprint.
//!
//! Built from one [`crate::scan::Scan`] like the [topology](crate::topology): each process is
//! matched against the [`catalog`]; the topmost process of a product is an agent or developer
//! tool. Everything started under it (helpers, shells, dev servers, MCP servers) belongs to it,
//! up to the next agent. From that the [`AgentsBuilder`] derives:
//!
//! * **folders**: working directories of the agent and its processes, plus recent projects
//!   from the [`recent`] sources that are safe to read;
//! * **ports**: listening sockets held by the agent or anything it started;
//! * **links**: live TCP connections, grouped by local service or remote host (by IP only);
//! * **access**: account, sandbox and approval hints, network exposure and macOS privacy areas
//!   (see [`access`]), each marked observed, inferred or unknown.
//!
//! Nothing here reads chats, settings that may hold tokens, or files in privacy-protected
//! folders.

pub mod access;
mod builder;
pub mod catalog;
mod footprint;
mod model;
mod network;
mod ownership;
pub mod recent;
pub mod tools;

pub use builder::AgentsBuilder;
pub use catalog::{identify, AgentDetector, AgentKind, AgentProduct, CatalogDetector, CATALOG};
pub use model::*;
pub use recent::{HomeRecent, NoRecent, RecentProjects};
pub use tools::{DefaultToolClassifier, ToolClassification, ToolClassifier};

/// Processes listed per agent.
pub const MAX_PROCESSES: usize = 40;
/// Working folders listed per agent, before recent projects.
pub const MAX_FOLDERS: usize = 12;
/// Link groups listed per agent.
pub const MAX_LINKS: usize = 16;
/// Tool processes listed per agent.
pub const MAX_TOOLS: usize = 24;

#[cfg(test)]
mod tests;
