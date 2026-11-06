//! A signal that wakes a waiting task, for the vault watcher and the agent door.

use std::sync::atomic::{AtomicBool, Ordering};

/// A signal that wakes the task waiting on it, without it having to poll: the system's
/// thread raises it, the task sleeps until then.
#[derive(Default)]
pub struct Signal {
    raised: AtomicBool,
    waker: std::sync::Mutex<Option<std::task::Waker>>,
}

impl Signal {
    pub fn raise(&self) {
        self.raised.store(true, Ordering::Release);
        if let Some(waker) = self.waker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            waker.wake();
        }
    }

    /// True, and the signal drops, if it was raised since the last call.
    pub fn take(&self) -> bool {
        self.raised.swap(false, Ordering::AcqRel)
    }

    /// Waits until the signal is raised (true) or `deadline` passes (false).
    pub async fn wait<D: std::future::Future<Output = ()> + Unpin>(&self, mut deadline: D) -> bool {
        std::future::poll_fn(|cx| {
            if self.take() {
                return std::task::Poll::Ready(true);
            }
            *self.waker.lock().unwrap_or_else(|e| e.into_inner()) = Some(cx.waker().clone());
            // Raised in the meantime, before the waker is in place: it is not missed.
            if self.take() {
                return std::task::Poll::Ready(true);
            }
            std::pin::Pin::new(&mut deadline).poll(cx).map(|()| false)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Runs a future on the current thread, which sleeps between wakeups.
    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        struct Unpark(std::thread::Thread);
        impl std::task::Wake for Unpark {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = Arc::new(Unpark(std::thread::current())).into();
        let mut cx = std::task::Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
            std::thread::park_timeout(std::time::Duration::from_secs(5));
        }
    }

    #[test]
    fn signal_wakes_a_sleeping_task() {
        let signal = Arc::new(Signal::default());
        // Raised before the wait: no sleep. Dropped afterwards.
        signal.raise();
        assert!(block_on(signal.wait(std::future::pending())));
        assert!(!signal.take());
        // Deadline passed, no signal: false.
        assert!(!block_on(signal.wait(std::future::ready(()))));
        // Raised by another thread while the task sleeps.
        let other = signal.clone();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            other.raise();
        });
        let start = std::time::Instant::now();
        assert!(block_on(signal.wait(std::future::pending())));
        assert!(start.elapsed() < std::time::Duration::from_secs(4));
        thread.join().unwrap();
    }
}
