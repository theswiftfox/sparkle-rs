use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub fn mtx_lock<A>(mtx: &'_ Mutex<A>) -> MutexGuard<'_, A> {
    match mtx.lock() {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("Mutex was poisoned. Resetting!");
            e.into_inner()
        }
    }
}

pub fn lock_read<A>(lock: &'_ RwLock<A>) -> RwLockReadGuard<'_, A> {
    match lock.read() {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("Mutex was poisoned. Resetting!");
            e.into_inner()
        }
    }
}

pub fn lock_write<A>(lock: &'_ RwLock<A>) -> RwLockWriteGuard<'_, A> {
    match lock.write() {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("Mutex was poisoned. Resetting!");
            e.into_inner()
        }
    }
}
