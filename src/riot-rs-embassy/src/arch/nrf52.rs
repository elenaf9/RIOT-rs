pub(crate) use embassy_executor::{InterruptExecutor, SendSpawner, SpawnToken};
pub use embassy_nrf::interrupt::SWI0_EGU0;
pub use embassy_nrf::{init, OptionalPeripherals, Peripherals};
pub use embassy_nrf::{interrupt, peripherals};
#[cfg(feature = "usb")]
use embassy_nrf::{
    peripherals,
    usb::{vbus_detect::HardwareVbusDetect, Driver},
    bind_interrupts, rng, usb as nrf_usb
};
use super::{Execute, Spawn};

#[cfg(feature = "usb")]
bind_interrupts!(struct Irqs {
    USBD => nrf_usb::InterruptHandler<peripherals::USBD>;
    POWER_CLOCK => nrf_usb::vbus_detect::InterruptHandler;
    RNG => rng::InterruptHandler<peripherals::RNG>;
});

#[interrupt]
unsafe fn SWI0_EGU0() {
    crate::EXECUTOR.on_interrupt()
}

impl Execute for InterruptExecutor {
    type SWI = SWI0_EGU0;
    type OptionalPeripherals = OptionalPeripherals;
    type Spawner = SendSpawner;
    type Peripherals = Peripherals;
    #[cfg(feature = "usb")]
    type UsbDriver = Driver<'static, peripherals::USBD, HardwareVbusDetect>;

    
    fn new() -> Self {
        InterruptExecutor::new()
    }
    
    fn init(op: Self::OptionalPeripherals) -> Peripherals {
        InterruptExecutor::init(op)
    }

    fn start(&self, swi: Self::SWI) {
        InterruptExecutor::start(&self, swi)
    }

    fn swi() -> Self::SWI {
        SWI0_EGU0
    }

    fn spawner(&self) -> Self::Spawner {
        self.0.spawner
    } 

    #[cfg(feature = "usb")]
    fn driver(peripherals: &mut Self::OptionalPeripherals) -> Self::UsbDriver {
        let usbd = peripherals.USBD.take().unwrap();
        Driver::new(usbd, Irqs, HardwareVbusDetect::new(Irqs))
    }
}
