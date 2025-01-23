# Ariel OS
[![Book][book-badge]][documentation-mdbook]
[![Documentation][rustdoc-badge]][documentation-dev-rustdoc]

Ariel OS is an operating system for secure, memory-safe, low-power Internet of Things (IoT).
Ariel OS is based on Rust from the ground up.
Hardware targets include varieties of hardware based on 32-bit microcontroller architectures (such as Cortex-M, RISC-V, Xtensa).
Check out our [table of supported hardware](https://ariel-os.github.io/ariel-os/dev/docs/book/hardware_functionality_support.html).

## Design

Ariel OS builds on top of existing projects in the Embedded Rust ecosystem, including [Embassy](https://github.com/embassy-rs/embassy), [esp-hal](https://github.com/esp-rs/esp-hal), [defmt](https://github.com/knurling-rs/defmt), [probe-rs](https://github.com/probe-rs/probe-rs), [sequential-storage](https://github.com/tweedegolf/sequential-storage), and [embedded-test](https://github.com/probe-rs/embedded-test) among others.
While those provide high-quality building blocks for a wide range of embedded applications, such projects do not provide the high level of integration that developers know from contemporary C-based operating systems for microcontrollers, such as [RIOT](https://github.com/RIOT-OS/RIOT/) or [Zephyr](https://github.com/zephyrproject-rtos/zephyr) for instance.

Ariel OS thus follows an approach whereby it simultaneously integrates many heterogeneous pre-existing crates, and adds missing operating system functionalities such as a preemptive multicore scheduler, portable peripheral APIs, additional network security facilities, as well as [laze](https://github.com/kaspar030/laze), a meta-build system to bind it all together.
The result?
A powerful framework that allows to write portable embedded Rust applications with minimal boilerplate, providing a batteries-included experience.


## Quickstart

The following assumes you have a Nordic nrf52840dk connected to your PC.
(For other supported boards, you can find your board's name in
[./src/ariel-os-boards/Cargo.toml](https://anonymous.4open.science/r/ariel-os-8DBF/src/ariel-os-boards/Cargo.toml)
and use it instead of 'nrf52840dk' in the below guidelines.)

The following instructions will enable you to flash and run the [`hello-world`
example](https://anonymous.4open.science/r/ariel-os-8DBF/examples/hello-world):

### Prerequisites

1. install needed system dependencies. On Ubuntu, the following is sufficient:

        apt install build-essential curl git python3 pkg-config \
                   libssl-dev llvm-dev cmake libclang-dev gcc-arm-none-eabi \
                   clang libnewlib-nano-arm-none-eabi unzip lld ninja-build

1. install [rustup](https://rustup.rs/)

1. install [laze](https://github.com/kaspar030/laze): `cargo install laze`

1. install [probe-rs](https://github.com/probe-rs/probe-rs): `cargo install probe-rs-tools --locked`

1. clone this repository and cd into it

1. install rust targets: `laze build install-toolchain`

### Run the example

1. Compile, flash and the hello-world example using `probe-rs run`

        laze -C examples/hello-world build -b nrf52840dk run

![Example](./doc/hello-world_render.svg)

<details>
<summary> (might fail if the flash is locked, click here for unlocking instructions) </summary>
This might fail due to a locked chip, e.g., on most nrf52840dk boards that are fresh from the factory.
In that case, the above command throws an error that ends with something like this:

```
An operation could not be performed because it lacked the permission to do so: erase_all
```

The chip can be unlocked using this command:

    laze -C examples/hello-world build -b nrf52840dk flash-erase-all
</details>

## More information

Please look [at the build system documentation](https://ariel-os.github.io/ariel-os/dev/docs/book/build_system.html) for more usage
information.

## Minimum Supported Rust Version (MSRV)

Ariel OS makes use of selected Rust unstable features. For the time being, it is
recommended to use the nightly version pinned by [`rust-toolchain.toml`](rust-toolchain.toml).

## Coding Conventions

Please see the chapter on
[coding conventions](https://ariel-os.github.io/ariel-os/dev/docs/book/coding-conventions.html)
in the documentation.

## Copyright & License

Ariel OS is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](./LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](./LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

Thanks to all the people who already contributed!

[book-badge]: https://img.shields.io/badge/Book-%F0%9F%93%94-blue
[rustdoc-badge]: https://img.shields.io/badge/Documentation-%F0%9F%93%94-blue
[documentation-mdbook]: https://ariel-os.github.io/ariel-os/dev/docs/book/
[documentation-dev-rustdoc]: https://ariel-os.github.io/ariel-os/dev/docs/api/
