//! Maintainer tool for the first-party analyzer catalog. Never part of holdmap itself.
//!
//!   keygen <secret-key-file>              new key pair; writes the secret key (mode 0600) and
//!                                         catalog/holdmap-catalog.pub
//!   sign <secret-key-file> <version> <YYYY-MM-DD>
//!                                         writes catalog/analyzers.json and its .minisig from
//!                                         the manifests listed in catalog::SOURCES
//!
//! The secret key is never printed. Keep it out of the repository.

use holdmap_analyzers::catalog::{render, trusted_comment, SOURCES};
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("catalog");
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["keygen", key] => keygen(Path::new(key), &dir),
        ["sign", key, version, date] => sign(Path::new(key), version.parse()?, date, &dir),
        _ => Err("usage: sign_catalog keygen <key> | sign <key> <version> <YYYY-MM-DD>".into()),
    }
}

fn keygen(key: &Path, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if key.exists() {
        return Err(format!("{} exists; refusing to overwrite a key", key.display()).into());
    }
    let pair = minisign::KeyPair::generate_unencrypted_keypair()?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options.open(key)?.write_all(
        pair.sk
            .to_box(Some("holdmap catalog secret key"))?
            .to_string()
            .as_bytes(),
    )?;
    let public: PathBuf = dir.join("holdmap-catalog.pub");
    std::fs::write(&public, pair.pk.to_box()?.to_string())?;
    println!(
        "secret key written (not shown); public key: {}",
        public.display()
    );
    Ok(())
}

fn sign(
    key: &Path,
    version: u32,
    date: &str,
    dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let sk = minisign::SecretKeyBox::from_string(&std::fs::read_to_string(key)?)?
        .into_unencrypted_secret_key()?;
    let json = render(version, date, SOURCES);
    let sig = minisign::sign(
        None,
        &sk,
        json.as_bytes(),
        Some(&trusted_comment(version)),
        Some("holdmap analyzer catalog signature"),
    )?;
    std::fs::write(dir.join("analyzers.json"), &json)?;
    std::fs::write(dir.join("analyzers.json.minisig"), sig.to_string())?;
    println!("signed catalog v{version}");
    Ok(())
}
