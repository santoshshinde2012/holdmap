# Vendored dependency fixes

## GLib security backports

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

The Linux regression's direct GLib dependency names this audited source with
`path = "../../../vendor/glib"` in the desktop manifest. The desktop crate has
`publish = false`, so it needs no registry fallback. GTK continues to use the root
`[patch.crates-io]` override; the verifier requires both dependency edges to select the same
local GLib 0.18.5 package. This declaration makes the test source explicit without changing
the version, lockfile or advisory policy.

Run `python3 scripts/verify-vendored-glib.py` from the repository to verify the complete source
snapshot, reverse both upstream patches and the local initialization change to prove they
contain only the audited changes, and check that Cargo.lock, GTK and the Linux regression
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
advisory is ignored. Dependabot may continue reporting the default branch's registry
declaration until this change is merged and its dependency graph refreshes. The original
registry crate and older released lockfiles remain affected. Track remediation through the
changed source and resolution, rather than describing the original advisory as a false positive.

When the desktop binding family can use a compatible fixed upstream release, remove the path
patch, this snapshot and its provenance checks together. Until then, changes to this directory
must be reviewed against the original archive, upstream fixes and documented local initialization
fix; do not apply general formatting,
rename files, alter the version to change scanner results, or add unrelated patches.

## Global-hotkey X11 reliability fixes

`global-hotkey/` retains the published **0.8.0** package, version and dual Apache-2.0/MIT
licenses. Only `src/platform_impl/x11/mod.rs` changes; the macOS and Windows backends,
public API, manifests and all other upstream files remain byte-identical.

The independent Linux desktop proof reproduced a genuine global key reservation after
the user selected **Off**: callbacks stopped and the setting persisted, but all four
CapsLock/NumLock variants stayed grabbed until the app exited. The released X11 driver
queued `XUngrabKey` requests without flushing or checking them. Collision rollback used
the same buffered requests. [Upstream pull request #197](https://github.com/tauri-apps/global-hotkey/pull/197)
identifies both causes, but remains unmerged and absent from the latest published release
when this fix was prepared.

This local patch checks each ungrab acknowledgment before removing registration state
or completing collision rollback. The checks flush requests and confirm that the X server
processed them; request or connection errors propagate through the existing error types.
This is a checked implementation of the buffering correction rather than a claim that the
open upstream patch has been released.

The patch also returns `OsError(BrokenPipe)` when either the worker request channel or its
reply channel closes. Previously all four public register/unregister methods could report
success after the X11 worker failed, including an invalid `DISPLAY`. The asynchronous
constructor remains unchanged: unavailable optional hotkeys cannot abort Tauri plugin
setup before the app can honor **Off**. Batch operations now send one fail-fast reply and
stop at their first error, matching the existing Windows behavior. Earlier successful keys
in a failed batch retain their registrations until explicitly removed.

The original package is from commit `2a620bf3852008b568f6d36c2baedcc3dd0822f2`.
Its [published archive](https://static.crates.io/crates/global-hotkey/global-hotkey-0.8.0.crate)
has SHA-256 `8c386b0a4a70cb2d39fffd74480f985b6f0bfbcb934b6a6b6b7e630e448f242e`,
which matches the original Cargo.lock checksum.
[global-hotkey-provenance.json](global-hotkey-provenance.json) records every original file
hash, the related upstream PR commit, the reviewed local fixes and the changed file hash.
[global-hotkey-x11.patch](global-hotkey-x11.patch) contains the exact minimal source diff,
including private channel regression tests. The published crate snapshot includes its
original lockfile, examples and metadata; Cargo build/cache output stays outside it.

Run `python3 scripts/verify-vendored-global-hotkey.py` from the repository. It verifies the
complete file set and hashes, reverses the exact reviewed patch back to the published X11
source, and checks that the lockfile, genuine Tauri plugin and Linux desktop regressions
all consume the same local package. It requires Python 3.11+ and Cargo; metadata resolution
may fetch dependencies but does not build the desktop or connect to a display.

[The retained Linux regressions](../apps/desktop/src-tauri/tests/hotkey_x11.rs) exercise all
four public API calls against an unavailable X11 worker in a finite owned subprocess. The
positive tests use a separate X11 client to check every lock variant, immediate release,
partial collision rollback, preservation of another client's reservation and fail-fast
batch behavior. They only connect when explicitly armed for a private Xvfb display:

```sh
xvfb-run -a env HOLDMAP_X11_TEST=1 cargo test -p holdmap-desktop --locked --test hotkey_x11 -- --include-ignored --test-threads=1
```

The ordinary workspace suite runs the unavailable-display regression and ignores the
private-display cases. An unarmed subprocess helper returns without any OS action.
Do not point these tests at an ordinary user's display. The independent genuine desktop
runtime proof also tests actual **Off**, shortcut callbacks, all lock reservations and
release while the application remains alive.

When a compatible upstream release contains these fixes, remove this patch, snapshot,
provenance checks and Cargo override together. Until then, preserve upstream source bytes,
version and licenses; review any further change against the published archive and exact
documented local diff. These X11 changes do not add Wayland portal support.
