use std::collections::BinaryHeap;
use std::cmp::Ordering;
use threadpool::ThreadPool;
use std::sync::mpsc;
use error::*;

pub struct OrdParQueue<T> {
    pool: ThreadPool,
    current_index: usize,
    sender: mpsc::SyncSender<ReverseTuple<T>>,
}

pub struct OrdParQueueIter<T> {
    receiver: mpsc::Receiver<ReverseTuple<T>>,
    next_index: usize,
    receive_buffer: BinaryHeap<ReverseTuple<T>>,
}

pub fn new<T>(num_cpus: usize) -> (OrdParQueue<T>, OrdParQueueIter<T>) {
    let (sender, receiver) = mpsc::sync_channel(num_cpus);
    (OrdParQueue {
        pool: ThreadPool::new(num_cpus),
        sender,
        current_index: 0,
    }, OrdParQueueIter {
        receiver,
        next_index: 0,
        receive_buffer: BinaryHeap::new()
    })
}

impl<T: Send + 'static> OrdParQueue<T> {
    pub fn push_sync(&mut self, item: T) -> CatResult<()> {
        self.sender.send(ReverseTuple(self.current_index, item)).map_err(|_| ErrorKind::ThreadSend)?;
        self.current_index += 1;
        Ok(())
    }

    pub fn push<F>(&mut self, async_callback: F) where F: FnOnce() -> T + Send + 'static {
        let tx = self.sender.clone();
        let i = self.current_index;
        self.current_index += 1;
        self.pool.execute(move || {
            let res = async_callback();
            tx.send(ReverseTuple(i, res)).ok(); // ignore error, panic is inappropriate here
        });
    }
}


impl<T> Iterator for OrdParQueueIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        while self.receive_buffer.peek().map(|i|i.0) != Some(self.next_index) {
            match self.receiver.recv() {
                Ok(item) => self.receive_buffer.push(item),
                Err(_) => {
                    // Sender dropped (but continue to dump receive_buffer buffer)
                    break;
                }
            }
        }

        if let Some(item) = self.receive_buffer.pop() {
            self.next_index += 1;
            Some(item.1)
        } else {
            None
        }
    }
}

struct ReverseTuple<T>(usize, T);
impl<T> PartialEq for ReverseTuple<T> {
    fn eq(&self, o: &Self) -> bool { o.0.eq(&self.0) }
}
impl<T> Eq for ReverseTuple<T> {}
impl<T> PartialOrd for ReverseTuple<T> {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> { o.0.partial_cmp(&self.0) }
}
impl<T> Ord for ReverseTuple<T> {
    fn cmp(&self, o: &Self) -> Ordering { o.0.cmp(&self.0) }
}
