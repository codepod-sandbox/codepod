fn main() {
    // Placeholder -- will be filled in Task 5
}

// ---------------------------------------------------------------------------
// WASM allocator exports — allow the host to allocate/free guest memory
// ---------------------------------------------------------------------------

/// Allocate `size` bytes of guest memory and return the pointer.
/// Used by the host to prepare buffers before calling into the guest.
#[no_mangle]
pub extern "C" fn __alloc(size: u32) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

/// Free `size` bytes of guest memory starting at `ptr`.
///
/// # Safety
///
/// `ptr` must have been allocated by `__alloc` with the same `size`.
#[no_mangle]
pub unsafe extern "C" fn __dealloc(ptr: *mut u8, size: u32) {
    let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
    std::alloc::dealloc(ptr, layout);
}
