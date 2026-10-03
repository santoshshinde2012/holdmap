//! Service topology: which local services talk to which (the "mesh"), grouped into clusters.
//!
//! - **Nodes** are services: a process tree (keyed by its launcher root, e.g. `npm run dev`), a
//!   container, a hidden listener, a client-only process, or the collapsed *external* node.
//! - **Edges** are established TCP connections, aggregated per (from, to, port).
//!   `a → b` (local) means *a depends on b*.
//! - **Clusters** come from [`ClusterDetector`]s: compose project, Kubernetes namespace,
//!   supervisor (pm2/turbo/concurrently/nx…), workspace root, git repo.
//!
//! Built by [`TopologyBuilder`] from a [`Scan`](crate::scan::Scan); exported by a
//! [`GraphExporter`]; [`stop_order`] drives dependency-aware cluster stops.

mod builder;
pub mod cluster;
pub mod export;
mod model;
mod order;

pub use builder::TopologyBuilder;
pub use cluster::{ClusterDetector, ClusterRegistry};
pub use export::{
    exporter, DotExporter, GraphExporter, JsonExporter, MermaidExporter, PlainStyle, TreeExporter,
    TreeStyle,
};
pub use model::*;
pub use order::{stop_order, StopOrder};

#[cfg(test)]
mod proptests;
#[cfg(test)]
pub(crate) mod tests;
