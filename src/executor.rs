use crate::Error;
use std::future::Future;
use std::marker::Send;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Mutex, TryLockError, Arc, Condvar};
use std::task::{Context, Poll};
use std::thread::{self, available_parallelism};
use std::time::{Duration, Instant};

pub(crate) type BoxFuture<'a, T> = Box<dyn Future<Output = T> + Send + 'a>;

struct ExecutorWake {
    earliest_woken: AtomicUsize,
    condvar: Condvar,
}

pub struct Executor<'a> {
    w: Arc<ExecutorWake>,
    tasks: Vec<(&'static str, Mutex<(Duration, Option<(Pin<BoxFuture<'a, Result<(), Error>>>, std::task::Waker)>)>)>,
}

impl<'a> Executor<'a> {
    pub fn new() -> Self {
        Executor {
            w: Arc::new(ExecutorWake {
                earliest_woken: AtomicUsize::new(usize::MAX),
                condvar: Condvar::new(),
            }),
            tasks: Vec::new(),
        }
    }

    pub fn add_task(&mut self, name: &'static str, future: impl Future<Output = Result<(), Error>> + 'a + Send) {
        let current_future_index = self.tasks.len();
        let w = Arc::clone(&self.w);
        let waker = waker_fn::waker_fn(move || {
            w.earliest_woken.fetch_min(current_future_index, Relaxed); // re-poll
            w.condvar.notify_one(); // break wait of some thread to re-poll quicker if possible
        });

        self.tasks.push((name, Mutex::new((Duration::ZERO, Some((Box::pin(future), waker))))));
    }

    pub(crate) fn execute_a_bunch(&self) -> Result<(), Error> {
        let err = Mutex::new(None);
        let waiter = Mutex::new(());
        thread::scope(|s| {
            let worker = || {
                'outer: loop {
                    let mut finished_tasks = 0;
                    let mut busy_tasks = 0;
                    for (current_task, (_, t)) in self.tasks.iter().enumerate() {
                        match t.try_lock() {
                            Ok(mut t) => if let Some((fut, waker)) = t.1.as_mut() {
                                let mut ctx = Context::from_waker(&waker);
                                let start = Instant::now();
                                let pol_res = Future::poll(fut.as_mut(), &mut ctx);
                                t.0 += start.elapsed();
                                match pol_res {
                                    Poll::Pending => {
                                        if self.w.earliest_woken.load(Relaxed) < current_task {
                                            // intentional race to avoid frequent writes,
                                            // harmless, because this thread will poll anyway even if other wakers ran now
                                            self.w.earliest_woken.store(usize::MAX, Relaxed);
                                            continue 'outer;
                                        }
                                    },
                                    Poll::Ready(res) => {
                                        t.1 = None; // ensure it won't be polled again
                                        self.w.condvar.notify_all(); // this may have been the last job, so make other threads check for exit condition
                                        if let Err(e) = res {
                                            // first to fail sets the result. TODO combine errors smartly, maybe disconnected channels err too soon
                                            err.lock().unwrap().get_or_insert(e);
                                            return;
                                        }
                                        continue;
                                    },
                                }
                            } else {
                                finished_tasks += 1;
                            },
                            Err(TryLockError::WouldBlock) => {
                                busy_tasks += 1;
                                continue
                            },
                            Err(TryLockError::Poisoned(_)) => {
                                err.lock().unwrap().get_or_insert(Error::ThreadSend);
                                return;
                            },
                        };
                    }
                    if finished_tasks == self.tasks.len() {
                        break;
                    }
                    if self.w.earliest_woken.load(Relaxed) == usize::MAX {
                        let (_, timedout) = self.w.condvar.wait_timeout(waiter.lock().unwrap(), Duration::from_secs(5)).unwrap();
                        if timedout.timed_out() {
                            eprintln!("••• OOOOF from busy {busy_tasks} finished {finished_tasks} total {}", self.tasks.len());
                        }
                    }
                }
            };

            let threads = available_parallelism().map(|t| t.get().max(2)).unwrap_or(8);
            for n in 0..threads {
                thread::Builder::new().name(format!("t{n}")).spawn_scoped(s, worker.clone()).unwrap();
            }

        });
        for (n, (name, t)) in self.tasks.iter().enumerate() {
            let res = t.lock().unwrap();
            eprintln!("task {name}{n} used {}ms time", res.0.as_millis());
        }
        if let Ok(Some(e)) = err.into_inner() {
            Err(e)
        } else {
            Ok(())
        }
    }

}
