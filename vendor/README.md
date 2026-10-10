# Vendored GLib security backports

`glib/` is the published GLib **0.18.5** crate with two audited upstream fixes:

- [Commit b5a4071](https://github.com/gtk-rs/gtk-rs-core/commit/b5a4071e439bef2b5eea76c3aa25e5ae84839e34)
  changes two lines in `src/variant_iter.rs` to make the C output pointer mutable and pass
  `&mut p`, fixing optimized `VariantStrIter` iteration.
- [Commit f54ceb3](https://github.com/gtk-rs/gtk-rs-core/commit/f54ceb387a2b6d6830753c1522cba7c858fb1aed)
  corrects `src/boxed_inline.rs` to allocate room for the entire slice before copying it.
  The original safe conversion API allocated only one element, causing a heap overflow for
  slices with multiple elements, as reported in [upstream issue #2040](https://github.com/gtk-rs/gtk-rs-core/issues/2040).

The corrected slice allocation also uses `g_malloc0` as a separate local initialization fix.
`Value` and `SendValue` copy callbacks call `g_value_init`, which requires a zero-filled
destination; the original uninitialized allocation could crash even for a single element.
This matches the existing individual `Value` copy allocation. The [GObject implementation](https://github.com/GNOME/glib/blob/main/gobject/gvalue.c)
documents that requirement, and [GLib's allocator](https://docs.gtk.org/glib/func.malloc0.html)
provides zero-filled memory. This local change is distinct from the two upstream commits.

The crate version and upstream MIT license remain unchanged.

The original package is from commit `42b9caf98e03ded086362d9653ca58fe94dc8658`.
Its [published archive](https://static.crates.io/crates/glib/glib-0.18.5.crate) has SHA-256
`233daaf6e83ae6a12a52055f568f9d7cf4671dabb78ff9560ab6da230ce00ee5`.
[glib-provenance.json](glib-provenance.json) records every original file hash, both fix commits,
the local initialization fix and both patched file hashes. The generated Cargo cache marker
`.cargo-ok` is excluded.

The current GTK3 dependency requires GLib 0.18. The iterator advisory was first fixed in
GLib 0.20; updating GLib independently to that version would leave GTK's 0.18 dependency
in place. This source override keeps
the existing API and binding family while fixing [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html)
and the boxed-inline allocation and initialization bugs.
It stays outside Holdmap's first-party Cargo workspace; upstream filenames and reference
files are retained so the imported package can be verified exactly.
This includes upstream's `Cargo.toml.orig` metadata; it is part of the published crate.
Git attributes preserve the snapshot's original bytes on Windows as well as Unix.

Run `python3 scripts/verify-vendored-glib.py` from the repository to verify the complete source
snapshot, reverse both upstream patches and the local initialization change to prove they
contain only the audited changes, and check that Cargo.lock and GTK's resolved dependency
select this local copy. The verifier requires Python
3.11+ and Cargo; locked metadata resolution can fetch dependencies but does not build GTK.
On Linux, run the iterator and boxed-inline slice regressions against the actual resolved
dependency with GLib optimization enabled:

```sh
G_DEBUG=fatal-warnings MALLOC_PERTURB_=165 cargo test -p holdmap-desktop --locked --config 'profile.test.package.glib.opt-level=3' --test glib_security
```

`cargo-deny` checks transitive unsoundness advisories with `unsound = "all"`. It does not scan
local path packages against registry advisories, so a clean result alone does not verify these
source changes. The source verifier and optimized runtime regressions are required checks; no
advisory is ignored. Dependabot may continue reporting the registry version in older branches
or released lockfiles. Track remediation through the changed source and resolution, rather
than describing the original advisory as a false positive.

When the desktop binding family can use a compatible fixed upstream release, remove the path
patch, this snapshot and its provenance checks together. Until then, changes to this directory
must be reviewed against the original archive, upstream fixes and documented local initialization
fix; do not apply general formatting,
rename files, alter the version to change scanner results, or add unrelated patches.
