use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::sync::MutexGuard;

pub trait ExternalGuard<I: Eq + Clone> {
    fn get_guard_id(&self) -> &I;
}

/// Provides mutual exclusion for a struct based on a external Mutex.
pub struct ExternallyGuarded<I: Eq + Clone, G: ExternalGuard<I>, T> {
    guard_id: I,
    payload: UnsafeCell<T>,
    _phantom: PhantomData<G>,
}

impl<I: Eq + Clone, G: ExternalGuard<I>, T> ExternallyGuarded<I, G, T> {
    /// Creates a new instance requiring a lock on the specified guard.
    ///
    /// # Safety
    /// Safety of this struct is derived from uniqueness of guard ids. Calling code must make
    /// sure that the guard id is unique amongst all instances of a specific guard type.
    pub unsafe fn new(guard: &G, value: T) -> Self {
        Self {
            guard_id: guard.get_guard_id().clone(),
            payload: UnsafeCell::new(value),
            _phantom: PhantomData,
        }
    }

    pub fn get<'a>(&'a self, _guard: &'a MutexGuard<G>) -> &'a T {
        if _guard.get_guard_id() != &self.guard_id {
            panic!("guard_id check failed");
        }
        unsafe { self.payload.get().as_ref().unwrap_unchecked() }
    }

    pub fn get_mut<'a>(&'a self, _guard: &'a mut MutexGuard<G>) -> &'a mut T {
        if _guard.get_guard_id() != &self.guard_id {
            panic!("guard_id check failed");
        }
        unsafe { self.payload.get().as_mut().unwrap_unchecked() }
    }

    pub fn borrow_mut(&mut self) -> &mut T {
        unsafe { self.payload.get().as_mut().unwrap_unchecked() }
    }
}

unsafe impl<I: Eq + Clone, G: ExternalGuard<I>, T> Send for ExternallyGuarded<I, G, T> where I: Send, T: Send {
}
unsafe impl<I: Eq + Clone, G: ExternalGuard<I>, T> Sync for ExternallyGuarded<I, G, T> where I: Send, T: Send {
}