//! The reader's walk over the shared location registry.
//!
//! The census and this reader share one table per product
//! ([`crate::transparency::locations`]). Rows with a reader are parsed here, in three passes so
//! MCP definitions are collected before skill metadata can use up the shared read budget.

use super::{CapabilityScope, Inventory};
use crate::transparency::locations::{self, Homes, LocationSpec, Reader};
use crate::transparency::PlaceScope;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Mcp,
    Skills,
    Other,
}

impl Pass {
    fn takes(self, reader: Reader) -> bool {
        match reader {
            Reader::None => false,
            Reader::Mcp(_) => self == Self::Mcp,
            Reader::Skills => self == Self::Skills,
            Reader::Policy | Reader::Instructions(..) => self == Self::Other,
        }
    }
}

/// The reader's scope for a registry layer. Machine-wide layers read as account-wide.
pub(super) fn reader_scope(scope: PlaceScope) -> CapabilityScope {
    match scope {
        PlaceScope::Project | PlaceScope::Local => CapabilityScope::Project,
        _ => CapabilityScope::User,
    }
}

impl Inventory<'_> {
    /// Parse every readable registry row for this product: account rows, then each project.
    pub(super) fn read_registry(&mut self, homes: &Homes) {
        let specs = locations::readable(self.product, self.platform);
        let projects = self.projects.clone();
        for pass in [Pass::Mcp, Pass::Skills, Pass::Other] {
            for spec in specs.iter().filter(|spec| pass.takes(spec.reader)) {
                if !spec.per_project() {
                    self.read_spec(spec, homes, None);
                }
            }
            for project in &projects {
                for spec in specs.iter().filter(|spec| pass.takes(spec.reader)) {
                    if spec.per_project() {
                        self.read_spec(spec, homes, Some(project));
                    }
                }
            }
        }
    }

    fn read_spec(&mut self, spec: &LocationSpec, homes: &Homes, project: Option<&Path>) {
        let Some(path) = locations::resolve(spec, homes, project) else {
            return;
        };
        let scope = reader_scope(spec.scope);
        match spec.reader {
            Reader::None => {}
            Reader::Mcp(format) => self.config(&path, scope, project, format),
            Reader::Skills => self.skills(&path, scope, project, 0),
            Reader::Policy => self.policy_file(&path, scope),
            Reader::Instructions(kind, None) => self.memory_file(&path, kind, scope),
            Reader::Instructions(kind, Some(extension)) => {
                self.instruction_directory(&path, kind, extension, scope)
            }
        }
    }
}
