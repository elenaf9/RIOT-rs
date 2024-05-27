#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(used_with_arg)]

use embassy_net::tcp::TcpSocket;
use embassy_time::Duration;
use embedded_io_async::Write;
use riot_rs::{debug::println, embassy::network};

#[riot_rs::task(autostart)]
async fn tcp_echo() {
    let stack = network::network_stack().await.unwrap();

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let mut buf = [0; 4096];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(10)));

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
