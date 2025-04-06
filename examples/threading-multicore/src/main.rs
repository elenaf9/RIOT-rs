#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]

use ariel_os::debug::log::*;

#[ariel_os::thread(autostart, priority = 2)]
fn thread0() {
    let core = ariel_os::thread::core_id();
    let tid = ariel_os::thread::current_tid().unwrap();
    info!("Hello from {:?} on {:?}", tid, core);
    loop {}
}

#[ariel_os::thread(autostart)]
fn thread1() {
    let core = ariel_os::thread::core_id();
    let tid = ariel_os::thread::current_tid().unwrap();
    info!("Hello from {:?} on {:?}", tid, core);
    loop {}
}
