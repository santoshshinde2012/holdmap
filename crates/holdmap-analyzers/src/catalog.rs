//! The first-party analyzer catalog. In v1 it is compiled into holdmap, so it is covered by
//! the release's signature and attestation; nothing is fetched.

use crate::manifest::AnalyzerManifest;

const BUNDLED: &[&str] = &[
    include_str!("../manifests/holdmap.osv.toml"),
    include_str!("../manifests/nvidia.skillspector.toml"),
];

/// Every bundled manifest, parsed. A bundled manifest that fails validation is a build bug.
pub fn bundled() -> Vec<AnalyzerManifest> {
    BUNDLED
        .iter()
        .map(|text| AnalyzerManifest::parse_bundled(text).expect("bundled manifest is valid"))
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_manifests_parse_with_unique_ids() {
        let all = super::bundled();
        assert_eq!(all.len(), 2);
        assert_ne!(all[0].id, all[1].id);
        assert!(all.iter().all(|m| !m.permissions.spawns_mcp_servers));
    }
}
