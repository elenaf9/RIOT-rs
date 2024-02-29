
pub trait Builder {
    fn build_config() -> Config {
        Config::default()
    }
}

#[cfg(feature = "net")]
pub enum NetDevice {
    #[cfg(feature = "usb_ethernet")]
    UsbEthernet,
    #[cfg(feature = "wifi_cyw43")]
    WifiCyw43
}

#[derive(Default)]
pub struct Config {
    #[cfg(feature = "usb")]
    pub(crate) with_usb: bool,

    #[cfg(feature = "net")]
    pub(crate) with_net: Option<(NetDevice, embassy_net::Config)>,

    // #[cfg(feature = "threading")]
    // pub(crate) with_threading: bool,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(feature = "usb")]
    pub fn with_usb(mut self) -> Self {
        self.with_usb = true;
        self
    }
    
    #[cfg(feature = "net")]
    pub fn with_net(mut self, device: NetDevice, config: Option<embassy_net::Config>) -> Self {
        #[cfg(feature = "usb_ethernet")]
        #[allow(irrefutable_let_patterns)]
        if let NetDevice::UsbEthernet = device {
            self = self.with_usb();
        }
        let net_config = config.unwrap_or_else(|| embassy_net::Config::dhcpv4(Default::default()));
        self.with_net = Some((device, net_config));
        self
    }

    // #[cfg(feature = "threading")]
    // pub fn with_threading(mut self) -> Self {
    //     self.with_threading =  true;
    //     self
    // }
}