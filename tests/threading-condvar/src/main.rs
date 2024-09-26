#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(used_with_arg)]

use portable_atomic::{AtomicUsize, Ordering};
use riot_rs::thread::{
    sync::{Condvar, Mutex},
    thread_flags, ThreadId,
};

static CONDVAR: Condvar = Condvar::new();
static MUTEX: Mutex<u8> = Mutex::new(0);
static RUN_ORDER: AtomicUsize = AtomicUsize::new(0);

#[riot_rs::thread(autostart, priority = 1)]
fn thread0() {
    assert_eq!(RUN_ORDER.fetch_add(1, Ordering::AcqRel), 3);

    // Initial counter is 0.
    let mut counter = MUTEX.lock();
    assert_eq!(*counter, 0);

    // Increase counter and signal to one waiting thread.
    *counter += 1;
    drop(counter);
    // Wakeup one waiting thread, namely thread1 because it was
    // the first waiting thread.
    CONDVAR.notify_one();

    // Wait for other threads to complete.
    thread_flags::wait_all(0b111);
    riot_rs::debug::log::info!("Test passed!");
}

#[riot_rs::thread(autostart, priority = 2)]
fn thread1() {
    // First running thread because prio is is higher than thread0's prio.
    assert_eq!(RUN_ORDER.fetch_add(1, Ordering::AcqRel), 0);

    // Initial counter is 0.
    let counter = MUTEX.lock();
    assert_eq!(*counter, 0);

    // Wait for signal that counter was increased.
    let mut counter = CONDVAR.wait(counter);
    assert_eq!(*counter, 1);

    // Increase counter again and signal to remaining waiting threads.
    *counter += 1;
    drop(counter);
    CONDVAR.notify_all();

    thread_flags::set(ThreadId::new(0), 1);
}

#[riot_rs::thread(autostart, priority = 2)]
fn thread2() {
    assert_eq!(RUN_ORDER.fetch_add(1, Ordering::AcqRel), 1);

    // Initial counter is 0.
    let counter = MUTEX.lock();
    assert_eq!(*counter, 0);

    // Wait for signal that counter was increased.
    let counter = CONDVAR.wait(counter);
    assert_eq!(*counter, 2);

    thread_flags::set(ThreadId::new(0), 0b10);
}

#[riot_rs::thread(autostart, priority = 2)]
fn thread3() {
    assert_eq!(RUN_ORDER.fetch_add(1, Ordering::AcqRel), 2);

    // Initial counter is 0.
    let counter = MUTEX.lock();
    assert_eq!(*counter, 0);

    // Wait for signal that counter was increased.
    let counter = CONDVAR.wait(counter);
    assert_eq!(*counter, 2);

    thread_flags::set(ThreadId::new(0), 0b100);
}
