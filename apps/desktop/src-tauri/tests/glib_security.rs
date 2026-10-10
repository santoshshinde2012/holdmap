//! Safety regressions over the GLib instance used by the Linux desktop.
//!
//! CI compiles GLib at opt-level=3: unoptimized builds can hide the original out-pointer
//! aliasing bug, which caused a null-pointer crash in each iterator operation below.
#![cfg(target_os = "linux")]

use glib::translate::{FromGlibContainer, ToGlibContainerFromSlice};
use glib::value::ToValue;
use glib::variant::ToVariant;
use std::cell::Cell;

// The macro's copy callback checks the allocator's region before writing. An undersized
// allocation therefore fails deterministically without corrupting the test process's heap.
#[derive(Copy, Clone)]
#[repr(C)]
pub struct SliceProbeFFI {
    words: [u64; 8],
}

thread_local! {
    static SLICE_ALLOCATION_BASE: Cell<usize> = const { Cell::new(0) };
}

extern "C" {
    fn malloc_usable_size(ptr: *mut std::ffi::c_void) -> usize;
}

unsafe fn checked_slice_copy(dest: *mut SliceProbeFFI, src: *const SliceProbeFFI) {
    SLICE_ALLOCATION_BASE.with(|base| {
        if base.get() == 0 {
            base.set(dest as usize);
        }
        let available = malloc_usable_size(base.get() as *mut std::ffi::c_void);
        let end = (dest as usize - base.get()) + std::mem::size_of::<SliceProbeFFI>();
        assert!(
            end <= available,
            "boxed-inline slice copy exceeds allocation: end={end}, allocation={available}"
        );
        std::ptr::copy_nonoverlapping(src, dest, 1);
    });
}

glib::wrapper! {
    pub struct SliceProbe(BoxedInline<SliceProbeFFI>);
    match fn {
        init => |ptr| std::ptr::write(ptr, SliceProbeFFI { words: [0; 8] }),
        copy_into => |dest, src| checked_slice_copy(dest, src),
        clear => |_ptr| (),
    }
}

#[test]
fn boxed_inline_slice_allocates_for_every_element() {
    let items = [
        SliceProbe {
            inner: SliceProbeFFI { words: [1; 8] },
        },
        SliceProbe {
            inner: SliceProbeFFI { words: [2; 8] },
        },
    ];
    SLICE_ALLOCATION_BASE.with(|base| base.set(0));
    let ptr =
        <SliceProbe as ToGlibContainerFromSlice<'_, *mut SliceProbeFFI>>::to_glib_full_from_slice(
            &items,
        );
    assert!(!ptr.is_null());
    // The callbacks above verified the entire region before any element copy.
    unsafe {
        assert_eq!((*ptr).words, [1; 8]);
        assert_eq!((*ptr.add(1)).words, [2; 8]);
        glib::ffi::g_free(ptr as *mut _);
    }
}

#[test]
fn boxed_inline_value_slices_initialize_and_transfer_owned_values() {
    // GValue initialization requires zero-filled destinations. The optimized CI step
    // perturbs fresh malloc storage so this does not depend on a fortunate allocator state.
    for expected in [vec![], vec!["single"], vec!["first", "middle", "last"]] {
        let values: Vec<glib::Value> = expected.iter().map(|s| s.to_value()).collect();
        let ptr = <glib::Value as ToGlibContainerFromSlice<
            '_,
            *mut glib::gobject_ffi::GValue,
        >>::to_glib_full_from_slice(&values);
        assert!(!ptr.is_null());
        // The full-transfer conversion moves every initialized value and frees the
        // GLib array. Dropping the Rust vector then releases its owned string values.
        let actual: Vec<glib::Value> =
            unsafe { FromGlibContainer::from_glib_full_num(ptr, values.len()) };
        let strings: Vec<String> = actual.iter().map(|v| v.get::<String>().unwrap()).collect();
        assert_eq!(strings, expected);
    }
}

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
