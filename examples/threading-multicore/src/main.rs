#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(used_with_arg)]

use ariel_os::debug::log::*;

#[ariel_os::thread(autostart)]
fn thread0() {
    let core = ariel_os::thread::core_id();
    let pid = ariel_os::thread::current_pid().unwrap();
    info!("Hello from {:?} on {:?}", pid, core);
    loop {}
}

#[ariel_os::thread(autostart)]
fn thread1() {
    let core = ariel_os::thread::core_id();
    let pid = ariel_os::thread::current_pid().unwrap();
    info!("Hello from {:?} on {:?}", pid, core);
    loop {}
}
