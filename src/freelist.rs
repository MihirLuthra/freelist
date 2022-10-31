// TODO: Yet to "try" a lock free queue instead of bitmap based buckets.
//
// I had this bitmap based sync already written from long time back.
// Back then I remember reading some perf bottlenecks of lock free queues
// and wrote this for some improvement.
// But I don't remember at all what was I trying to improve
// and I surely didn't do any benchmarks.

use std::cell::UnsafeCell;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicUsize, Ordering};

use bit_fiddler::{set, unset};

/// A freelist containing `N` buckets. These buckets store
/// power of 2 sizes.
/// For example, if N is 5, 5 buckets will be held:
/// 1) bucket for 1 byte (2^0)
/// 2) bucket for 2 byte (2^1)
/// 3) bucket for 4 byte (2^2)
/// 4) bucket for 8 byte (2^3)
/// 5) bucket for 16 byte (2^4)
///
/// Each bucket has a certain capacity to store pointers.
/// Generally, freelist is helpful when producer and consumer
/// are both fast and ideally the size of each bucket shouldn't
/// be kept very large in that case.
///
/// (For now, size of each bucket is fixed to size_of::<usize>() * 8
/// but maybe configurable in future)
pub struct FreeList<T, const N: usize>([Dump<T>; N]);

macro_rules! impl_const_new {
    ($n:literal) => {
        impl<T> FreeList<T, $n> {
            /// Initialize a freelist with empty buckets.
            pub const fn new() -> FreeList<T, $n> {
                FreeList(seq_macro::seq!(
                    _ in 0..$n {
                        [#(Dump::new(),)*]
                    }
                ))
            }
        }
    };
}

impl_const_new!(1);
impl_const_new!(2);
impl_const_new!(3);
impl_const_new!(4);
impl_const_new!(5);
impl_const_new!(6);
impl_const_new!(7);
impl_const_new!(8);
impl_const_new!(9);
impl_const_new!(10);
impl_const_new!(11);
impl_const_new!(12);
impl_const_new!(13);
impl_const_new!(14);
impl_const_new!(15);
impl_const_new!(16);
impl_const_new!(17);
impl_const_new!(18);
impl_const_new!(19);
impl_const_new!(20);

impl<T, const N: usize> FreeList<T, N> {
    /// Expects a size which is power of 2 and returns
    /// a pointer if available in freelist.
    ///
    /// Returns SizeNotPowerOf2 if `size` is not power of 2
    /// Returns BucketEmpty is nothing is available.
    /// Returns BucketNotAvailable is bucket for the given
    /// size doesn't exist.
    pub fn recycle(&self, size: usize) -> Result<*mut T, Error> {
        if !size.is_power_of_two() {
            return Err(Error::SizeNotPowerOf2);
        }

        let power = size.trailing_zeros();

        if power < N as u32 {
            self.0[power as usize].recycle().ok_or(Error::BucketEmpty)
        } else {
            Err(Error::BucketNotAvailable)
        }
    }

    /// Throws the given pointer into the freelist.
    ///
    /// Returns SizeNotPowerOf2 if `size` is not power of 2
    /// Returns BucketFull if the corresponding bucket is full.
    /// Returns BucketNotAvailable is bucket for the given
    /// size doesn't exist.
    pub fn throw(&self, ptr: *mut T, size: usize) -> Result<(), Error> {
        if !size.is_power_of_two() {
            return Err(Error::SizeNotPowerOf2);
        }

        let power = size.trailing_zeros();

        if power < N as u32 {
            self.0[power as usize].throw(ptr).map_err(|_| Error::BucketFull)
        } else {
            Err(Error::BucketNotAvailable)
        }
    }

    /// Clears the freelist.
    ///
    /// f(ptr, bucket_size)
    ///   ptr:
    ///     ptr to free
    ///   bucket_size:
    ///     power of 2. For example, if
    ///     this value is 4, size to free
    ///     is 16.
    pub fn clear(&self, f: impl Fn(*mut T, usize)) {
        for (idx, dump) in self.0.iter().enumerate() {
            dump.clear(|ptr| f(ptr, idx))
        }
    }

    /// Clears bucket for the particular size.
    pub fn clear_bucket(&self, size: usize, f: impl Fn(*mut T)) -> Result<(), Error> {
        if !size.is_power_of_two() {
            return Err(Error::SizeNotPowerOf2);
        }

        let power = size.trailing_zeros();

        if power < N as u32 {
            self.0[power as usize].clear(f);
            Ok(())
        } else {
            Err(Error::BucketNotAvailable)
        }
    }
}

#[derive(Debug, Clone)]
/// Error thrown by methods of FreeList
pub enum Error {
    /// The bucket for the requested size is full.
    /// Memory can't be stored on the freelist.
    BucketFull,
    /// Bucket for the given size is not available.
    /// Can't be stored or fetched from the freelist.
    BucketNotAvailable,
    /// The bucket for the requested size is empty.
    /// Basically, memory can't be obtained from freelist.
    BucketEmpty,
    /// The freelist only supports power of 2
    /// sizes. The calling code needs to handle
    /// going to next power of 2 if needed.
    SizeNotPowerOf2,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::BucketFull => write!(f, "bucket is full"),
            Error::BucketNotAvailable => write!(f, "bucket not available"),
            Error::BucketEmpty => write!(f, "bucket is empty"),
            Error::SizeNotPowerOf2 => write!(f, "given size should be power of 2"),
        }
    }
}

impl std::error::Error for Error {}

/// In this struct,
/// max_bits!(reader_bitmap) == max_bits!(writer_bitmap) == dump.len()
///
/// The accesses to dump[] array are synchronized by reader_bitmap
/// and writer_bitmap.
///
/// Max possible length is (sizeof(usize) * 8) which is actually
/// all what is needed as such a structure is meant for cases
/// where producer and consumer are equally fast.
/// Otherwise also, it isn't generally required to keep a lot
/// of memory unfreed.
pub struct Dump<T> {
    reader_bitmap: AtomicUsize,
    writer_bitmap: AtomicUsize,
    dump: UnsafeCell<[*mut T; usize::BITS as usize]>,
}

unsafe impl<T> Send for Dump<T> {}
unsafe impl<T> Sync for Dump<T> {}

impl<T> Dump<T> {
    /// Returns a new Dump instance.
    ///
    /// ```ignore
    ///
    /// struct Example {
    ///     a: i32,
    ///     b: String,
    /// }
    ///
    /// let dump = Dump::<Example>::new();
    /// ```
    pub const fn new() -> Self {
        Dump {
            reader_bitmap: AtomicUsize::new(0),
            writer_bitmap: AtomicUsize::new(0),
            dump: UnsafeCell::new([null_mut::<T>(); usize::BITS as usize]),
        }
    }

    /// Adds a new element to the dump. On success it returns
    /// () and on failure returns back the ptr indicating
    /// that it couldn't be stored.
    ///
    /// To synchronize this addition to the dump[] array, the following
    /// procedure is followed:
    ///
    /// 1) It checks `writer_bitmap` for unset bits (0 bits).
    /// 2) When it finds one, it atomically sets it.
    /// 3) We use this bit position as the index in `dump[]` to store the value.
    /// 4) Setting the bit in `writer_bitmap` ensures that no
    ///    other thread will write at that index.
    /// 5) After storing `raw` in the `dump[]`, we tell reader threads
    ///    that this index is available for read. To do this, we set this
    ///    same bit position in `reader_bitmap` atomically.
    pub fn throw(&self, raw: *mut T) -> Result<(), *mut T> {
        let mut old_writer_bitmap = self.writer_bitmap.load(Ordering::Relaxed);
        let mut first_empty_spot;

        loop {
            // basically returns the first bit which is 0
            first_empty_spot = old_writer_bitmap.trailing_ones();

            // occupy `first_empty_spot` in `old_writer_bitmap` and assign it to `new_writer_bitmap`
            let new_writer_bitmap = if first_empty_spot == usize::BITS {
                return Err(raw);
            } else {
                set!(old_writer_bitmap, usize, first_empty_spot)
            };

            match self.writer_bitmap.compare_exchange_weak(
                old_writer_bitmap,
                new_writer_bitmap,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(old) => old_writer_bitmap = old,
            };
        }

        let dump_ptr = self.dump.get();

        unsafe {
            (*dump_ptr)[first_empty_spot as usize] = raw;
        }

        let mut old_reader_bitmap = self.reader_bitmap.load(Ordering::Relaxed);

        loop {
            let new_reader_bitmap = set!(old_reader_bitmap, usize, first_empty_spot);

            /*
             * Memory order on success should be `Ordering::Release`.
             * If it was Ordering::Relaxed, it would become possible
             * that `recycle()` sees this bit as set in `reader_bitmap`
             * but doesn't see the newly updated value in `dump[]`.
             */
            match self.reader_bitmap.compare_exchange_weak(
                old_reader_bitmap,
                new_reader_bitmap,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(old) => old_reader_bitmap = old,
            };
        }

        Ok(())
    }

    /// Gets a value from the dump. On success it returns
    /// the value `*mut T` and on failure (). Failure indicates
    /// that dump is empty.
    ///
    /// To synchronize the retreival from the dump[] array, the following
    /// procedure is followed:
    ///
    /// 1) A set bit is searched in `reader_bitmap` and then we
    ///    atomically unset that bit in `reader_bitmap`.
    /// 2) Corresponding to the bit posn that we unset, we get the
    ///    `dump[bit_posn]`.
    /// 3) Then to allow writers to use this position for new writes,
    ///    we unset this bit from `writer_bitmap`.
    /// 4) Finally, we return `dump[bit_posn]`.
    pub fn recycle(&self) -> Option<*mut T> {
        let mut old_reader_bitmap = self.reader_bitmap.load(Ordering::Relaxed);
        let mut first_set_spot;

        loop {
            // basically returns the first bit which is 1
            first_set_spot = old_reader_bitmap.trailing_zeros();

            // occupy `first_set_spot` in `old_reader_bitmap` and assign it to `new_reader_bitmap`
            let new_reader_bitmap = if first_set_spot == usize::BITS {
                return None;
            } else {
                unset!(old_reader_bitmap, usize, first_set_spot)
            };

            match self.reader_bitmap.compare_exchange_weak(
                old_reader_bitmap,
                new_reader_bitmap,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(old) => old_reader_bitmap = old,
            };
        }

        let dump_ptr = self.dump.get();

        let retval = unsafe { (*dump_ptr)[first_set_spot as usize] };

        let mut old_writer_bitmap = self.writer_bitmap.load(Ordering::Relaxed);

        loop {
            let new_writer_bitmap = unset!(old_writer_bitmap, usize, first_set_spot);

            match self.writer_bitmap.compare_exchange_weak(
                old_writer_bitmap,
                new_writer_bitmap,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(old) => old_writer_bitmap = old,
            };
        }

        Some(retval)
    }

    /// This executes closure `f` for every value in the dump
    /// and clears the dump.
    ///
    /// Does the following:
    /// - Tries to replace reader bitmap with 0
    /// - Calls f() for each index that was set as per the bitmap.
    /// - Sets writer bitmap to 0.
    pub fn clear(&self, f: impl Fn(*mut T)) {
        let mut old_reader_bitmap = self.reader_bitmap.load(Ordering::Relaxed);
        let new_reader_bitmap = 0;

        loop {
            if old_reader_bitmap == 0 {
                return;
            }

            match self.reader_bitmap.compare_exchange_weak(
                old_reader_bitmap,
                new_reader_bitmap,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(old) => old_reader_bitmap = old,
            };
        }

        let mut old_reader_bitmap_copy = old_reader_bitmap;

        loop {
            let first_set_spot = old_reader_bitmap_copy.trailing_zeros();

            if first_set_spot == usize::BITS {
                break;
            }

            unset!(in old_reader_bitmap_copy, usize, first_set_spot);

            let dump_ptr = self.dump.get();
            let val_at_index = unsafe { (*dump_ptr)[first_set_spot as usize] };

            f(val_at_index);
        }

        let mut old_writer_bitmap = self.writer_bitmap.load(Ordering::Relaxed);

        loop {
            let new_writer_bitmap = old_writer_bitmap & !old_reader_bitmap;

            match self.writer_bitmap.compare_exchange_weak(
                old_writer_bitmap,
                new_writer_bitmap,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(old) => old_writer_bitmap = old,
            };
        }
    }
}
