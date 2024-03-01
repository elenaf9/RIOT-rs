//! Dummy module used to satisfy platform-independent tooling.

use core::future::{self, Future};

use embassy_executor::SpawnError;

use super::{Execute, Spawn};


pub struct DummyExecutor;

impl Execute for DummyExecutor {
    type Spawner = DummySpawner;
    type SWI = ();
    type Peripherals = ();
    type OptionalPeripherals = ();
    #[cfg(feature = "usb")]
    type UsbDriver = ();

    fn new() -> Self {
        DummyExecutor
    }

    fn init(_: Self::OptionalPeripherals) -> Self::Peripherals {
        ()
    }

    fn swi() -> Self::SWI {
        ()
    }

    fn spawner(&self) -> Self::Spawner {
        DummySpawner
    }

    fn start(&self, _: Self::SWI) {}

    #[cfg(feature = "usb")]
    fn driver(peripherals: &mut Self::OptionalPeripherals) -> Self::UsbDriver {
        ()
    }
}

pub struct DummySpawner;

impl Spawn for DummySpawner {

    fn spawn<S: Send>(&self, _: embassy_executor::SpawnToken<S>) -> Result<(), SpawnError> {
        Ok(())
    }

    fn for_current_executor() -> impl Future<Output = Self> {
        future::ready(DummySpawner)
    }
}
