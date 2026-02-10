/// The abort handle takes a `FnOnce` callback as argument, which is
/// called as soon the abort-handle is dropped.
///
/// Can be used eg. to wrap 'unregister' functions, to unsubscribe from
/// callback based streams etc.
///
/// To invoke the abort-callback immediately use [AbortHandle::abort_now].
///
/// To drop this [AbortHandle] without calling the included abort-callback
/// use [AbortHandle::detach].
pub struct AbortHandle(Option<Box<dyn FnOnce() + Send>>);
impl AbortHandle {
    /// Creates a new [AbortHandle] with the given `abort_callback` that
    /// will be invoked when this handle is dropped.
    pub fn new(abort_callback: Box<dyn FnOnce() + Send>) -> Self {
        Self(Some(abort_callback))
    }

    /// Invokes the included 'abort' callback immediately, instead of when
    /// this handle is dropped.
    pub fn abort_now(mut self) {
        self.invoke_abort_cb();
    }

    /// The included abort-callback will be dropped without invoking it,
    /// so whatever it should abort will continue running when this
    /// [AbortHandle] is dropped.
    ///
    /// **ATTENTION:** This effectively leaks whatever should be cancelled with the
    /// abort-callback.
    /// Use only for subscriptions that are made to run detached, like subscriptions that will
    /// be cleaned up anyway at some defined point.
    pub fn detach(mut self) {
        let _ = self.0.take();
    }

    fn invoke_abort_cb(&mut self) {
        if let Some(cb) = self.0.take() {
            cb();
        }
    }
}
impl Drop for AbortHandle {
    fn drop(&mut self) {
        self.invoke_abort_cb();
    }
}

/// Thread local variant of [AbortHandle] - the abort callback is not `Send`.
///
/// The abort handle takes a `FnOnce` callback as argument, which is
/// called as soon the abort-handle is dropped.
///
/// Can be used eg. to wrap 'unregister' functions, to unsubscribe from
/// callback based streams etc.
///
/// To invoke the abort-callback immediately use [TlAbortHandle::abort_now].
///
/// To drop this [TlAbortHandle] without calling the included abort-callback
/// use [TlAbortHandle::detach].
pub struct TlAbortHandle(Option<Box<dyn FnOnce()>>);
impl TlAbortHandle {
    /// Creates a new [AbortHandle] with the given `abort_callback` that
    /// will be invoked when this handle is dropped.
    pub fn new(abort_callback: Box<dyn FnOnce()>) -> Self {
        Self(Some(abort_callback))
    }

    /// Invokes the included 'abort' callback immediately, instead of when
    /// this handle is dropped.
    pub fn abort_now(mut self) {
        self.invoke_abort_cb();
    }

    /// The included abort-callback will be dropped without invoking it,
    /// so whatever it should abort will continue running when this
    /// [TlAbortHandle] is dropped.
    ///
    /// **ATTENTION:** This effectively leaks whatever should be cancelled with the
    /// abort-callback.
    /// Use only for subscriptions that are made to run detached, like subscriptions that will
    /// be cleaned up anyway at some defined point.
    pub fn detach(mut self) {
        let _ = self.0.take();
    }

    fn invoke_abort_cb(&mut self) {
        if let Some(cb) = self.0.take() {
            cb();
        }
    }
}
impl Drop for TlAbortHandle {
    fn drop(&mut self) {
        self.invoke_abort_cb();
    }
}
