#![deny(warnings)]
#![deny(missing_docs)]

//! [FreeList] type provided by this crate can be
//! on top of allocators to reuse allocated memory.
//!
//! This is meant for scenarios where allocators are
//! slow and don't perform well between threads. For
//! example, current allocator (rust version 1.65) for
//! `x86_64-fortanix-unknown-sgx` holds a golbal spin lock
//! to allocate memory. This can be insanely slow.
//! Putting a freelist on top gives a huge performance boost.
//!
//! One use case of this freelist is in [calloc] module.
//! For exmaple, the `calloc` and `free` functions provided
//! by it can be used to override default calloc/free in
//! mbedtls crate.
//!
//! Otherwise, it maybe used in a global_allocator.

mod freelist;
pub use freelist::*;

#[cfg(feature = "calloc")]
/// Provides calloc/free wrappers that use
/// [FreeList] type.
pub mod calloc;
