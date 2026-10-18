//! Signed catalog updates: verification, staging, the review diff, acceptance and the pins
//! that make a changed analyzer ask again. Uses a throwaway test key, never holdmap's.

use holdmap_analyzers::catalog::{
    self, render, trusted_comment, CatalogError, CatalogSlot, CatalogSource, CatalogStore,
    SignedCatalog, SOURCES,
};
use holdmap_analyzers::probe::{BinaryInfo, BinaryProbe};
use holdmap_analyzers::runner::{AnalyzerRunner, RawOutput, RunContext, RunError};
use holdmap_analyzers::service::ServiceError;
use holdmap_analyzers::{
    AnalyzerInput, AnalyzerManifest, AnalyzerService, AnalyzerState, MemoryStore, Stores,
};
use std::sync::Arc;

#[path = "../examples/catalog_key/mod.rs"]
mod catalog_key;

struct Key {
    sk: minisign::SecretKey,
    pk: String,
}

fn key() -> Key {
    let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    Key {
        pk: pair.pk.to_box().unwrap().to_string(),
        sk: pair.sk,
    }
}

fn sign(key: &Key, version: u32, manifests: &[&str]) -> SignedCatalog {
    let json = render(version, "2026-11-01", manifests);
    let sig = minisign::sign(
        None,
        &key.sk,
        json.as_bytes(),
        Some(&trusted_comment(version)),
        None,
    )
    .unwrap();
    SignedCatalog {
        json,
        signature: sig.to_string(),
    }
}

struct Probe;
impl BinaryProbe for Probe {
    fn locate(&self, _: &AnalyzerManifest) -> Option<BinaryInfo> {
        Some(BinaryInfo {
            path: "/opt/bin/skillspector".into(),
            sha256: "a".repeat(64),
        })
    }
    fn version(&self, _: &AnalyzerManifest, _: &std::path::Path) -> Option<String> {
        Some("2.13.0".into())
    }
}

struct NoRun;
impl AnalyzerRunner for NoRun {
    fn run(
        &self,
        _: &AnalyzerManifest,
        _: &AnalyzerInput,
        _: &RunContext,
    ) -> Result<Vec<RawOutput>, RunError> {
        Err(RunError::Unavailable("not in this test".into()))
    }
}

struct Source(SignedCatalog);
impl CatalogSource for Source {
    fn fetch(&self) -> Result<SignedCatalog, String> {
        Ok(self.0.clone())
    }
}

fn service(key: &Key, store: Arc<MemoryStore>, fetched: SignedCatalog) -> AnalyzerService {
    let stores = Stores {
        consent: store.clone(),
        results: store.clone(),
        reviews: store.clone(),
        secrets: store.clone(),
        catalog: store.clone(),
        manifests: store,
    };
    AnalyzerService::with_parts(
        catalog::bundled(),
        stores,
        Box::new(Probe),
        Box::new(NoRun),
        Box::new(NoRun),
    )
    .with_catalog_source(&key.pk, Box::new(Source(fetched)))
}

/// SkillSpector with a longer time limit, and a new third-party CLI analyzer.
fn update() -> Vec<String> {
    let skillspector = SOURCES[0].replace("timeout_secs = 120", "timeout_secs = 300");
    assert_ne!(skillspector, SOURCES[0]);
    let extra = SOURCES[0]
        .replace("id = \"nvidia.skillspector\"", "id = \"example.scanner\"")
        .replace("name = \"SkillSpector\"", "name = \"Example Scanner\"");
    vec![skillspector, SOURCES[1].to_string(), extra]
}

#[test]
fn a_newer_signed_catalog_is_staged_reviewed_and_only_then_applied() {
    let key = key();
    let store = Arc::new(MemoryStore::default());
    let texts = update();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let svc = service(&key, store.clone(), sign(&key, 2, &refs));
    svc.approve("nvidia.skillspector").unwrap();

    let before = svc.catalog_status();
    assert_eq!(
        (before.version, before.accepted, before.pending.is_none()),
        (1, false, true)
    );

    let diff = svc.fetch_catalog().unwrap();
    assert_eq!((diff.from_version, diff.to_version), (1, 2));
    assert_eq!(diff.added, ["Example Scanner (example.scanner)"]);
    assert!(diff.removed.is_empty());
    assert_eq!(diff.changed.len(), 1);
    let change = &diff.changed[0];
    assert_eq!(change.id, "nvidia.skillspector");
    assert_eq!(change.fields.len(), 1, "{:?}", change.fields);
    assert_eq!(change.fields[0].field, "runner.timeout_secs");
    assert_eq!(
        (
            change.fields[0].before.as_str(),
            change.fields[0].after.as_str()
        ),
        ("120", "300")
    );

    // Staged only: nothing changed yet.
    assert_eq!(svc.list().len(), 2);
    assert_eq!(svc.catalog_status().pending, Some(diff));
    assert_eq!(
        svc.get("nvidia.skillspector").unwrap().state,
        AnalyzerState::Approved
    );

    let after = svc.accept_catalog().unwrap();
    assert_eq!(
        (after.version, after.accepted, after.pending),
        (2, true, None)
    );
    assert_eq!(svc.list().len(), 3);
    let view = svc.get("nvidia.skillspector").unwrap();
    assert_eq!(
        view.state,
        AnalyzerState::Changed,
        "a changed manifest asks again"
    );
    assert_eq!(
        svc.get("example.scanner").unwrap().state,
        AnalyzerState::NotApproved
    );

    // The accepted copy is what a restart loads.
    let (active, accepted) = catalog::load_active(store.as_ref(), &key.pk);
    assert!(accepted);
    assert_eq!(active.version, 2);
}

#[test]
fn tampered_foreign_and_older_catalogs_are_refused() {
    let key = key();
    let store = Arc::new(MemoryStore::default());
    let svc = service(&key, store.clone(), sign(&key, 1, SOURCES));

    // Same version as the one in use.
    assert!(matches!(
        svc.fetch_catalog(),
        Err(ServiceError::Catalog(CatalogError::NotNewer {
            have: 1,
            got: 1
        }))
    ));
    // Signed by another key (holdmap's real bundled catalog against the test key).
    let mut bundled = SignedCatalog::bundled();
    assert!(matches!(
        svc.stage_catalog(&bundled),
        Err(ServiceError::Catalog(CatalogError::Signature))
    ));
    // Edited after signing.
    let mut good = sign(&key, 3, SOURCES);
    good.json = good.json.replace("api.osv.dev", "evil.example");
    assert!(matches!(
        svc.stage_catalog(&good),
        Err(ServiceError::Catalog(CatalogError::Signature))
    ));
    // A signature for another version can't be replayed onto this file.
    let v3 = sign(&key, 3, SOURCES);
    bundled.json = render(4, "2026-11-01", SOURCES);
    bundled.signature = v3.signature;
    assert!(matches!(
        svc.stage_catalog(&bundled),
        Err(ServiceError::Catalog(CatalogError::Signature))
    ));
    assert!(store.catalog(CatalogSlot::Pending).is_none());
    assert!(svc.accept_catalog().is_err(), "nothing to accept");
}

#[test]
fn a_catalog_cannot_invent_built_ins_or_list_an_analyzer_twice() {
    let key = key();
    let svc = service(
        &key,
        Arc::new(MemoryStore::default()),
        sign(&key, 1, SOURCES),
    );
    let fake_builtin = SOURCES[1].replace("id = \"holdmap.osv\"", "id = \"evil.builtin\"");
    let r = svc.stage_catalog(&sign(&key, 2, &[SOURCES[0], &fake_builtin]));
    assert!(
        matches!(r, Err(ServiceError::Catalog(CatalogError::Malformed(_)))),
        "{r:?}"
    );
    let r = svc.stage_catalog(&sign(&key, 2, &[SOURCES[0], SOURCES[0]]));
    assert!(
        matches!(r, Err(ServiceError::Catalog(CatalogError::Malformed(_)))),
        "{r:?}"
    );
}

#[test]
fn rejecting_discards_the_staged_update() {
    let key = key();
    let texts = update();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let svc = service(&key, Arc::new(MemoryStore::default()), sign(&key, 2, &refs));
    svc.fetch_catalog().unwrap();
    svc.reject_catalog().unwrap();
    assert!(svc.catalog_status().pending.is_none());
    assert_eq!(svc.list().len(), 2);
}

/// The minisign CLI writes unencrypted (`-W`) keys with an unset checksum; the signing tool
/// must still read them, and keys from the crate, and refuse anything else.
#[test]
fn the_signing_tool_reads_minisign_cli_and_crate_keys() {
    use ct_codecs::{Base64, Decoder, Encoder};
    let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let pk = pair.pk.to_box().unwrap().to_string();
    let crate_file = pair.sk.to_box(None).unwrap().to_string();
    let line = crate_file.lines().nth(1).unwrap();
    let mut bytes = Base64::decode_to_vec(line, None).unwrap();
    let n = bytes.len();
    bytes[n - 32..].fill(0); // what `minisign -G -W` writes
    let cli_file = format!(
        "untrusted comment: minisign secret key\n{}\n",
        Base64::encode_to_string(&bytes).unwrap()
    );
    for file in [crate_file.as_str(), cli_file.as_str()] {
        let sk = catalog_key::load(file).unwrap();
        let json = render(1, "2026-10-18", SOURCES);
        let sig =
            minisign::sign(None, &sk, json.as_bytes(), Some(&trusted_comment(1)), None).unwrap();
        let signed = SignedCatalog {
            json,
            signature: sig.to_string(),
        };
        assert!(catalog::verify(&signed, &pk).is_ok());
    }
    assert!(catalog_key::load("untrusted comment: x\nnot-base64!\n").is_err());
    assert!(
        catalog_key::load(&pk).is_err(),
        "a public key is not a secret key"
    );
}
