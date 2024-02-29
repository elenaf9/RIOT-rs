#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(used_with_arg)]

use riot_rs::rt::debug::{exit, println};
use riot_rs::embassy::{Builder, init_config};

struct AppConfig;
impl Builder for AppConfig {}

init_config!(AppConfig);

#[riot_rs::thread]
fn main() {
    println!(
        "Hello from main()! Running on a {} board.",
        riot_rs::buildinfo::BOARD,
    );

    exit(Ok(()));
}
