use std::cell::UnsafeCell;
use std::collections::HashMap;

use libc::{c_void, size_t};
use once_cell::unsync::Lazy;

use crate::freelist::{Error, FreeList};

thread_local! {
    /// Mapping from pointer to size of memory
    static MEMORY_MAP: Lazy<UnsafeCell<HashMap<*mut c_void, usize>>> = Lazy::new(|| {
        UnsafeCell::new(HashMap::new())
    });
}

static FREELIST: FreeList<c_void, 11> = FreeList::<_, 11>::new();

/// A calloc wrapper that to make use of freelist. If freelist doesn't
/// have any pointers, it will call `underlying_calloc()`.
///
/// The requested size is converted into the next power of 2 if this function
/// thinks that it can be reused in freelist later. Otherwise, it forwards the
/// requested args as they are to `underlying_calloc`.
/// If the freelist thinks this ptr can later be used, it stores it in a thread
/// local map. [free] would store this in freelist only if this thread local
/// state has a mapping for it.
///
/// NOTE: `underlying_calloc` is expected to allocate exactly what is asked from it.
pub fn calloc(nmemb: size_t, size: size_t, underlying_calloc: impl FnOnce(size_t, size_t) -> *mut c_void) -> *mut c_void {
    let next_power_of_2 = (nmemb * size).next_power_of_two();
    let mut new_nmemb = 1;
    let mut new_size = next_power_of_2;
    let mut recyclable = true;

    let res = match FREELIST.recycle(next_power_of_2) {
        Ok(ptr) => Ok(ptr),
        Err(Error::BucketFull) => unreachable!(),
        Err(Error::BucketEmpty) => Err(()),
        Err(Error::SizeNotPowerOf2 /* in case next_power_of_2() returns 0 */ | Error::BucketNotAvailable) => {
            recyclable = false;
            new_nmemb = nmemb;
            new_size = size;
            Err(())
        },
    };

    let res = match res {
        Ok(ptr) => ptr,
        Err(_) => underlying_calloc(new_nmemb, new_size),
    };

    if recyclable {
        MEMORY_MAP.with(|m| unsafe { m.get().as_mut().unwrap().insert(res, new_nmemb * new_size) });
    }

    res
}

/// A free wrapper that puts ptr on the freelist if it is reusable.
/// If freelist is full or unusable, it simply calls `underlying_free`.
///
/// See [calloc] for more info.
pub fn free(ptr: *mut c_void, underlying_free: impl Fn(*mut c_void)) {
    if let Some(&size) = MEMORY_MAP.with(|m| unsafe { m.get().as_ref().unwrap().get(&ptr) }) {
        match FREELIST.throw(ptr, size) {
            Ok(()) => {}
            Err(Error::BucketEmpty | Error::BucketNotAvailable | Error::SizeNotPowerOf2) => unreachable!(),
            Err(Error::BucketFull) => {
                MEMORY_MAP.with(|m| unsafe { m.get().as_mut().unwrap().remove(&ptr) });
                underlying_free(ptr)
            }
        };
    } else {
        underlying_free(ptr)
    }
}

/// Clears freelist.
///
/// Implementation in this module has thread local tracking. (See [calloc]).
/// So, if thread doesn't know about the pointer, it won't be reused and
/// will just keep lying in the freelist.
/// So, clear_freelist should be called periodically to make space
/// for new pointers.
pub fn clear_freelist(underlying_free: impl Fn(*mut c_void)) {
    FREELIST.clear(|ptr, _| underlying_free(ptr));
}
