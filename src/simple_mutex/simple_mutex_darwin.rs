#![allow(unsafe_code, dead_code)]

use core::cell::UnsafeCell;
use core::default::Default;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

#[allow(non_camel_case_types)]
pub mod sys {
    #[repr(C)]
    pub struct os_unfair_lock(pub u32);

    pub type os_unfair_lock_t = *mut os_unfair_lock;
    pub type os_unfair_lock_s = os_unfair_lock;

    pub const OS_UNFAIR_LOCK_INIT: os_unfair_lock = os_unfair_lock(0);

    unsafe extern "C" {
        // part of libSystem, no link needed
        pub fn os_unfair_lock_lock(lock: os_unfair_lock_t);
        pub fn os_unfair_lock_unlock(lock: os_unfair_lock_t);
        pub fn os_unfair_lock_trylock(lock: os_unfair_lock_t) -> bool;
        pub fn os_unfair_lock_assert_owner(lock: os_unfair_lock_t);
        pub fn os_unfair_lock_assert_not_owner(lock: os_unfair_lock_t);
    }
}

/// Simple lightweight mutex implementation for apple targets **without timeout or fairness guarantee**.
///
/// Uses apple's `os_unfair_lock`.
pub struct SMutex<T: ?Sized> {
    lock: UnsafeCell<sys::os_unfair_lock>,
    cell: UnsafeCell<T>,
}

struct CantSendMutexGuardBetweenThreads;

pub struct SMutexGuard<'a, T: ?Sized> {
    mutex: &'a SMutex<T>,
    // could just be *const (), but this produces a better error message
    pd: PhantomData<*const CantSendMutexGuardBetweenThreads>,
}

unsafe impl<T: ?Sized + Send> Sync for SMutex<T> {}
unsafe impl<T: ?Sized + Send> Send for SMutex<T> {}

impl<T: ?Sized> SMutex<T> {
    #[inline]
    pub const fn new(value: T) -> Self
    where
        T: Sized,
    {
        SMutex {
            lock: UnsafeCell::new(sys::OS_UNFAIR_LOCK_INIT),
            cell: UnsafeCell::new(value),
        }
    }
    #[inline]
    pub fn lock<'a>(&'a self) -> SMutexGuard<'a, T> {
        unsafe {
            sys::os_unfair_lock_lock(self.lock.get());
        }
        SMutexGuard {
            mutex: self,
            pd: PhantomData,
        }
    }
    #[inline]
    pub fn try_lock<'a>(&'a self) -> Option<SMutexGuard<'a, T>> {
        let ok = unsafe { sys::os_unfair_lock_trylock(self.lock.get()) };
        if ok {
            Some(SMutexGuard {
                mutex: self,
                pd: PhantomData,
            })
        } else {
            None
        }
    }
    #[inline]
    fn assert_not_owner(&self) {
        unsafe {
            sys::os_unfair_lock_assert_not_owner(self.lock.get());
        }
    }
}

// It's (potentially) Sync but not Send, because os_unfair_lock_unlock must be called from the
// locking thread.
unsafe impl<'a, T: ?Sized + Sync> Sync for SMutexGuard<'a, T> {}

impl<'a, T: ?Sized> Deref for SMutexGuard<'a, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.cell.get() }
    }
}

impl<'a, T: ?Sized> DerefMut for SMutexGuard<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.cell.get() }
    }
}

impl<'a, T: ?Sized> Drop for SMutexGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            sys::os_unfair_lock_unlock(self.mutex.lock.get());
        }
    }
}

impl<T: ?Sized + Default> Default for SMutex<T> {
    #[inline]
    fn default() -> Self {
        SMutex::new(T::default())
    }
}

impl<T> From<T> for SMutex<T> {
    #[inline]
    fn from(t: T) -> SMutex<T> {
        SMutex::new(t)
    }
}

#[cfg(test)]
mod tests {
    use super::SMutex;
    const TEST_CONST: SMutex<u32> = SMutex::new(42);
    #[test]
    fn basics() {
        let m = TEST_CONST;
        *m.lock() += 1;
        {
            let mut g = m.try_lock().unwrap();
            *g += 1;
            assert!(m.try_lock().is_none());
        }
        m.assert_not_owner();
        assert_eq!(*m.lock(), 44);
    }
}
