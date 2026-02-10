#![allow(unsafe_code, dead_code)]

use atomic_wait::{wait, wake_one};
use std::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{
        AtomicU32,
        Ordering::{Acquire, Relaxed, Release},
    },
};

/// Simple lightweight mutex implementation for non apple targets **without timeout or fairness guarantee**.
///
/// Uses atomics with `wait` and `wake_one` from the `atomic_wait` crate.
#[derive(Default)]
pub struct SMutex<T> {
    /// 0: unlocked
    /// 1: locked, no other threads waiting
    /// 2: locked, other threads waiting
    state: AtomicU32,
    value: UnsafeCell<T>,
}
unsafe impl<T> Sync for SMutex<T> where T: Send {}

impl<T> SMutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            state: AtomicU32::new(0), // unlocked state
            value: UnsafeCell::new(value),
        }
    }

    pub fn try_lock(&self) -> Option<SMutexGuard<'_, T>> {
        if self.state.compare_exchange(0, 1, Acquire, Relaxed).is_err() {
            // The lock was already locked
            return None;
        }
        Some(SMutexGuard { mutex: self })
    }

    pub fn lock(&self) -> SMutexGuard<'_, T> {
        if self.state.compare_exchange(0, 1, Acquire, Relaxed).is_err() {
            // The lock was already locked, we have to wait.
            wait_lock_contended(&self.state);
        }
        SMutexGuard { mutex: self }
    }
}

#[cold]
fn wait_lock_contended(state: &AtomicU32) {
    let mut spin_count = 0;

    while state.load(Relaxed) == 1 && spin_count < 10 {
        spin_count += 1;

        if spin_count <= 3 {
            std::hint::spin_loop();
        } else {
            std::thread::yield_now();
        }
    }

    if state.compare_exchange(0, 1, Acquire, Relaxed).is_ok() {
        return;
    }

    while state.swap(2, Acquire) != 0 {
        wait(state, 2);
    }
}

pub struct SMutexGuard<'a, T> {
    mutex: &'a SMutex<T>,
}

impl<T> Drop for SMutexGuard<'_, T> {
    fn drop(&mut self) {
        // After unlock 'wake' call is only needed if state is '2' - other threads waiting.
        if self.mutex.state.swap(0, Release) == 2 {
            wake_one(&self.mutex.state);
        }
    }
}

impl<T> Deref for SMutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for SMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.value.get() }
    }
}

// TODO: Enable the cond var as soon the default implementation of simple mutex can be used
// on iOS - as soon we support only iOS 14+ - right now the cond var is anyway not needed.
/*// Condition variable for the usage with [SMutex].
///
/// Only one waiter supported, so only `notify_one` is provided.
pub struct SCondvar {
    counter: AtomicU32,
}

impl SCondvar {
    pub const fn new() -> Self {
        Self {
            counter: AtomicU32::new(0),
        }
    }

    pub fn notify_one(&self) {
        self.counter.fetch_add(1, Relaxed);
        wake_one(&self.counter);
    }

    pub fn wait<'a, T>(&self, guard: SMutexGuard<'a, T>) -> SMutexGuard<'a, T> {
        let counter_value = self.counter.load(Relaxed);

        // Unlock the mutex by dropping the guard,
        // but remember the mutex so we can lock it again later.
        let mutex = guard.mutex;
        drop(guard);

        // Waits, but only if the counter hasn't changed since unlocking.
        wait(&self.counter, counter_value);

        mutex.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    #[test]
    fn test_condvar() {
        let mutex = SMutex::new(0);
        let condvar = SCondvar::new();

        let mut wakeups = 0;

        thread::scope(|s| {
            s.spawn(|| {
                thread::sleep(Duration::from_secs(1));
                *mutex.lock() = 123;
                condvar.notify_one();
            });

            let mut m = mutex.lock();
            while *m < 100 {
                m = condvar.wait(m);
                wakeups += 1;
            }

            assert_eq!(*m, 123);
        });

        // Check that the main thread actually did wait (not busy-loop),
        // while still allowing for a few spurious wake ups.
        assert!(wakeups < 10);
    }
}
*/
