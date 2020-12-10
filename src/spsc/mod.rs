use std::{
    cell::UnsafeCell,
    fmt::Debug,
    mem::MaybeUninit,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

const SHIFT: usize = (std::mem::size_of::<AtomicUsize>() * 8) - 1;
const CLOSED_BIT: usize = 1 << SHIFT;

struct Elem<T> {
    data: UnsafeCell<MaybeUninit<T>>,
}

impl<T> Elem<T> {
    fn uninit() -> Self {
        Self {
            data: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
}

pub enum PushError<T> {
    Full(T),
    Closed(T),
}

impl<T> Debug for PushError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PushError::Full(_) => write!(f, "PushError::Full(..)"),
            PushError::Closed(_) => write!(f, "PushError::Closed(..)"),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum PopError {
    Empty,
    Closed,
}

struct Queue<T> {
    /// pop modify the head
    head: AtomicUsize,
    /// push modify the tail
    tail: AtomicUsize,
    buffer: Box<[Elem<T>]>,
}

struct Receiver<T> {
    queue: Arc<Queue<T>>,
}

impl<T> Receiver<T> {
    pub fn pop(&mut self) -> Result<T, PopError> {
        self.queue.pop()
    }
}

struct Sender<T> {
    queue: Arc<Queue<T>>,
}

impl<T> Sender<T> {
    pub fn push(&mut self, value: T) -> Result<(), PushError<T>> {
        self.queue.push(value)
    }
}

unsafe impl<T> Send for Sender<T> {}
unsafe impl<T> Send for Receiver<T> {}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        self.queue.set_closed();
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        self.queue.set_closed();
    }
}

impl<T> Queue<T> {
    fn new_queue(capacity: usize) -> Self {
        assert!(capacity > 0);

        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(Elem::uninit())
        }

        Queue {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            buffer: buffer.into_boxed_slice(),
        }
    }

    #[allow(clippy::new_ret_no_self)]
    pub fn new(capacity: usize) -> (Sender<T>, Receiver<T>) {
        let queue = Arc::new(Self::new_queue(capacity));

        (
            Sender {
                queue: Arc::clone(&queue),
            },
            Receiver { queue },
        )
    }

    fn set_closed(&self) {
        self.tail
            .fetch_update(Ordering::Release, Ordering::Relaxed, |tail| {
                Some(tail | CLOSED_BIT)
            })
            .unwrap();
    }

    pub fn push(&self, elem: T) -> Result<(), PushError<T>> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);

        let is_closed = tail & CLOSED_BIT != 0;
        let tail = tail & !CLOSED_BIT;
        let buffer_length = self.buffer.len();

        if is_closed {
            Err(PushError::Closed(elem))
        } else if head.wrapping_add(buffer_length) == tail {
            Err(PushError::Full(elem))
        } else {
            let index = tail % buffer_length;

            let data = self.buffer[index].data.get();
            unsafe {
                data.write(MaybeUninit::new(elem));
            }

            self.tail.store(tail.wrapping_add(1), Ordering::Release);

            Ok(())
        }
    }

    pub fn pop(&self) -> Result<T, PopError> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        let is_closed = tail & CLOSED_BIT != 0;
        let tail = tail & (!CLOSED_BIT);

        if head == tail {
            if is_closed {
                Err(PopError::Closed)
            } else {
                Err(PopError::Empty)
            }
        } else {
            let index = head % self.buffer.len();

            let data = self.buffer[index].data.get();
            let data = unsafe { data.read().assume_init() };

            self.head.store(head.wrapping_add(1), Ordering::Release);

            Ok(data)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{PopError, PushError, Queue};

    #[test]
    fn simple() {
        let queue = Queue::new_queue(5);

        queue.push(1).unwrap();
        queue.push(2).unwrap();

        assert_eq!(queue.pop().unwrap(), 1);
        assert_eq!(queue.pop().unwrap(), 2);

        assert_eq!(queue.pop(), Err(PopError::Empty));
    }

    #[test]
    fn full() {
        let queue = Queue::new_queue(2);

        queue.push(1).unwrap();
        queue.push(2).unwrap();

        assert!(queue.push(3).is_err());
    }

    #[test]
    fn empty() {
        let queue = Queue::<usize>::new_queue(2);
        assert!(queue.pop().is_err());
    }

    #[test]
    fn seq() {
        let queue = Queue::new_queue(2);

        queue.push(1).unwrap();
        queue.push(2).unwrap();

        assert!(queue.push(3).is_err());

        assert_eq!(queue.pop().unwrap(), 1);
        queue.push(4).unwrap();

        assert!(queue.push(5).is_err());
        assert!(queue.push(6).is_err());

        assert_eq!(queue.pop().unwrap(), 2);
        assert_eq!(queue.pop().unwrap(), 4);

        assert!(queue.pop().is_err());
        assert!(queue.pop().is_err());

        queue.push(7).unwrap();
        assert_eq!(queue.pop().unwrap(), 7);
        queue.push(8).unwrap();
        queue.push(9).unwrap();

        assert!(queue.push(10).is_err());
        assert!(queue.push(11).is_err());

        assert_eq!(queue.pop().unwrap(), 8);
        assert_eq!(queue.pop().unwrap(), 9);
        assert!(queue.pop().is_err());
        assert!(queue.pop().is_err());
        assert!(queue.pop().is_err());

        queue.push(12).unwrap();
        queue.push(13).unwrap();

        assert_eq!(queue.pop().unwrap(), 12);
        assert_eq!(queue.pop().unwrap(), 13);

        queue.push(14).unwrap();
        assert_eq!(queue.pop().unwrap(), 14);
        queue.push(15).unwrap();
        assert_eq!(queue.pop().unwrap(), 15);
        queue.push(16).unwrap();
        assert_eq!(queue.pop().unwrap(), 16);

        queue.push(17).unwrap();
        queue.push(18).unwrap();
        assert!(queue.push(19).is_err());

        assert_eq!(queue.pop().unwrap(), 17);
        assert_eq!(queue.pop().unwrap(), 18);
        assert!(queue.pop().is_err());
    }

    #[test]
    fn closed() {
        let (mut sender, mut recv) = Queue::new(10);

        sender.push(10).unwrap();

        drop(sender);

        assert_eq!(recv.pop().unwrap(), 10);
        assert_eq!(recv.pop(), Err(PopError::Closed));
    }

    #[test]
    fn closed_recv() {
        let (mut sender, recv) = Queue::new(10);

        sender.push(1).unwrap();

        drop(recv);

        match sender.push(2) {
            Err(PushError::Closed(_)) => {}
            _ => panic!(),
        }
    }

    #[test]
    fn threads() {
        for size in 1..=10 {
            let (mut sender, mut recv) = Queue::new(size);

            std::thread::spawn(move || {
                sender.push(1).unwrap();

                for n in 0..1_000_000 {
                    loop {
                        match sender.push(n) {
                            Ok(_) => break,
                            Err(PushError::Closed(_)) => panic!("closed"),
                            _ => {}
                        }
                    }
                }
            });

            while let Err(e) = recv.pop() {
                assert_eq!(e, PopError::Empty);
            }

            let mut last_value = 0;

            for n in 0..1_000_000 {
                loop {
                    match recv.pop() {
                        Ok(v) => {
                            assert_eq!(v, n, "value={} loop={} last_value={}", v, n, last_value);
                            last_value = v;
                            break;
                        }
                        Err(PopError::Closed) => panic!(),
                        _ => {}
                    }
                }
            }

            std::thread::sleep(Duration::from_millis(10));
            assert_eq!(recv.pop(), Err(PopError::Closed));
        }
    }
}