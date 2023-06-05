use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, Thread};

pub struct Sender<'a, T> {
    channel: &'a Channel<T>,
    receiving_thread: Thread,
}

pub struct Receiver<'a, T> {
    channel: &'a Channel<T>,
    _no_send: PhantomData<*const ()>,
}

pub struct Channel<T> {
    message: UnsafeCell<MaybeUninit<T>>,
    ready: AtomicBool,
}

unsafe impl<T> Sync for Channel<T> where T: Send {}

impl<T> Sender<'_, T> {
    pub fn send(self, message: T) {
        unsafe { (*self.channel.message.get()).write(message) };
        self.channel.ready.store(true, Ordering::Release);
        self.receiving_thread.unpark();
    }
}

impl<T> Receiver<'_, T> {
    pub fn is_ready(&self) -> bool {
        self.channel.ready.load(Ordering::Relaxed)
    }

    pub fn receive(self) -> T {
        // Remember that `thread::park()` might return spuriously. (Or because something
        // other than our send method called `unpark()`.) This means that we cannot
        // assume that the ready flag has been set when `park()` returns. So, we
        // need to use a loop to check the flag again after getting unparked.
        while !self.channel.ready.swap(false, Ordering::Acquire) {
            thread::park();
        }
        // Safety: We've just checked (and reset) the ready flag.
        unsafe { (*self.channel.message.get()).assume_init_read() }
    }
}

impl<T> Channel<T> {
    pub const fn new() -> Self {
        Self {
            message: UnsafeCell::new(MaybeUninit::uninit()),
            ready: AtomicBool::new(false),
        }
    }

    pub fn split<'a>(&'a mut self) -> (Sender<'a, T>, Receiver<'a, T>) {
        *self = Self::new();
        (
            Sender {
                channel: self,
                receiving_thread: thread::current(),
            },
            Receiver {
                channel: self,
                _no_send: PhantomData,
            },
        )
    }
}

impl<T> Drop for Channel<T> {
    fn drop(&mut self) {
        // We don’t need to use an atomic operation to check the atomic ready flag,
        // because an object can only be dropped if it is fully owned by whichever
        // thread is dropping it
        if *self.ready.get_mut() {
            // The same holds for Unsafe Cell
            unsafe { self.message.get_mut().assume_init_drop() }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::one_shot::Channel;
    use std::thread;

    #[test]
    fn test() {
        let mut channel = Channel::new();
        thread::scope(|s| {
            let (sender, receiver) = channel.split();
            s.spawn(move || {
                sender.send("hello world!");
            });
            assert_eq!(receiver.receive(), "hello world!");
        });
    }
}
