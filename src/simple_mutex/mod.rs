//! Simple lightweight mutex implementation.
//!
//! Attention: This mutex implementation is lightweight because it is totally unfair, has no deadlock detection
//! and lacks all features known from the std and parking-lot versions.
//! Should only be used for short locks where a mutex can not be replaced by a alternative.
//!
//! Uses apple's `os_unfair_lock` for iOS / macOS targets and atomics with `wait` and `wake_one` from the
//! `atomic_wait` crate for all other targets.

#[cfg(any(target_vendor = "apple", test))]
mod simple_mutex_darwin;

#[cfg(any(not(target_vendor = "apple"), test))]
mod simple_mutex_default;

#[cfg(not(target_vendor = "apple"))]
pub use simple_mutex_default::{SMutex, SMutexGuard};

#[cfg(target_vendor = "apple")]
pub use simple_mutex_darwin::{SMutex, SMutexGuard};
