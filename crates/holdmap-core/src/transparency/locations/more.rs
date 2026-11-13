//! Agents whose locations are listed but whose configuration is not parsed.

use super::LocationSpec as L;

/// Rows for products without a dedicated table.
pub(super) fn specs(_product: &str, _platform: &str) -> Vec<L> {
    Vec::new()
}
