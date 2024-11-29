use ariel_os_utils::usize_from_env_or;

use super::CoreId;

pub trait Multicore {
    const CORES: u32;
    /// Stack size for the idle threads.
    const IDLE_THREAD_STACK_SIZE: usize = 256;

    fn core_id() -> CoreId;

    fn startup_cores();

    fn schedule_on_core(id: CoreId);
}

cfg_if::cfg_if! {
    if #[cfg(context = "rp2040")] {
        mod rp2040;
        pub use rp2040::Chip;
    } else if #[cfg(context = "esp32s3")] {
        mod esp32s3;
        pub use esp32s3::Chip;
    }
    else {

        pub struct Chip;
        impl Multicore for Chip {
            const CORES: u32 = 1;

            fn core_id() -> CoreId {
                CoreId::new(0)
            }

            fn startup_cores() {}

            fn schedule_on_core(_id: CoreId) { }
        }
    }
}

pub fn schedule_on_core(id: CoreId) {
    Chip::schedule_on_core(id);
}

/// Main stack size for the second core, that is also used by the ISR.
///
/// Uses default from `ariel-os-rt` if not specified.
/// The `CONFIG_ISR_STACKSIZE` env name and default is copied from
/// `ariel-os-rt`.
#[allow(dead_code, reason = "used in chip submodules")]
const ISR_STACKSIZE_CORE1: usize = usize_from_env_or!(
    "CONFIG_ISR_STACKSIZE_CORE1",
    usize_from_env_or!("CONFIG_ISR_STACKSIZE", 8192, "ISR stack size (in bytes)"),
    "Core 1 ISR stack size (in bytes)"
);
