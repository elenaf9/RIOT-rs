#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![feature(used_with_arg)]

// FAIL: the `autostart` parameter must be present when requesting peripherals
#[ariel_os::task(peripherals)]
async fn main(_foo: Bar) {}

struct Bar;
