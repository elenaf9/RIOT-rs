#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(used_with_arg)]

use riot_rs::debug::{exit, log::*, EXIT_SUCCESS};

#[riot_rs::thread(autostart)]
fn main() {
    info!(
        "riot_rs::thread::semaphore::Semaphore: {}",
        core::mem::size_of::<riot_rs::thread::semaphore::Semaphore>(),
    );
    info!(
        "riot_rs::thread::Thread: {}",
        riot_rs::thread::thread_struct_size()
    );

    exit(EXIT_SUCCESS);
}
