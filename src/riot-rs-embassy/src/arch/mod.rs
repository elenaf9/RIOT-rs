use core::future::Future;

use embassy_executor::SpawnError;

#[cfg(context = "nrf52")]
mod nrf52;
#[cfg(context = "rp2040")]
mod rp2040;
#[cfg(not(any(context = "nrf52", context = "rp2040")))]
mod dummy;

#[cfg(context = "nrf52")]
pub type Executor =  GenericExecutor<nrf52::InterruptExecutor>;
#[cfg(context = "rp2040")]
pub type Executor =  GenericExecutor<rp2040::InterruptExecutor>;
#[cfg(not(any(context = "dummy", context = "rp2040")))]
pub type Executor = GenericExecutor<dummy::DummyExecutor>;

pub struct GenericExecutor<T>(T);

impl<T: Execute> Execute for GenericExecutor<T> {
    type Spawner = T::Spawner;
    type OptionalPeripherals = T::OptionalPeripherals;
    type Peripherals = T::Peripherals;
    type SWI = T::SWI;
    #[cfg(feature = "usb")]
    type UsbDriver = T::UsbDriver;
    
    fn new() -> Self {
        Self(T::new())
    }
    
    fn init(op: Self::OptionalPeripherals) -> Self::Peripherals {
        T::init(op)
    }

    fn start(&self, swi: Self::SWI) {
        self.0.start(swi)
    }

    fn swi() -> Self::SWI {
        T::swi()
    }

    fn spawner(&self) -> Self::Spawner {
        self.0.spawner()
    }
    
    #[cfg(feature = "usb")]
    fn driver(peripherals: &mut Self::OptionalPeripherals) -> Self::UsbDriver {
        T::driver(peripherals)
    }
}

pub trait Execute: Send + Sync {
    type SWI;
    type Peripherals;
    type OptionalPeripherals: From<Self::Peripherals> + Default;
    type Spawner: Spawn;
    #[cfg(feature = "usb")]
    type UsbDriver;

    fn new() -> Self;

    fn init(_: Self::OptionalPeripherals) -> Self::Peripherals;

    fn start(&self, _: Self::SWI);

    fn swi() -> Self::SWI;

    fn spawner(&self) -> Self::Spawner;

    #[cfg(feature = "usb")]
    fn driver(peripherals: &mut Self::OptionalPeripherals) -> Self::UsbDriver;

}

pub trait Spawn: Sized {

    fn spawn<S: Send>(&self, _token: embassy_executor::SpawnToken<S>) -> Result<(), SpawnError>;

    fn for_current_executor() -> impl Future<Output = Self>;
}

impl Spawn for embassy_executor::SendSpawner {
    fn for_current_executor() -> impl Future<Output = Self> {
        embassy_executor::SendSpawner::for_current_executor()
    }

    fn spawn<S: Send>(&self, token:  embassy_executor::SpawnToken<S>) -> Result<(), SpawnError> {
        embassy_executor::SendSpawner::spawn(&self, token)
    }
}
