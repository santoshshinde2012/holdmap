//! RUSTSEC-2024-0429 regression over the GLib instance used by the Linux desktop.
//!
//! CI compiles GLib at opt-level=3: unoptimized builds can hide the original out-pointer
//! aliasing bug, which caused a null-pointer crash in each iterator operation below.
#![cfg(target_os = "linux")]

use glib::variant::ToVariant;

#[test]
fn variant_string_iteration_remains_sound_when_optimized() {
    let variant = ["first", "middle", "last"].to_variant();
    assert_eq!(variant.array_iter_str().unwrap().next(), Some("first"));
    assert_eq!(variant.array_iter_str().unwrap().next_back(), Some("last"));
    assert_eq!(variant.array_iter_str().unwrap().nth(1), Some("middle"));
    assert_eq!(
        variant.array_iter_str().unwrap().nth_back(1),
        Some("middle")
    );
    assert_eq!(variant.array_iter_str().unwrap().last(), Some("last"));

    let mut both_ends = variant.array_iter_str().unwrap();
    assert_eq!(both_ends.next(), Some("first"));
    assert_eq!(both_ends.next_back(), Some("last"));
    assert_eq!(both_ends.next(), Some("middle"));
    assert_eq!(both_ends.next_back(), None);
    assert_eq!(both_ends.next(), None);
    assert_eq!(variant.array_iter_str().unwrap().nth(usize::MAX), None);
    assert_eq!(variant.array_iter_str().unwrap().nth_back(usize::MAX), None);

    let empty = Vec::<String>::new().to_variant();
    assert_eq!(empty.array_iter_str().unwrap().next(), None);
    assert_eq!(empty.array_iter_str().unwrap().next_back(), None);
}
