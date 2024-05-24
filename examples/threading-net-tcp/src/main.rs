#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(used_with_arg)]

use embassy_net::tcp::TcpSocket;
use embedded_io_async::Write;
use esp_hal::{
    interrupt,
    peripherals::{Interrupt, SYSTIMER},
};
use riot_rs::{
    debug::println,
    embassy::network,
    thread::{thread_flags, ThreadId},
};

#[cfg(context = "esp32c6")]
use esp_hal::peripherals::INTPRI as SystemPeripheral;
#[cfg(context = "esp32c3")]
use esp_hal::peripherals::SYSTEM as SystemPeripheral;

// Handle the systimer alarm 0 interrupt, configured in esp-wifi.
extern "C" fn systimer_target0_() {
    unsafe {
        SYSTIMER::steal()
            .int_clr()
            .write(|w| w.target0().clear_bit_by_one())
    }
    // Wakeup `esp_wifi_thread`.
    riot_rs::thread::wakeup(ThreadId::new(0));
}

// CPU Interrupt 3 triggers the scheduler in `esp-wifi`.
fn yield_to_esp_wifi_scheduler() {
    unsafe {
        (&*SystemPeripheral::PTR)
            .cpu_intr_from_cpu_3()
            .modify(|_, w| w.cpu_intr_from_cpu_3().set_bit());
    }
}

/// High priority thread that frequently wakes up to run the esp-wifi
/// scheduler.
#[riot_rs::thread(autostart, priority = 10, stacksize = 4096)]
fn esp_wifi_thread() {
    // Wait until `embassy` was intialized.
    thread_flags::wait_all(1);

    // Bind the periodic systimer that is configured in esp-wifi to our own handler.
    unsafe {
        interrupt::bind_interrupt(
            Interrupt::SYSTIMER_TARGET0,
            core::mem::transmute(systimer_target0_ as *const ()),
        );
    }

    loop {
        // Yield to the esp-wifi scheduler tasks, so that they get a chance to run.
        yield_to_esp_wifi_scheduler();
        // Sleep until the systimer alarm 0 interrupts again.
        riot_rs::thread::sleep()
    }
}

/// Application task.
#[riot_rs::task(autostart)]
async fn tcp_echo() {
    // Start the esp-wifi thread.
    thread_flags::set(ThreadId::new(0), 1);

    let stack = network::network_stack().await.unwrap();

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let mut buf = [0; 4096];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

        println!("Listening on TCP:1234...");
        if let Err(e) = socket.accept(1234).await {
            println!("accept error: {:?}", e);
            continue;
        }

        println!("Received connection from {:?}", socket.remote_endpoint());

        loop {
            let n = match socket.read(&mut buf).await {
                Ok(0) => {
                    println!("read EOF");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    println!("read error: {:?}", e);
                    break;
                }
            };

            match socket.write_all(&buf[..n]).await {
                Ok(()) => {}
                Err(e) => {
                    println!("write error: {:?}", e);
                    break;
                }
            };
        }
    }
}

/// Low priority thread that runs the application logic.
#[riot_rs::thread(autostart, stacksize = 4096)]
fn main() {
    println!("main()");
    extern "Rust" {
        fn riot_rs_embassy_init() -> !;
    }
    // This autostarts the `tcp_echo` tasks above.
    unsafe { riot_rs_embassy_init() };
}
