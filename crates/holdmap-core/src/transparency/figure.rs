//! An integer counter that is either observed or explicitly unknown.
//!
//! A figure never substitutes one operating-system counter for another. Callers add figures
//! only when every input was observed; a partial sum is not reported.

use serde::{Deserialize, Serialize};

/// One integer observation. Exactly one of [`Self::value`] and [`Self::unknown`] is set by
/// the constructors. A deserialized figure that breaks that rule is treated as unknown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedU64 {
    /// The counter, when the platform call returned it.
    #[serde(default)]
    pub value: Option<u64>,
    /// Why [`Self::value`] is absent. Empty when the value was observed.
    #[serde(default)]
    pub unknown: Option<String>,
}

impl ObservedU64 {
    /// A counter the platform returned, including a genuine zero.
    pub fn observed(value: u64) -> Self {
        Self {
            value: Some(value),
            unknown: None,
        }
    }

    /// The platform did not return this counter.
    pub fn unknown(reason: impl Into<String>) -> Self {
        Self {
            value: None,
            unknown: Some(reason.into()),
        }
    }

    /// The observed integer, only when a reason was not also recorded.
    pub fn get(&self) -> Option<u64> {
        match (&self.value, &self.unknown) {
            (Some(value), None) => Some(*value),
            _ => None,
        }
    }

    /// Text for a byte counter: a size, or `unknown` plus the recorded reason.
    pub fn bytes_text(&self) -> String {
        match self.get() {
            Some(value) => crate::util::human_bytes(value),
            None => format!("unknown ({})", self.reason()),
        }
    }

    /// The recorded reason, or a fixed sentence when a figure is internally inconsistent.
    pub fn reason(&self) -> &str {
        match (&self.value, &self.unknown) {
            (None, Some(reason)) => reason,
            (Some(_), None) => "",
            (Some(_), Some(_)) => {
                "the sample recorded both a value and an unknown reason, so it was not used"
            }
            (None, None) => "no value and no reason were recorded",
        }
    }
}

/// Sum every figure, or return unknown when any input was not observed.
pub fn sum_observed(items: &[ObservedU64], empty: &str) -> ObservedU64 {
    if items.is_empty() {
        return ObservedU64::unknown(empty);
    }
    let mut total = 0u64;
    for item in items {
        match item.get() {
            Some(value) => total = total.saturating_add(value),
            None => return ObservedU64::unknown(item.reason()),
        }
    }
    ObservedU64::observed(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contradictory_figures_are_not_treated_as_numbers() {
        let bad = ObservedU64 {
            value: Some(5),
            unknown: Some("no".into()),
        };
        assert_eq!(bad.get(), None);
        assert!(bad.bytes_text().starts_with("unknown"));
    }

    #[test]
    fn a_missing_input_blocks_the_sum() {
        let items = [
            ObservedU64::observed(10),
            ObservedU64::unknown("smaps_rollup was unreadable"),
        ];
        let sum = sum_observed(&items, "empty");
        assert_eq!(sum.get(), None);
        assert_eq!(sum.reason(), "smaps_rollup was unreadable");
        assert_eq!(sum_observed(&[], "no processes").reason(), "no processes");
    }
}
