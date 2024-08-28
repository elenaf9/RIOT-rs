#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(used_with_arg)]

use riot_rs::{debug::log::*, thread::ThreadId};

#[riot_rs::thread(autostart, priority = 3)]
fn thread0() {
    let pid = riot_rs::thread::current_pid().unwrap();
    info!(
        "{} on {}: Running at prio {}.",
        pid,
        riot_rs::thread::core_id(),
        riot_rs::thread::get_priority(pid).unwrap()
    );
    let new_thread1_prio = 5;
    info!(
        "{} on {}: Changing Thread 1's prio to {}.",
        pid,
        riot_rs::thread::core_id(),
        new_thread1_prio
    );
    riot_rs::thread::set_priority(ThreadId::new(1), new_thread1_prio);
    info!(
        "{} on {}: Looping forever now at prio {}.",
        pid,
        riot_rs::thread::core_id(),
        riot_rs::thread::get_priority(pid).unwrap()
    );
    loop {}
}

#[riot_rs::thread(autostart, priority = 1)]
fn thread1() {
    let pid = riot_rs::thread::current_pid().unwrap();
    info!(
        "{} on {}: Running at prio {}.",
        pid,
        riot_rs::thread::core_id(),
        riot_rs::thread::get_priority(pid).unwrap()
    );
    let new_prio = 1;
    info!(
        "{} on {}: Changing own prio back to {}.",
        pid,
        riot_rs::thread::core_id(),
        new_prio
    );
    riot_rs::thread::set_priority(pid, new_prio);
    unreachable!("Core(s) should be blocked by other two high prio threads.")
}

#[riot_rs::thread(autostart, priority = 2)]
fn thread2() {
    let pid = riot_rs::thread::current_pid().unwrap();
    info!(
        "{} on {}: Looping forever at prio {}.",
        pid,
        riot_rs::thread::core_id(),
        riot_rs::thread::get_priority(pid).unwrap()
    );
    loop {}
}
