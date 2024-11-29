//! To provide a custom network configuration, use the `ariel_os::config` attribute macro.

use embassy_net::{Runner, Stack};
use embassy_sync::once_lock::OnceLock;

use crate::{sendcell::SendCell, NetworkDevice};

#[allow(dead_code)]
pub const ETHERNET_MTU: usize = 1514;

pub type NetworkStack = Stack<'static>;

pub(crate) static STACK: OnceLock<SendCell<NetworkStack>> = OnceLock::new();

pub async fn network_stack() -> Option<NetworkStack> {
    STACK.get().await.get_async().await.copied()
}

#[embassy_executor::task]
pub(crate) async fn net_task(mut runner: Runner<'static, NetworkDevice>) -> ! {
    runner.run().await
}

pub(crate) fn config() -> embassy_net::Config {
    #[cfg(not(feature = "override-network-config"))]
    {
        embassy_net::Config::dhcpv4(Default::default())
    }
    #[cfg(feature = "override-network-config")]
    {
        extern "Rust" {
            fn ariel_os_network_config() -> embassy_net::Config;
        }
        unsafe { ariel_os_network_config() }
    }
}
