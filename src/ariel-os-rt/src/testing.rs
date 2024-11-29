// #![feature(custom_test_frameworks)]
// #![test_runner(ariel_os_rt::testing::test_runner)]
// #![reexport_test_harness_main = "test_main"]

use ariel_os_debug::{exit, print, println, EXIT_SUCCESS};

pub trait Testable {
    fn run(&self);
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        print!("{}...\t", core::any::type_name::<T>());
        self();
        println!("[ok]");
    }
}

pub fn test_runner(tests: &[&dyn Testable]) {
    println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }
    println!("Done.");
    exit(EXIT_SUCCESS);
}
