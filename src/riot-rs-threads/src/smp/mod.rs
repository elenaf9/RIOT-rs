use super::CoreId;

pub trait Multicore {
    const CORES: u32;

    fn core_id() -> CoreId;

    fn startup_cores();

    fn schedule_on_core(id: CoreId);
}

cfg_if::cfg_if! {
    if #[cfg(context = "rp2040")] {
        mod rp2040;
        pub use rp2040::Chip;
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
