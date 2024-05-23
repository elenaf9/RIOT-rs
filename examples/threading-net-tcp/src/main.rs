#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(used_with_arg)]

use embassy_net::{tcp::TcpSocket, Ipv4Address};
use embassy_time::Timer;
use embedded_io_async::Write;
use esp_hal::{
    interrupt,
    peripherals::{Interrupt, SYSTIMER},
};
use riot_rs::{
    debug::println,
    embassy::{arch::Executor, make_static, network},
    thread::ThreadId,
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
    // Wakeup `wifi_background_loop`.
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

/// Tasks that drives the `esp-wifi` scheduler.
///
/// Because the task is `autostart`ed, it will run within the thread
/// that initializes embassy, namely `esp_wifi_thread` below.
#[riot_rs::task(autostart)]
async fn wifi_background_loop() {
    println!("wifi_background_loop()");

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
        // Yield to other background tasks autostarted by `riot-rs-embassy`.
        Timer::after_nanos(1).await;
        // Sleep until the systimer alarm 0 interrupts again.
        riot_rs::thread::sleep()
    }
}

/// High priority thread that frequently wakes up to run the esp-wifi
/// scheduler.
/// Because embassy is initialized in this thread, the autostarted
/// `wifi_background_loop` above will run within this thread.
#[riot_rs::thread(autostart, priority = 10, stacksize = 4096)]
fn esp_wifi_thread() {
    println!("main()");
    // riscv::interrupt::disable()
    extern "Rust" {
        fn riot_rs_embassy_init() -> !;
    }
    // This autostarts the `wifi_background_loop` tasks above.
    unsafe { riot_rs_embassy_init() };
}

#[riot_rs::task(pool_size = 1)]
async fn tcp_echo() {
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

            //println!("rxd {:02x}", &buf[..n]);

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
fn application_thread() {
    println!("network_thread()");
    let executor = make_static!(Executor::new());
    executor.run(|spawner| spawner.must_spawn(tcp_echo()));
}

#[riot_rs::thread(autostart, priority = 2)]
fn third_thread() {
    // Creating a third embassy executor would cause a panic.
    // This is because each embassy executor requires a system timer and
    // there are only 3 in total. One is already allocated to esp-wifi,
    // thus only 2 remain for the executors.

    // let executor = make_static!(Executor::new());
    // core::hint::black_box(executor);
}
