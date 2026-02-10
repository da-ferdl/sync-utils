mod abort_handle;
mod global_dispatch_queue;
mod simple_mutex;
mod timer;

pub use abort_handle::{AbortHandle, TlAbortHandle};
pub use global_dispatch_queue::GlobalDispatchQueue;
pub use simple_mutex::{SMutex, SMutexGuard};
pub use timer::Timer;

/// Common boxed callback type declaration for callback functions that can be
/// invoked only once.
pub type OnceFunc = Box<dyn FnOnce() + Send>;

/// Thread local variant of [OnceFunc] - `!Send`.
pub type TlOnceFunc = Box<dyn FnOnce()>;

/// Common boxed callback type declaration for callback functions that can be
/// invoked repeatedly.
pub type RepeatingFunc = Box<dyn FnMut() + Send>;
