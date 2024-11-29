#![no_main]
#![no_std]
#![feature(impl_trait_in_assoc_type)]
#![feature(used_with_arg)]

use ariel_os::debug::{exit, log::*};

use ariel_os::{blocker, EXECUTOR};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

static SIGNAL: Signal<CriticalSectionRawMutex, u32> = Signal::new();

#[embassy_executor::task]
async fn async_task() {
    use embassy_time::{Duration, Timer, TICK_HZ};
    let mut counter = 0u32;
    loop {
        if counter % 2 == 0 {
            info!("async_task() signalling");
            SIGNAL.signal(counter);
        } else {
            info!("async_task()");
        }
        Timer::after(Duration::from_ticks(TICK_HZ / 10)).await;
        counter += 1;
    }
}

#[ariel_os::thread(autostart)]
fn main() {
    use embassy_time::Instant;

    info!(
        "Hello from main()! Running on a {} board.",
        ariel_os::buildinfo::BOARD,
    );

    let spawner = EXECUTOR.spawner();
    spawner.spawn(async_task()).unwrap();

    for _ in 0..10 {
        let val = blocker::block_on(SIGNAL.wait());
        info!(
            "now={}ms threadtest() val={}",
            Instant::now().as_millis(),
            val,
        );
    }

    exit(Ok(()));
}
