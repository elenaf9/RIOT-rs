pub mod gpio;

use esp_hal::{clock::ClockControl, embassy, prelude::*};

pub use esp_hal::{
    embassy::executor::Executor,
    interrupt,
    peripherals::{Interrupt, OptionalPeripherals, Peripherals, SYSTIMER},
};

#[derive(Default)]
pub struct Config {}

pub fn init(_config: Config) -> OptionalPeripherals {
    let mut peripherals = OptionalPeripherals::from(Peripherals::take());
    let system = peripherals.SYSTEM.take().unwrap().split();
    let clocks = ClockControl::max(system.clock_control).freeze();

    #[cfg(feature = "wifi-esp")]
    {
        use esp_hal::rng::Rng;
        use esp_wifi::{initialize, EspWifiInitFor};

        riot_rs_debug::println!("riot-rs-embassy::arch::esp::init(): wifi");

        let timer = esp_hal::systimer::SystemTimer::new(peripherals.SYSTIMER.take().unwrap());

        #[cfg(target_arch = "riscv32")]
        let init = initialize(
            EspWifiInitFor::Wifi,
            timer.alarm0,
            Rng::new(peripherals.RNG.take().unwrap()),
            system.radio_clock_control,
            &clocks,
        )
        .unwrap();
        // Mark alarm0 as allocated so that it won't be reused by the embassy executor.
        unsafe { assert_eq!(embassy_time_driver::allocate_alarm().unwrap().id(), 0) };

        crate::wifi::esp_wifi::WIFI_INIT.set(init).unwrap();
    }

    let timer = unsafe { esp_hal::systimer::SystemTimer::new_async(SYSTIMER::steal()) };
    embassy::init(&clocks, timer);

    peripherals
}
