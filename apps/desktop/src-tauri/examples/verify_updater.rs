//! Verify updater artifacts and their signed release version before publishing assets.

use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
struct Asset {
    name: String,
    path: String,
    digest: String,
}

#[derive(Clone, Serialize)]
struct Platform {
    name: String,
    digest: String,
    signature: String,
}

#[derive(Serialize)]
struct Manifest {
    matrix: String,
    version: String,
    platforms: BTreeMap<String, Platform>,
}

fn invalid(message: &str) -> Box<dyn std::error::Error> {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}

fn read_small(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("release metadata exceeds its size limit"));
    }
    Ok(bytes)
}

fn signed_version(signature: &Signature, version: &str) -> Result<()> {
    let versions: Vec<_> = signature
        .trusted_comment()
        .split('\t')
        .filter_map(|field| field.strip_prefix("version:"))
        .collect();
    if versions.len() != 1 {
        return Err(invalid("updater signature must carry one signed version"));
    }
    if semver::Version::parse(versions[0].trim_start_matches('v'))?
        != semver::Version::parse(version.trim_start_matches('v'))?
    {
        return Err(invalid(
            "updater signature version differs from the release",
        ));
    }
    Ok(())
}

fn verify_artifact(
    path: &Path,
    encoded_signature: &str,
    key: &PublicKey,
    version: &str,
) -> Result<String> {
    let decoded = STANDARD.decode(encoded_signature.trim())?;
    let signature = Signature::decode(std::str::from_utf8(&decoded)?)?;
    // Current Tauri signs prehashed artifacts. Stream them so verification does not
    // allocate an installer-sized buffer. This also verifies the trusted comment.
    let mut verifier = key.verify_stream(&signature)?;
    let mut digest = Sha256::new();
    let mut file = File::open(path)?;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        verifier.update(&buffer[..count]);
        digest.update(&buffer[..count]);
    }
    verifier.finalize()?;
    // The comment is authenticated only after both minisign signatures pass.
    signed_version(&signature, version)?;
    Ok(format!("{:x}", digest.finalize()))
}

fn installer(name: &str) -> Option<(&'static str, u8)> {
    [
        (".app.tar.gz", "app", 100),
        (".AppImage", "appimage", 100),
        (".AppImage.tar.gz", "appimage", 99),
        (".exe", "nsis", 100),
        (".nsis.zip", "nsis", 99),
        (".msi", "msi", 98),
        (".msi.zip", "msi", 97),
        (".deb", "deb", 90),
        (".rpm", "rpm", 89),
    ]
    .into_iter()
    .find_map(|(suffix, kind, priority)| name.ends_with(suffix).then_some((kind, priority)))
}

fn verify_manifest(input: &Path, matrix: &str, version: &str, key: &PublicKey) -> Result<Manifest> {
    let target = match matrix {
        "macos-arm64" => "darwin-aarch64",
        "macos-x64" => "darwin-x86_64",
        "linux-x64" => "linux-x86_64",
        "windows-x64" => "windows-x86_64",
        _ => return Err(invalid("unknown release matrix")),
    };
    semver::Version::parse(version)?;
    let assets: Vec<Asset> = serde_json::from_slice(&read_small(input, 1024 * 1024)?)?;
    if assets.is_empty() || assets.len() > 64 {
        return Err(invalid("invalid release asset count"));
    }
    let mut by_name = HashMap::new();
    for asset in &assets {
        if Path::new(&asset.path).file_name().and_then(|s| s.to_str()) != Some(asset.name.as_str())
            || !asset.digest.bytes().all(|b| b.is_ascii_hexdigit())
            || asset.digest.len() != 64
            || by_name.insert(asset.name.as_str(), asset).is_some()
        {
            return Err(invalid("invalid release asset manifest"));
        }
    }
    let mut platforms = BTreeMap::new();
    let mut priorities = HashMap::new();
    let mut primary = None;
    for signed in assets.iter().filter(|asset| asset.name.ends_with(".sig")) {
        let name = signed.name.strip_suffix(".sig").unwrap();
        let artifact = by_name
            .get(name)
            .ok_or_else(|| invalid("updater signature has no matching artifact"))?;
        let (kind, priority) =
            installer(name).ok_or_else(|| invalid("unsupported signed updater artifact"))?;
        if !matches!(
            (matrix, kind),
            ("macos-arm64" | "macos-x64", "app")
                | ("linux-x64", "appimage" | "deb" | "rpm")
                | ("windows-x64", "nsis" | "msi")
        ) {
            return Err(invalid("updater artifact belongs to another platform"));
        }
        let signature_bytes = read_small(Path::new(&signed.path), 16 * 1024)?;
        if format!("{:x}", Sha256::digest(&signature_bytes)) != signed.digest {
            return Err(invalid("signature digest differs from collected metadata"));
        }
        let signature = std::str::from_utf8(&signature_bytes)?.trim().to_owned();
        if verify_artifact(Path::new(&artifact.path), &signature, key, version)? != artifact.digest
        {
            return Err(invalid("artifact digest differs from collected metadata"));
        }
        let platform = Platform {
            name: name.into(),
            digest: artifact.digest.clone(),
            signature,
        };
        let key = format!("{target}-{kind}");
        if priorities.get(&key) == Some(&priority) {
            return Err(invalid(
                "ambiguous updater artifacts for one installer type",
            ));
        }
        // Verify both raw and compatibility archives, but prefer the current raw
        // installer when both are present. Keep one download per installer target.
        if priorities.get(&key).is_none_or(|best| priority > *best) {
            priorities.insert(key.clone(), priority);
            platforms.insert(key, platform.clone());
        }
        if primary.as_ref().is_none_or(|(best, _)| priority > *best) {
            primary = Some((priority, platform));
        }
    }
    let (_, primary) =
        primary.ok_or_else(|| invalid("no signed updater artifacts were verified"))?;
    platforms.insert(target.into(), primary);
    Ok(Manifest {
        matrix: matrix.into(),
        version: version.into(),
        platforms,
    })
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [input, matrix, version, output] = args.as_slice() else {
        return Err(invalid(
            "usage: verify_updater <assets.json> <matrix> <version> <output.json>",
        ));
    };
    let encoded_key = std::env::var("TAURI_UPDATER_PUBKEY")?;
    if encoded_key.len() > 16 * 1024 {
        return Err(invalid("updater public key exceeds its size limit"));
    }
    let key_bytes = STANDARD.decode(encoded_key.trim())?;
    let key = PublicKey::decode(std::str::from_utf8(&key_bytes)?)?;
    let manifest = verify_manifest(Path::new(input), matrix, version, &key)?;
    std::fs::write(output, serde_json::to_vec(&manifest)?)?;
    println!("Verified signed updater artifacts for {matrix} version {version}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated by Tauri CLI 2.12.1 with an ephemeral test key; no private key is retained.
    const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDg2MTg2QzMzNzRCMjI5QzgKUldUSUtiSjBNMndZaGlPY3hOc1B3ODhYR0x6Q21oZ0dpZzZVMHpMWFVjRFRTSGxMTE9UYTVLSmIK";
    const SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUSUtiSjBNMndZaGtYUFRTa3FRc3ZGRmpBVzcvVWl4UnNyNnJDTDRNUm5ISmJ6ZDkzbG1XZ0l4T0dEcDJXNUM1RHlNeXFqOFZxY3E0MUk3QW9NWFB5cUg0QmVza3RmNkFBPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkxODE4OTU5CWZpbGU6YXJ0aWZhY3QJdmVyc2lvbjowLjUuMApqVmc5aHBTYVlrMjVSTzJYUGFHQVp6cmlFREkrTTlTTHRoeGkvSjRIVE9ESzlBV3RYTmRWeXFtVVVnb3pQZjJhM21ySzBhOFpYcnlScXlrdC85KzZBdz09Cg==";
    const PAYLOAD: &[u8] = b"holdmap signed updater fixture\n";

    fn key() -> PublicKey {
        PublicKey::decode(std::str::from_utf8(&STANDARD.decode(PUBLIC_KEY).unwrap()).unwrap())
            .unwrap()
    }

    #[test]
    fn real_tauri_signature_verifies_and_binds_the_version() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("artifact");
        std::fs::write(&artifact, PAYLOAD).unwrap();
        assert_eq!(
            verify_artifact(&artifact, SIGNATURE, &key(), "0.5.0").unwrap(),
            format!("{:x}", Sha256::digest(PAYLOAD))
        );
        assert!(verify_artifact(&artifact, SIGNATURE, &key(), "0.5.1").is_err());
        std::fs::write(&artifact, b"tampered installer").unwrap();
        assert!(verify_artifact(&artifact, SIGNATURE, &key(), "0.5.0").is_err());
    }

    #[test]
    fn unauthenticated_version_comment_and_malformed_signatures_fail() {
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("artifact");
        std::fs::write(&artifact, PAYLOAD).unwrap();
        let original = String::from_utf8(STANDARD.decode(SIGNATURE).unwrap()).unwrap();
        let changed = STANDARD.encode(original.replace("version:0.5.0", "version:0.5.1"));
        assert!(verify_artifact(&artifact, &changed, &key(), "0.5.1").is_err());
        for malformed in [
            "!invalid!".into(),
            STANDARD.encode("truncated"),
            STANDARD.encode(original.replace("version:0.5.0", "")),
        ] {
            assert!(verify_artifact(&artifact, &malformed, &key(), "0.5.0").is_err());
        }
    }

    #[test]
    fn upstream_minisign_fixture_verifies_but_missing_signed_version_fails() {
        let public =
            PublicKey::from_base64("RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3")
                .unwrap();
        let signature = STANDARD.encode("untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1556193335\tfile:test\ny/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==");
        let directory = tempfile::tempdir().unwrap();
        let artifact = directory.path().join("artifact");
        std::fs::write(&artifact, b"test").unwrap();
        let error = verify_artifact(&artifact, &signature, &public, "0.5.0").unwrap_err();
        assert!(error.to_string().contains("must carry one signed version"));
    }

    #[test]
    fn signed_metadata_matches_each_platform_and_rejects_wrong_digests() {
        let directory = tempfile::tempdir().unwrap();
        for (matrix, name, primary, installer_key) in [
            (
                "macos-arm64",
                "holdmap.app.tar.gz",
                "darwin-aarch64",
                "darwin-aarch64-app",
            ),
            (
                "macos-x64",
                "holdmap.app.tar.gz",
                "darwin-x86_64",
                "darwin-x86_64-app",
            ),
            (
                "linux-x64",
                "holdmap.AppImage",
                "linux-x86_64",
                "linux-x86_64-appimage",
            ),
            (
                "windows-x64",
                "holdmap.exe",
                "windows-x86_64",
                "windows-x86_64-nsis",
            ),
        ] {
            let artifact = directory.path().join(name);
            let signature = directory.path().join(format!("{name}.sig"));
            std::fs::write(&artifact, PAYLOAD).unwrap();
            std::fs::write(&signature, SIGNATURE).unwrap();
            let input = directory.path().join("assets.json");
            let mut assets = serde_json::json!([
                {"name":name,"path":artifact,"digest":format!("{:x}",Sha256::digest(PAYLOAD))},
                {"name":format!("{name}.sig"),"path":signature,"digest":format!("{:x}",Sha256::digest(SIGNATURE))}
            ]);
            std::fs::write(&input, serde_json::to_vec(&assets).unwrap()).unwrap();
            let verified = verify_manifest(&input, matrix, "0.5.0", &key()).unwrap();
            assert!(verified.platforms.contains_key(primary));
            assert!(verified.platforms.contains_key(installer_key));
            assert!(verify_manifest(&input, matrix, "0.5.1", &key()).is_err());
            assets[0]["digest"] = "0".repeat(64).into();
            std::fs::write(&input, serde_json::to_vec(&assets).unwrap()).unwrap();
            assert!(verify_manifest(&input, matrix, "0.5.0", &key()).is_err());
        }
    }

    #[test]
    fn raw_installers_are_preferred_while_compatibility_archives_are_also_verified() {
        let directory = tempfile::tempdir().unwrap();
        for (matrix, names, target, preferred) in [
            (
                "windows-x64",
                ["holdmap.exe", "holdmap.nsis.zip"],
                "windows-x86_64-nsis",
                "holdmap.exe",
            ),
            (
                "linux-x64",
                ["holdmap.AppImage", "holdmap.AppImage.tar.gz"],
                "linux-x86_64-appimage",
                "holdmap.AppImage",
            ),
        ] {
            let mut assets = Vec::new();
            for name in names {
                for (name, bytes) in [
                    (name.to_owned(), PAYLOAD),
                    (format!("{name}.sig"), SIGNATURE.as_bytes()),
                ] {
                    let path = directory.path().join(&name);
                    std::fs::write(&path, bytes).unwrap();
                    assets.push(serde_json::json!({"name":name,"path":path,"digest":format!("{:x}",Sha256::digest(bytes))}));
                }
            }
            let input = directory.path().join("assets.json");
            std::fs::write(&input, serde_json::to_vec(&assets).unwrap()).unwrap();
            let verified = verify_manifest(&input, matrix, "0.5.0", &key()).unwrap();
            assert_eq!(verified.platforms[target].name, preferred);
            let compatibility = directory.path().join(names[1]);
            std::fs::write(compatibility, b"tampered unused compatibility artifact").unwrap();
            assert!(verify_manifest(&input, matrix, "0.5.0", &key()).is_err());
            assets.push(assets[0].clone());
            std::fs::write(&input, serde_json::to_vec(&assets).unwrap()).unwrap();
            assert!(verify_manifest(&input, matrix, "0.5.0", &key()).is_err());
        }
    }
}
