use embassy_time::{Duration, Timer};
use esp_wifi::{
    wifi::{
        ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiStaDevice,
        WifiState,
    },
    EspWifiInitialization,
};
use once_cell::sync::OnceCell;
use riot_rs_debug::log::{debug, info};

use crate::{arch::OptionalPeripherals, Spawner};

pub type NetworkDevice = WifiDevice<'static, WifiStaDevice>;

// Ideally, all Wi-Fi initialization would happen here.
// Unfortunately that's complicated, so we're using WIFI_INIT to pass the
// `EspWifiInitialization` from `crate::arch::esp::init()`.
// Using a `once_cell::OnceCell` here for critical-section support, just to be
// sure.
pub static WIFI_INIT: OnceCell<EspWifiInitialization> = OnceCell::new();

#[cfg(feature = "threading")]
pub static WIFI_THREAD_ID: OnceCell<riot_rs_threads::ThreadId> = OnceCell::new();

pub fn init(peripherals: &mut OptionalPeripherals, spawner: Spawner) -> NetworkDevice {
    let wifi = peripherals.WIFI.take().unwrap();
    let init = WIFI_INIT.get().unwrap();
    let (device, controller) = esp_wifi::wifi::new_with_mode(init, wifi, WifiStaDevice).unwrap();

    spawner.spawn(connection(controller)).ok();

    device
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    #[cfg(feature = "threading")]
    {
        let thread_id = WIFI_THREAD_ID.get().unwrap();
        riot_rs_threads::thread_flags::set(*thread_id, 0b1);
    }

    debug!("start connection task");
    debug!("Device configuration: {:?}", controller.get_configuration());
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
mod wifi_thread {

    use super::*;
    #[cfg(context = "esp32c6")]
    use esp_hal::peripherals::INTPRI as SystemPeripheral;
    #[cfg(context = "esp32c3")]
    use esp_hal::peripherals::SYSTEM as SystemPeripheral;
    use esp_hal::{
        interrupt,
        peripherals::{Interrupt, SYSTIMER},
    };

    extern "Rust" {
        // `esp_wifi` handler that does the context switching.
        fn FROM_CPU_INTR3(trap_frame: &mut interrupt::TrapFrame);
    }


    /// Interceptor for yielding to the esp-wifi scheduler.
    ///
    /// It only hands over to the esp-wifi scheduler if we're in the context 
    /// of the `esp_wifi_thread`. 
    /// Otherwise it first initiates a context switch to `esp_wifi_thread`, which
    /// will then trigger again a task yield there.
    fn intercept_task_yield(trap_frame: &mut interrupt::TrapFrame) {
        clear_interrupts();

        let wifi_thread_pid = WIFI_THREAD_ID.get().unwrap();

        // Check if we are currently in the context of the `esp_wifi_thread`.
        // If that's the case, hand over to the esp-wifi scheduler so that it can run its task.
        // Because `esp_wifi_thread` runs at highest priority, it won't be preempted.
        // If not, wake up `esp_wifi_thread`.
        match riot_rs_threads::current_pid() {
            Some(pid) if pid == *wifi_thread_pid && riot_rs_threads::is_running(pid) => unsafe {
                FROM_CPU_INTR3(trap_frame)
            },
            _ => {
                riot_rs_threads::wakeup(*wifi_thread_pid);
            }
        }
    }

    /// Thread that runs the esp-wifi scheduler.
    /// 
    /// Because it runs at highest priority, it can't be preempted by any riot-rs threads and therefore
    /// the two schedulers won't interleave.
    #[riot_rs_macros::thread(autostart, stacksize = 4096, priority = riot_rs_threads::SCHED_PRIO_LEVELS as u8 - 1)]
    fn esp_wifi_thread() {
        WIFI_THREAD_ID
            .set(riot_rs_threads::current_pid().unwrap())
            .unwrap();

        // Wait until `embassy` was initialized.
        riot_rs_threads::thread_flags::wait_one(0b1);

        // Intercept the esp-wifi interrupts with our own handler.
        for intr in [Interrupt::SYSTIMER_TARGET0, Interrupt::FROM_CPU_INTR3] {
            unsafe {
                interrupt::bind_interrupt(
                    intr,
                    core::mem::transmute(intercept_task_yield as *const ()),
                );
            }
            interrupt::enable(intr, interrupt::Priority::Priority2).unwrap();
        }

        loop {
            // Thread will be woken up by `intercept_task_yield` when an esp-wifi 
            // task or ISR attempts to yield to the esp-wifi scheduler.
            riot_rs_threads::sleep();
            // Yield again now that we are in the correct context.
            yield_task();
        }
    }

    fn clear_interrupts() {
        unsafe {
            // Clear CPU Interrupt 3.
            (*SystemPeripheral::PTR)
                .cpu_intr_from_cpu_3()
                .modify(|_, w| w.cpu_intr_from_cpu_3().clear_bit());

            // Clear systimer target 0 interrupt.
            SYSTIMER::steal()
                .int_clr()
                .write(|w| w.target0().clear_bit_by_one())
        }
    }

    fn yield_task() {
        unsafe {
            let cpu_intr_3 = (&*SystemPeripheral::PTR).cpu_intr_from_cpu_3();

            cpu_intr_3.modify(|_, w| w.cpu_intr_from_cpu_3().set_bit());
            // Avoid that any subsequent code is executed before the interrupt
            // actually triggered.
            while cpu_intr_3.read().cpu_intr_from_cpu_3().bit() {}
        }
    }
}
