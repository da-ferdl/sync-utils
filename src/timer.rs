use crate::{OnceFunc, RepeatingFunc};
use std::{
    sync::{Arc, OnceLock, mpsc},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const TIMER_CHANNEL_ERROR_MSG: &str = "Timer channel should never disconnect";
const TIME_NOW_ERROR_MSG: &str = "Time.now duration since unix epoch should never fail";

static TIMER_CHANNEL_SENDER_CELL: OnceLock<mpsc::Sender<Option<TimerTask>>> = OnceLock::new();
fn get_timer_channel_sender_ref() -> &'static mpsc::Sender<Option<TimerTask>> {
    &TIMER_CHANNEL_SENDER_CELL.get_or_init(|| {
        let (sender, receiver) = mpsc::channel::<Option<TimerTask>>();

        let sender_copy = sender.clone();
        thread::Builder::new()
            .stack_size(256 * 1024)
            .name("r-timer".into())
            .spawn(move || TimerHandler::run(receiver, sender_copy))
            .expect("failed to spawn timer thread");

        sender
    })
}

/// Timer to execute a callback either once or repeatedly:
///
/// - [Timer::schedule_once] - executes the callback once after a the delay [Duration]
/// - [Timer::schedule_repeating] - executes the callback repeatedly with the repeat [Duration]
/// until cancelled.
///
/// This [Timer] is cancelled when dropped (for `once` timers if not done already or detached).
///
/// To cancel this [Timer] immediately use [Timer::cancel].
///
/// To executed a delayed callback once 'detached' - executed in any case without cancel handle,
/// use [Timer::schedule_once_detached].
///
/// ### **Note:**
/// - **The smallest delay / repeat duration unit is 1 microsecond and the timer works in microsecond steps.**
/// - **All timers use the same dedicated `timer` thread, so the provided callbacks should not block / run for longer time.**
pub struct Timer {
    /// The `Arc` serves as flag to use the strong count as closed indication.
    /// It is shared between this [Timer] and the associated [TimerTask].
    closed_flag: Option<Arc<()>>,
    sender: mpsc::Sender<Option<TimerTask>>,
}
impl Timer {
    /// Executes the [OnceFunc] once after the given delay duration if not cancelled before the
    /// duration has elapsed.
    ///
    /// The returned [Timer] is cancelled when dropped and can be cancelled immediately
    /// with [Timer::cancel], if not already done.
    pub fn schedule_once(delay: Duration, func: OnceFunc) -> Self {
        Self::impl_self(
            FuncType::Once(Some(func)),
            dur_min_adjusted_as_micros(delay),
        )
    }

    /// Executes the [OnceFunc] once after the given delay duration.
    ///
    /// **Attention:** This returns no [Timer] handle, so the execution can not be cancelled.
    pub fn schedule_once_detached(delay: Duration, func: OnceFunc) {
        let duration_micros = dur_min_adjusted_as_micros(delay);

        let now_micros = now_as_micros();

        let _ = get_timer_channel_sender_ref().send(
            TimerTask {
                closed_flag: None,
                invoke_at: now_micros + duration_micros,
                is_once_detached: false,
                func: FuncType::Once(Some(func)),
            }
            .into(),
        );
    }

    /// Executes the [RepeatingFunc] repeatedly with the given repeat duration until cancelled.
    ///
    /// The returned [Timer] is cancelled when dropped and can be cancelled immediately
    /// with [Timer::cancel].
    pub fn schedule_repeating(repeat: Duration, func: RepeatingFunc) -> Self {
        let duration_micros = dur_min_adjusted_as_micros(repeat);
        Self::impl_self(FuncType::Repeating(duration_micros, func), duration_micros)
    }

    /// Cancels the timer.
    ///
    /// Noop for once timers that are already done.
    pub fn cancel(mut self) {
        self.close();
    }

    fn impl_self(fn_type: FuncType, duration_micros: u128) -> Self {
        let shared_closed_arc = Arc::new(());

        let this = Self {
            closed_flag: Some(shared_closed_arc.clone()),
            sender: get_timer_channel_sender_ref().clone(),
        };

        let now_micros = now_as_micros();

        let _ = this.sender.send(
            TimerTask {
                closed_flag: Some(shared_closed_arc),
                invoke_at: now_micros + duration_micros,
                is_once_detached: false,
                func: fn_type,
            }
            .into(),
        );

        this
    }

    fn close(&mut self) {
        if let Some(c) = self.closed_flag.take() {
            // If strong count is one the timer is either already done or closed.
            if Arc::strong_count(&c) == 1 {
                return;
            }
            drop(c);
            let _ = self.sender.send(None);
        }
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        self.close();
    }
}

enum FuncType {
    Once(Option<OnceFunc>),
    /// `u128` is the repeat duration as microseconds.
    Repeating(u128, RepeatingFunc),
}

struct TimerTask {
    /// The `Arc` serves as flag to use the strong count as closed indication.
    /// It is shared between this [TimerTask] and the associated [Timer].
    closed_flag: Option<Arc<()>>,
    /// Indicates if this is a detached once callback - not cancelable.
    is_once_detached: bool,
    /// Invoke-at is time.now + repeat / delay duration in microseconds.
    invoke_at: u128,
    func: FuncType,
}
impl TimerTask {
    fn is_closed(&self) -> bool {
        if self.is_once_detached {
            return false;
        }

        if let Some(c) = &self.closed_flag {
            // If strong count is one the timer is either already done or closed.
            if Arc::strong_count(c) > 1 {
                return false;
            }
        }

        true
    }

    fn close(&mut self) {
        if let Some(c) = self.closed_flag.take() {
            drop(c);
        }
    }
}

struct TimerHandler {
    receiver: mpsc::Receiver<Option<TimerTask>>,
    sender: mpsc::Sender<Option<TimerTask>>,
    delay_tasks: Vec<TimerTask>,
}
impl TimerHandler {
    fn run(receiver: mpsc::Receiver<Option<TimerTask>>, sender: mpsc::Sender<Option<TimerTask>>) {
        let mut this = Self {
            receiver,
            sender,
            delay_tasks: Vec::with_capacity(1000),
        };

        let mut next_type = NextType::Wait;

        loop {
            match &next_type {
                NextType::Wait => next_type = this.wait(),
                NextType::WaitWithTimeout(duration) => next_type = this.wait_with_timeout(duration),
            }
        }
    }

    fn wait(&mut self) -> NextType {
        match self.receiver.recv() {
            Ok(v) => self.handle_signal(v),
            Err(_) => unreachable!("{TIMER_CHANNEL_ERROR_MSG}"),
        }
    }

    fn wait_with_timeout(&mut self, dur: &Duration) -> NextType {
        match self.receiver.recv_timeout(*dur) {
            Ok(v) => self.handle_signal(v),
            Err(e) => match e {
                mpsc::RecvTimeoutError::Timeout => self.handle_signal(None),
                mpsc::RecvTimeoutError::Disconnected => unreachable!("{TIMER_CHANNEL_ERROR_MSG}"),
            },
        }
    }

    fn handle_signal(&mut self, task: Option<TimerTask>) -> NextType {
        if let Some(v) = task {
            self.delay_tasks.push(v);
        }

        let mut next_wait_timeout_micros: Option<u128> = None;

        self.delay_tasks.retain_mut(|v| {
            if v.is_closed() {
                return false;
            }

            let now_micros = now_as_micros();

            let mut once_cb = if v.invoke_at <= now_micros {
                match &mut v.func {
                    FuncType::Once(fn_slot) => fn_slot.take(),
                    FuncType::Repeating(repeat_micros, cb) => {
                        v.invoke_at = now_micros + *repeat_micros;
                        cb();
                        None
                    }
                }
            } else {
                None
            };

            if let Some(cb) = once_cb.take() {
                // close the once task so the associated timer needs to do nothing on drop.
                v.close();
                cb();

                // It's a once timer which is now done - return immediately with remove indication.
                return false;
            }

            match &next_wait_timeout_micros {
                Some(t) => {
                    if v.invoke_at < *t {
                        let _ = next_wait_timeout_micros.replace(v.invoke_at);
                    }
                }
                None => {
                    let _ = next_wait_timeout_micros.replace(v.invoke_at);
                }
            };

            true
        });

        match next_wait_timeout_micros {
            Some(next_wait_micros) => {
                let now_micros = now_as_micros();

                if next_wait_micros <= now_micros {
                    // Go through the channel instead of calling 'handle_signal',
                    // so other timer instances that where sent in the meanwhile have
                    // a chance to be set on the queue.
                    let _ = self.sender.send(None);
                    NextType::Wait
                } else {
                    let wait_duration_micros = next_wait_micros - now_micros;
                    NextType::WaitWithTimeout(Duration::from_micros(wait_duration_micros as u64))
                }
            }
            None => NextType::Wait,
        }
    }
}

enum NextType {
    Wait,
    WaitWithTimeout(Duration),
}

/// Returns the given duration as milliseconds.
///
/// Defaults to 1 microsecond if the given duration is smaller.
fn dur_min_adjusted_as_micros(duration: Duration) -> u128 {
    let micros = duration.as_micros();

    if micros == 0 {
        return 1;
    }

    micros
}

/// Returns time.now as microseconds.
fn now_as_micros() -> u128 {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect(TIME_NOW_ERROR_MSG);

    dur.as_micros()
}
