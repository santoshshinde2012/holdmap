# Vendored GLib security backport

`glib/` is the published GLib **0.18.5** crate, with the exact two-line fix from
[upstream commit b5a4071](https://github.com/gtk-rs/gtk-rs-core/commit/b5a4071e439bef2b5eea76c3aa25e5ae84839e34)
backported to `src/variant_iter.rs`. It makes the C output pointer mutable and passes `&mut p`
so optimized `VariantStrIter` iteration does not dereference a null pointer.
The crate version and upstream MIT license remain unchanged.

The original package is from commit `42b9caf98e03ded086362d9653ca58fe94dc8658`.
Its [published archive](https://static.crates.io/crates/glib/glib-0.18.5.crate) has SHA-256
`233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5`.
[glib-provenance.json](glib-provenance.json) records every original file hash and the single
patched file hash. The generated Cargo cache marker `.cargo-ok` is excluded.

The current GTK3 dependency requires GLib 0.18. Updating GLib independently to the fixed
upstream release 0.20 would leave GTK's 0.18 dependency in place. This source override keeps
the existing API and binding family while fixing [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html).
It stays outside Holdmap's first-party Cargo workspace; upstream filenames and reference
files are retained so the imported package can be verified exactly.
This includes upstream's `Cargo.toml.orig` metadata; it is part of the published crate.
Git attributes preserve the snapshot's original bytes on Windows as well as Unix.

Run `python3 scripts/verify-vendored-glib.py` from the repository to verify the complete source
snapshot, reverse the patch to prove it contains only the two upstream changes, and check that
Cargo.lock and GTK's resolved dependency select this local copy. The verifier requires Python
3.11+ and Cargo; locked metadata resolution can fetch dependencies but does not build GTK.
On Linux, run the optimized regression against the actual resolved dependency:

```sh
cargo test -p holdmap-desktop --locked --config 'profile.test.package.glib.opt-level=3' --test glib_security
```

`cargo-deny` checks transitive unsoundness advisories with `unsound = "all"`. It does not scan
local path packages against registry advisories, so a clean result alone does not verify this
backport. The source verifier and optimized runtime regression are required checks; no
advisory is ignored. Dependabot may continue reporting the registry version in older branches
or released lockfiles. Track remediation through the changed source and resolution, rather
than describing the original advisory as a false positive.

When the desktop binding family can use a compatible fixed upstream release, remove the path
patch, this snapshot and its provenance checks together. Until then, changes to this directory
must be reviewed against the original archive and upstream fix; do not apply general formatting,
rename files, alter the version to change scanner results, or add unrelated patches.
