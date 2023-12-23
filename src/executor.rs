use crate::Error;
use std::future::Future;
use std::marker::Send;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Relaxed, Acquire, Release};
use std::sync::{Mutex, TryLockError, Arc, Condvar};
use std::task::{Context, Poll};
use std::thread::{self, available_parallelism};
use std::time::{Duration, Instant};

pub(crate) type SendFuture<'a, T> = Box<dyn Future<Output = T> + Send + 'a>;
pub(crate) type LocalFuture<'a, T> = Box<dyn Future<Output = T> + 'a>;

struct Task<F> {
    task_id: usize,
    used: Duration,
    future: Option<(Pin<F>, std::task::Waker)>,
}

struct ExecutorWake {
    latest_woken_task_id: AtomicUsize,
    condvar: Condvar,
}

pub(crate) struct Executor<'a> {
    w: Arc<ExecutorWake>,
    local_w: Arc<ExecutorWake>,
    send_tasks: Vec<(&'static str, Mutex<Task<SendFuture<'a, Result<(), Error>>>>)>,
    local_tasks: Vec<(&'static str, Mutex<Task<LocalFuture<'a, Result<(), Error>>>>)>,
}

impl<'a> Executor<'a> {
    pub fn new() -> Self {
        Executor {
            w: Arc::new(ExecutorWake {
                latest_woken_task_id: AtomicUsize::new(0),
                condvar: Condvar::new(),
            }),
            local_w: Arc::new(ExecutorWake {
                latest_woken_task_id: AtomicUsize::new(0),
                condvar: Condvar::new(),
            }),
            send_tasks: Vec::new(),
            local_tasks: Vec::new(),
        }
    }

    #[inline]
    pub fn add_tasks<F>(&mut self, name: &'static str, n: usize, make_future: impl Fn() -> F) where F: Future<Output = Result<(), Error>> + 'a + Send {
        for _ in 0..n {
            self.add_task(name, make_future())
        }
    }

    pub fn add_task<F>(&mut self, name: &'static str, future: F) where F: Future<Output = Result<(), Error>> + 'a + Send {
        let task_id = 1 + self.send_tasks.len() + self.local_tasks.len();
        let w = Arc::clone(&self.w);
        let waker = waker_fn::waker_fn(move || {
            w.latest_woken_task_id.fetch_max(task_id, Relaxed); // re-poll
            w.condvar.notify_one(); // break wait of some thread to re-poll quicker if possible
        });
        self.send_tasks.push((name, Mutex::new(Task {
            task_id,
            used: Duration::ZERO,
            future: Some((Box::pin(future), waker)),
        })));
    }

    #[inline]
    pub fn add_local_task(&mut self, name: &'static str, future: impl Future<Output = Result<(), Error>> + 'a) {
        let task_id = 1 + self.send_tasks.len() + self.local_tasks.len();
        let w = Arc::clone(&self.local_w);
        let waker = waker_fn::waker_fn(move || {
            w.latest_woken_task_id.fetch_max(task_id, Relaxed); // re-poll
            w.condvar.notify_one(); // break wait of some thread to re-poll quicker if possible
        });
        self.local_tasks.push((name, Mutex::new(Task {
            task_id,
            used: Duration::ZERO,
            future: Some((Box::pin(future), waker)),
        })));
    }

    /// This polls futures in their priority order (later tasks polled first)
    /// and is for CPU-bound futures. Blocking is good!
    #[inline(never)]
    fn worker_inner<F>(w: &ExecutorWake, tasks: &[(&'static str, Mutex<Task<F>>)], err: &Mutex<Option<Error>>, idle_waiter: &Mutex<()>) where F: Deref, F: DerefMut, <F as Deref>::Target: Future<Output = Result<(), Error>> {
        'outer: loop {
            let mut finished_tasks = 0;
            // poll from the last task to avoid starting new frames before old ones finished
            for (_, task) in tasks.iter().rev() {
                // busy tasks stay locked while polled
                let mut task = match task.try_lock() {
                    Ok(t) => t,
                    Err(TryLockError::WouldBlock) => {
                        continue;
                    },
                    Err(TryLockError::Poisoned(_)) => {
                        set_err(err, Error::ThreadSend);
                        w.condvar.notify_all();
                        return;
                    },
                };
                if let Some((future, waker)) = task.future.as_mut() {
                    let future = future.as_mut();
                    let start = Instant::now();
                    let poll_result = Future::poll(future, &mut Context::from_waker(&waker));
                    task.used += start.elapsed();
                    match poll_result {
                        Poll::Pending => {
                            let r = w.latest_woken_task_id.load(Acquire);
                            // if a higher-priority task is ready, restart polling from the highest-priority tasks
                            if r >= task.task_id {
                                // intentional race to avoid frequent writes,
                                // harmless, because this thread will poll anyway even if other wakers ran now
                                w.latest_woken_task_id.store(0, Release);
                                continue 'outer;
                            }
                        },
                        Poll::Ready(finished) => {
                            task.future = None; // ensure it won't be polled again
                            let has_failed = finished.is_err();
                            if let Err(e) = finished {
                                // first to fail sets the result. TODO combine errors smartly, maybe disconnected channels err too soon
                                set_err(err, e);
                            }
                            drop(task);
                            w.condvar.notify_all(); // this may have been the last job, so make other threads check for exit condition
                            if has_failed {
                                return;
                            } else {
                                continue;
                            }
                        },
                    }
                } else {
                    finished_tasks += 1;
                }
            }
            if finished_tasks == tasks.len() {
                // no need to wake threads here, since all are woken on every finish
                return;
            }
            // no tasks ready to be polled again?
            if w.latest_woken_task_id.load(Acquire) == 0 {
                // timeout here is only because my executor is crappy
                let _ = w.condvar.wait_timeout(idle_waiter.lock().unwrap(), Duration::from_secs(1)).unwrap();
            }
        }
    }

    pub(crate) fn execute_all(&self) -> Result<(), Error> {
        let err = Mutex::new(None);

        let tasks = self.send_tasks.as_slice();
        let idle_waiter = &Mutex::new(());
        let w = &*self.w;

        let local_tasks = self.local_tasks.as_slice();
        let local_idle_waiter = &Mutex::new(());
        let local_w = &*self.local_w;

        thread::scope(|s| {
            let worker = || {
                Self::worker_inner(w, tasks, &err, idle_waiter);
            };

            let threads = available_parallelism().map(|t| t.get().max(2)).unwrap_or(8).min(tasks.len());
            for n in 0..threads {
                thread::Builder::new().name(format!("t{n}")).spawn_scoped(s, worker.clone()).unwrap();
            }

            Self::worker_inner(local_w, local_tasks, &err, local_idle_waiter);
        });

        if let Some(e) = err.into_inner().map_err(|_| Error::ThreadSend)? {
            Err(e)
        } else {
            Ok(())
        }
    }
}

#[cold]
fn set_err(err: &Mutex<Option<Error>>, e: Error) {
    if let Ok(mut err) = err.lock() {
        err.get_or_insert(e);
    }
}
