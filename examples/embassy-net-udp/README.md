# embassy-net-udp

## About

This application is testing basic
[embassy](https://github.com/embassy-rs/embassy) _networking_ usage with Ariel OS.

## How to run

In this folder, run

    laze build -b nrf52840dk run

With the device USB cable connected, a USB ethernet device should pop up.
Ariel OS will reply to ping requests on 10.42.0.61.

Look [here](../README.md#networking) or more information about network configuration.
