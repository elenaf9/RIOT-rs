#![no_std]

use nrf52;

use riot_rs_rt::debug::println;

pub fn init() {
    println!("microbit_v2::init()");
    nrf52::init();
}