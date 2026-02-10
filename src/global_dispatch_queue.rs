use crossbeam_queue::SegQueue;
use std::{
    sync::{Arc, OnceLock},
    thread::{self, JoinHandle},
};

use crate::OnceFunc;

/// A multi purpose single threaded dispatch queue.
///
/// The [GlobalDispatchQueue] uses internally two queues, one for high
/// priority and one for default priority.
///
/// **ATTENTION:** Per thread loop the high priority queue is iterated until empty and as soon empty
/// only one item is taken from the default priority queue.
///
/// So the high priority queue should never be used for high frequency events, otherwise
/// the default priority queue never gets a chance to to process it's items.
pub struct GlobalDispatchQueue(Arc<DQThread>);
impl GlobalDispatchQueue {
    pub fn as_static_ref() -> &'static Self {
        GLOBAL_DISPATCH_QUEUE_STORE_CELL.get_or_init(|| Self(DQThread::new()))
    }

    /// Functions sent with this method are executed before
    /// functions sent with [Self::send_priority_default].
    ///
    /// This should never be used for high frequency invocations, otherwise the default
    /// priority queue never get's a change to process it's items - therefore
    /// use [Self::send_priority_default] instead.
    ///
    /// This can be used for low frequency invocations with higher priority,
    /// like subscribe / unsubscribe to event streams, app lifecycle events etc.
    ///
    /// See [GlobalDispatchQueue] doc comment for details on
    /// how the functions are executed on the dispatch thread.
    ///
    pub fn send_priority_high(&self, func: OnceFunc) {
        self.0.send_high(func);
    }

    /// Functions sent with this method are executed after
    /// functions sent with [Self::send_priority_high].
    ///
    /// See [GlobalDispatchQueue] doc comment for details on
    /// how the functions are executed on the dispatch thread.
    pub fn send_priority_default(&self, func: OnceFunc) {
        self.0.send_default(func);
    }
}
impl Clone for GlobalDispatchQueue {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Cell for the storage of [GlobalDispatchQueue].
static GLOBAL_DISPATCH_QUEUE_STORE_CELL: OnceLock<GlobalDispatchQueue> = OnceLock::new();

struct DQThread((Arc<Queues>, JoinHandle<()>));
impl DQThread {
    fn new() -> Arc<Self> {
        let queues = Queues::new();
        let queues_cloned: Arc<Queues> = queues.clone();
        let handle = thread::Builder::new()
            .stack_size(128 * 1024)
            .name(format!("glob-dq-thread"))
            .spawn(move || Self::run(queues))
            .expect("cannot spawn global dispatch-queue thread");

        Arc::new(Self((queues_cloned, handle)))
    }

    fn send_high(&self, item: OnceFunc) {
        self.0 .0.high.push(item);
        self.0 .1.thread().unpark();
    }

    fn send_default(&self, item: OnceFunc) {
        self.0 .0.default.push(item);
        self.0 .1.thread().unpark();
    }

    fn run(queues: Arc<Queues>) {
        loop {
            let mut q1_empty = true;
            let mut q2_empty = true;

            while let Some(high_fn) = queues.high.pop() {
                q1_empty = false;
                high_fn();
            }

            if let Some(default_fn) = queues.default.pop() {
                q2_empty = false;
                default_fn();
            }

            if q1_empty && q2_empty {
                thread::park();
            }
        }
    }
}

struct Queues {
    high: SegQueue<OnceFunc>,
    default: SegQueue<OnceFunc>,
}
impl Queues {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            high: SegQueue::new(),
            default: SegQueue::new(),
        })
    }
}
