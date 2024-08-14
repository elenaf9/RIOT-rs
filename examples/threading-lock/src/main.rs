#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(used_with_arg)]

use embassy_time::{Duration, Timer};
use riot_rs::{debug::log::*, thread::lock::Lock, thread::ThreadId};

static LOCK: Lock = Lock::new();

#[riot_rs::task(autostart)]
async fn task_with_timer() {
    let pid = riot_rs::thread::current_pid().unwrap();
    LOCK.acquire();
    let prio = riot_rs::thread::get_priority(pid).unwrap();
    info!("Task with priority {} got lock", prio);
    info!("Task is waiting for timer now...");
    Timer::after(Duration::from_secs(3)).await;
    let prio = riot_rs::thread::get_priority(pid).unwrap();
    info!("Task priority now: {}", prio);
    info!("Task is releasing the lock.");
    LOCK.release();
    let prio = riot_rs::thread::get_priority(pid).unwrap();
    info!("Task priority after releasing lock: {} ", prio);
}

fn get_lock() {
    let pid = riot_rs::thread::current_pid().unwrap();
    let prio = riot_rs::thread::get_priority(pid).unwrap();
    info!("{} with prio {} waits for lock", pid, prio);
    LOCK.acquire();
    info!("{} got lock", pid);
    LOCK.release();
    info!("{} released lock", pid);
}

#[riot_rs::thread(autostart)]
fn thread0() {
    get_lock();
}

#[riot_rs::thread(autostart, priority = 10)]
fn thread1() {
    // Wait for flag so that this thread is trying
    riot_rs::thread::thread_flags::wait_one(0b1);
    get_lock();
}

#[riot_rs::thread(autostart)]
fn thread2() {
    riot_rs::thread::thread_flags::set(ThreadId::new(1), 0b1);
    get_lock();
}
