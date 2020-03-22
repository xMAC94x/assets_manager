//! Definitions of cache entries and locks

use std::{
    fmt,
    hash,
    mem,
    ops::Deref,
    ptr,
};


#[cfg(feature = "parking_lot")]
pub use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
#[cfg(not(feature = "parking_lot"))]
pub use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};


/// `RwLock` from `parking_lot` and `std` have different APIs, so we use this
/// simple wrapper to easily permit both.
pub(crate) mod rwlock {
    use super::{RwLock, RwLockReadGuard, RwLockWriteGuard};

    /// Simple wrapper around `RwLock::read`.
    #[inline]
    pub fn read<T: ?Sized>(this: &RwLock<T>) -> RwLockReadGuard<T> {
        #[cfg(feature = "parking_lot")]
        let guard = this.read();

        #[cfg(not(feature = "parking_lot"))]
        let guard = this.read().unwrap();

        guard
    }

    /// Simple wrapper around `RwLock::write`.
    #[inline]
    pub fn write<T: ?Sized>(this: &RwLock<T>) -> RwLockWriteGuard<T> {
        #[cfg(feature = "parking_lot")]
        let guard = this.write();

        #[cfg(not(feature = "parking_lot"))]
        let guard = this.write().unwrap();

        guard
    }

    /// Simple wrapper around `RwLock::get_mut`.
    #[inline]
    pub fn get_mut<T: ?Sized>(this: &mut RwLock<T>) -> &mut T {
        #[cfg(feature = "parking_lot")]
        let guard = this.get_mut();

        #[cfg(not(feature = "parking_lot"))]
        let guard = this.get_mut().unwrap();

        guard
    }

    /// Simple wrapper around `RwLock::into_inner`.
    #[inline]
    pub fn into_inner<T>(this: RwLock<T>) -> T {
        #[cfg(feature = "parking_lot")]
        let inner = this.into_inner();

        #[cfg(not(feature = "parking_lot"))]
        let inner = this.into_inner().unwrap();

        inner
    }
}

/// This struct is used to store [`ContreteCacheEntry`] of different types in
/// the same container.
///
/// A [`ContreteCacheEntry`] can be safely transmuted in this struct. However,
/// it can only be transmuted back with the type parameter which was used to
/// create it.
///
/// The `repr(C)` attribute ensures that the compiler doesn't change the layout
/// of the struct, so the data transmutation is legal. It is thus important to
/// keep the definitions of these structs in sync.
///
/// # Safety
///
/// - Methods that are generic over `T` can only be called with the same `T` used
/// to create them.
/// - When an `AssetRefLock<'a, T>` is returned, you have to ensure that `self`
/// outlives it. The `CacheEntry` can be moved be cannot be dropped.
///
/// [`ContreteCacheEntry`]: struct.ContreteCacheEntry.html
#[repr(C)]
pub(crate) struct CacheEntry {
    /// A pointeur representing the `Box` contained by the underlying `ContreteCacheEntry`.
    data: *const RwLock<()>,

    /// The concrete function to call to drop the concrete entry.
    drop_concrete: unsafe fn(&mut CacheEntry),
}

impl<'a> CacheEntry {
    /// Creates a new `CacheEntry` containing an asset of type `T`.
    ///
    /// The returned structure can safely use its methods with type parameter `T`.
    #[inline]
    pub fn new<T: Send + Sync>(asset: T) -> Self {
        let concrete = ContreteCacheEntry {
            data: Box::new(RwLock::new(asset)),
            drop: CacheEntry::drop_data::<T>,
        };

        unsafe { mem::transmute(concrete) }
    }

    /// Drops the inner data of the `CacheEntry`.
    ///
    /// Leaves `self.data` dangling.
    ///
    /// # Safety
    ///
    /// See type-level documentation.
    unsafe fn drop_data<T: Send + Sync>(&mut self) {
        let my_box = &mut self.data as *mut *const RwLock<()> as *mut Box<RwLock<T>>;
        ptr::drop_in_place(my_box);
    }

    /// Reurns a reference to the underlying lock
    ///
    /// # Safety
    ///
    /// See type-level documentation.
    #[inline]
    pub unsafe fn get_ref<T: Send + Sync>(&self) -> AssetRefLock<'a, T> {
        let concrete = {
            let ptr = self as *const CacheEntry as *const ContreteCacheEntry<T>;
            &*ptr
        };
        concrete.get_ref()
    }

    /// Write a value and a get reference to the underlying lock
    ///
    /// # Safety
    ///
    /// See type-level documentation.
    pub unsafe fn write<T: Send + Sync>(&self, asset: T) -> AssetRefLock<'a, T> {
        let lock = self.get_ref();
        let mut cached_guard = rwlock::write(&lock.data);
        *cached_guard = asset;
        drop(cached_guard);
        lock
    }

    /// Consumes the `CacheEntry` and returns its inner value.
    ///
    /// # Safety
    ///
    /// See type-level documentation.
    #[inline]
    pub unsafe fn into_inner<T: Send + Sync>(self) -> T {
        let concrete: ContreteCacheEntry<T> = mem::transmute(self);
        concrete.into_inner()
    }
}

// Safety: T is Send + Sync
unsafe impl Send for CacheEntry {}
unsafe impl Sync for CacheEntry {}

impl fmt::Debug for CacheEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad("CacheEntry { ... }")
    }
}

impl Drop for CacheEntry {
    fn drop(&mut self) {
        unsafe {
            (self.drop_concrete)(self);
        }
    }
}


/// The concrete type behind a [`CacheEntry`].
///
/// See [`CacheEntry`] for more informations.
///
/// [`CacheEntry`]: struct.CacheEntry.html
#[repr(C)]
struct ContreteCacheEntry<T> {
    data: Box<RwLock<T>>,
    drop: unsafe fn(&mut CacheEntry),
}

impl<T: Send + Sync> ContreteCacheEntry<T> {
    /// Gets a reference to the inner `RwLock`
    #[inline]
    fn get_ref(&self) -> AssetRefLock<T> {
        AssetRefLock { data: &*self.data }
    }

    /// Consumes the `ContreteCacheEntry` to get the inner value.
    #[inline]
    fn into_inner(self) -> T {
        rwlock::into_inner(*self.data)
    }
}

impl<T> fmt::Debug for ContreteCacheEntry<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.data.read().fmt(f)
    }
}

/// A lock on an asset.
///
/// The type parameter `A` represents type of the locked asset.
///
/// This structure wraps a RwLock, so assets can be written to be reloaded.
/// As such, any number of read guard can exist at the same time, but none
/// can exist while reloading an asset.
///
/// This is the structure you want to use to store a reference to an asset.
pub struct AssetRefLock<'a, A> {
    data: &'a RwLock<A>,
}

impl<A> AssetRefLock<'_, A> {
    /// Locks the pointed asset for reading.
    ///
    /// Returns a RAII guard which will release the lock once dropped.
    #[inline]
    pub fn read(&self) -> AssetRef<'_, A> {
        AssetRef {
            guard: rwlock::read(self.data),
        }
    }

    /// Checks if the two assets refer to the same cache entry
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.data, other.data)
    }
}

impl<A> Clone for AssetRefLock<'_, A> {
    fn clone(&self) -> Self {
        Self {
            data: self.data,
        }
    }
}

impl<A> Copy for AssetRefLock<'_, A> {}

impl<A> hash::Hash for AssetRefLock<'_, A>
where
    A: hash::Hash,
{
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        rwlock::read(self.data).hash(state);
    }
}

impl<A> fmt::Debug for AssetRefLock<'_, A>
where
    A: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssetRefLock").field("data", &*rwlock::read(&self.data)).finish()
    }
}

/// RAII guard used to keep a read lock on an asset and release it when dropped.
///
/// It can be obtained by calling [`AssetRefLock::read`].
///
/// [`AssetRefLock::read`]: struct.AssetRefLock.html#method.read
pub struct AssetRef<'a, A> {
    guard: RwLockReadGuard<'a, A>,
}

impl<A> Deref for AssetRef<'_, A> {
    type Target = A;

    #[inline]
    fn deref(&self) -> &A {
        &self.guard
    }
}

impl<A> fmt::Display for AssetRef<'_, A>
where
    A: fmt::Display,
{
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<A> fmt::Debug for AssetRef<'_, A>
where
    A: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}
