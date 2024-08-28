#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(used_with_arg)]

use embassy_time::{Duration, Timer};
use riot_rs::{debug::log::*, thread::sync::Mutex, thread::ThreadId};

static COUNTER: Mutex<usize> = Mutex::new(0);

#[riot_rs::task(autostart)]
async fn task_with_timer() {
    let pid = riot_rs::thread::current_pid().unwrap();

    let mut shared_counter = COUNTER.lock();
    *shared_counter += 1;

    let prio = riot_rs::thread::get_priority(pid).unwrap();
    info!(
        "Task with priority {} got mutex; current counter: {}",
        prio, *shared_counter
    );

    // Start other tasks.
    riot_rs::thread::thread_flags::set(ThreadId::new(0), 0b10);
    riot_rs::thread::thread_flags::set(ThreadId::new(1), 0b10);
    riot_rs::thread::thread_flags::set(ThreadId::new(2), 0b10);

    info!("Task waiting for timer now...");
    Timer::after(Duration::from_secs(3)).await;

    let prio = riot_rs::thread::get_priority(pid).unwrap();
    info!("Task priority now: {}", prio);

    drop(shared_counter);

    let prio = riot_rs::thread::get_priority(pid).unwrap();
    info!("Task priority after releasing mutex: {}", prio);
}

fn get_mutex() {
    let pid = riot_rs::thread::current_pid().unwrap();
    let prio = riot_rs::thread::get_priority(pid).unwrap();

    info!("{} with prio {} waits for mutex", pid, prio);

    let mut shared_counter = COUNTER.lock();
    *shared_counter += 1;

    info!("{} got mutex; current counter: {}", pid, *shared_counter);
}

#[riot_rs::thread(autostart)]
fn thread0() {
    riot_rs::thread::thread_flags::wait_one(0b10);
    get_mutex();
}

#[riot_rs::thread(autostart, priority = 10)]
fn thread1() {
    riot_rs::thread::thread_flags::wait_one(0b10);
    // Allow thread0 to be the first thread that waits for the mutex.
    riot_rs::thread::thread_flags::wait_one(0b1);
    get_mutex();
}

#[riot_rs::thread(autostart)]
fn thread2() {
    riot_rs::thread::thread_flags::wait_one(0b10);
    riot_rs::thread::thread_flags::set(ThreadId::new(1), 0b1);
    get_mutex();
}
