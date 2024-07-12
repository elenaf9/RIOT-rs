use embassy_time::{Duration, Timer};
use esp_wifi::{
    wifi::{
        ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiStaDevice,
        WifiState,
    },
    EspWifiInitialization,
};
use once_cell::sync::OnceCell;
use riot_rs_debug::log::*;

use crate::{arch::OptionalPeripherals, Spawner};

#[cfg(feature = "threading")]
use core::cell::RefCell;
#[cfg(feature = "threading")]
use critical_section::Mutex;
#[cfg(feature = "threading")]
use esp_hal::{interrupt, peripherals, Cpu};

pub type NetworkDevice = WifiDevice<'static, WifiStaDevice>;

#[cfg(feature = "threading")]
static RIOT_RS_CTX: Mutex<RefCell<usize>> = Mutex::new(RefCell::new(0));

// Ideally, all Wi-Fi initialization would happen here.
// Unfortunately that's complicated, so we're using WIFI_INIT to pass the
// `EspWifiInitialization` from `crate::arch::esp::init()`.
// Using a `once_cell::OnceCell` here for critical-section support, just to be
// sure.
pub static WIFI_INIT: OnceCell<EspWifiInitialization> = OnceCell::new();

pub fn init(peripherals: &mut OptionalPeripherals, spawner: Spawner) -> NetworkDevice {
    #[cfg(feature = "threading")]
    rebind_interrupt();

    let wifi = peripherals.WIFI.take().unwrap();
    let init = WIFI_INIT.get().unwrap();
    let (device, controller) = esp_wifi::wifi::new_with_mode(init, wifi, WifiStaDevice).unwrap();

    spawner.spawn(connection(controller)).ok();

    device
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    debug!("start connection task");
    // debug!("Device capabilities: {:?}", controller.get_capabilities());
    loop {
        match esp_wifi::wifi::get_wifi_state() {
            WifiState::StaConnected => {
                // wait until we're no longer connected
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                Timer::after(Duration::from_secs(5)).await
            }
            _ => {}
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: crate::wifi::WIFI_NETWORK.try_into().unwrap(),
                password: crate::wifi::WIFI_PASSWORD.try_into().unwrap(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            debug!("Starting Wi-Fi");
            controller.start().await.unwrap();
            debug!("Wi-Fi started!");
        }
        debug!("About to connect...");

        match controller.connect().await {
            Ok(_) => info!("Wifi connected!"),
            Err(e) => {
                info!("Failed to connect to Wi-Fi: {:?}", e);
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[cfg(feature = "threading")]
extern "Rust" {
    /// esp-wifi handler for CPU Interrupt 3.
    ///
    /// CPU Interrupt 3 is used in the esp-wifi scheduler
    /// to initiate context switches between their tasks.
    fn FROM_CPU_INTR3(trap_frame: &mut interrupt::TrapFrame);
}

#[cfg(feature = "threading")]
fn handle_schedule(trap_frame: &mut interrupt::TrapFrame) {
    // Disable the RIOT-rs scheduler if we are in a RIOT-rs thread context and
    // now delegating to the esp-wifi scheduler.
    critical_section::with(|cs| {
        let mut pc = RIOT_RS_CTX.borrow(cs).borrow_mut();
        if *pc == 0 {
            *pc = trap_frame.pc;
            // CPU Interrupt 1 is used in `riot-rs-threads` to trigger the scheduler.
            interrupt::disable(Cpu::ProCpu, peripherals::Interrupt::FROM_CPU_INTR1);
        }
    });

    unsafe {
        // Call the esp-wifi scheduler.
        FROM_CPU_INTR3(trap_frame)
    }

    // Re-enable the RIOT-rs scheduler if all esp-wifi tasks have run and
    // we're back in a RIOT-rs thread context.
    critical_section::with(|cs| {
        let mut pc = RIOT_RS_CTX.borrow(cs).borrow_mut();
        if *pc == trap_frame.pc {
            *pc = 0;
            interrupt::enable(
                peripherals::Interrupt::FROM_CPU_INTR1,
                interrupt::Priority::min(),
            )
            .unwrap();
        }
    });
}

// Handle the systimer alarm 0 interrupt, configured in esp-wifi.
#[cfg(feature = "threading")]
extern "C" fn systimer_target0_(trap_frame: &mut interrupt::TrapFrame) {
    handle_schedule(trap_frame);
}

#[cfg(feature = "threading")]
extern "C" fn from_cpu_intr3_(trap_frame: &mut interrupt::TrapFrame) {
    handle_schedule(trap_frame);
}

#[cfg(feature = "threading")]
pub fn rebind_interrupt() {
    unsafe {
        // Bind the periodic systimer that is configured in esp-wifi to our own handler.
        interrupt::bind_interrupt(
            peripherals::Interrupt::SYSTIMER_TARGET0,
            core::mem::transmute(systimer_target0_ as *const ()),
        );

        // CPU Interrupt
        interrupt::bind_interrupt(
            peripherals::Interrupt::FROM_CPU_INTR3,
            core::mem::transmute(from_cpu_intr3_ as *const ()),
        );
    }

    interrupt::enable(
        peripherals::Interrupt::SYSTIMER_TARGET0,
        interrupt::Priority::Priority2,
    )
    .unwrap();

    interrupt::enable(
        peripherals::Interrupt::FROM_CPU_INTR3,
        interrupt::Priority::Priority2,
    )
    .unwrap();
}
