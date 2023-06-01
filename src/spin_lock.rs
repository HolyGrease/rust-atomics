use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

/// Promise to the compiler that it is actually safe for our type to be shared
/// between threads. However, since the lock can be used to send values of type
/// T from one thread to another, we must limit this promise to types that are
/// safe to send between threads.
unsafe impl<T> Sync for SpinLock<T> where T: Send {}

impl<T> SpinLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> Guard<T> {
        while self.locked.swap(true, Ordering::Acquire) {
            // Tells the processor that we’re spinning while waiting for `locked` to change.
            // On most major platforms, this hint results in a special instruction that
            // causes the processor core to optimize its behavior for such a situation
            std::hint::spin_loop();
        }
        Guard { lock: self }
    }
}

pub struct Guard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // Safety: The very existence of this Guard
        // guarantees we've exclusively locked the lock.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // Safety: The very existence of this Guard
        // guarantees we've exclusively locked the lock.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release)
    }
}

#[cfg(test)]
mod tests {
    use crate::spin_lock::SpinLock;
    use std::thread;

    #[test]
    fn test() {
        let x = SpinLock::new(Vec::new());
        thread::scope(|s| {
            s.spawn(|| x.lock().push(1));
            s.spawn(|| {
                let mut g = x.lock();
                g.push(2);
                g.push(2);
            });
        });
        let g = x.lock();
        assert!(g.as_slice() == [1, 2, 2] || g.as_slice() == [2, 2, 1]);
    }
}
