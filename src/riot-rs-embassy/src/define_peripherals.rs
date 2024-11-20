/// This macro allows to extract the specified peripherals from `OptionalPeripherals` for use in an
/// application.
///
/// The generated struct can be obtained by calling the `take_peripherals()` method on
/// `&mut OptionalPeripherals`.
///
/// The `define_peripherals!` macro expects a `peripherals` module to be in scope, where the
/// peripheral types should come from.
///
/// It makes sense to use this macro multiple times, coupled with conditional compilation (using
/// the [`cfg`
/// attribute](https://doc.rust-lang.org/reference/conditional-compilation.html#the-cfg-attribute)),
/// to define different setups for different boards.
///
// Inspired by https://github.com/adamgreig/assign-resources/tree/94ad10e2729afdf0fd5a77cd12e68409a982f58a
// under MIT license
#[macro_export]
macro_rules! define_peripherals {
    (
        $(#[$outer:meta])*
        $peripherals:ident {
            $(
                $(#[$inner:meta])*
                $peripheral_name:ident : $peripheral_field:ident $(=$peripheral_alias:ident)?
            ),*
            $(,)?
        }
    ) => {
        #[allow(dead_code,non_snake_case)]
        $(#[$outer])*
        pub struct $peripherals {
            $(
                $(#[$inner])*
                pub $peripheral_name: peripherals::$peripheral_field
            ),*
        }

        $($(
            #[allow(missing_docs, non_camel_case_types)]
            pub type $peripheral_alias = peripherals::$peripheral_field;
        )?)*

        impl $crate::define_peripherals::TakePeripherals<$peripherals> for &mut $crate::hal::OptionalPeripherals {
            fn take_peripherals(&mut self) -> $peripherals {
                $peripherals {
                    $(
                        $(#[$inner])*
                        $peripheral_name: self.$peripheral_field.take().unwrap()
                    ),*
                }
            }
        }
    }
}

/// This macros allows to group peripheral structs defined with `define_peripherals!` into a single
/// struct that also implements `take_peripherals()`.
#[macro_export]
macro_rules! group_peripherals {
    (
        $(#[$outer:meta])*
        $group:ident {
            $(
                $(#[$inner:meta])*
                $peripheral_name:ident : $peripherals:ident
            ),*
            $(,)?
        }
    ) => {
        #[allow(dead_code,non_snake_case)]
        $(#[$outer])*
        pub struct $group {
            $(
                $(#[$inner])*
                pub $peripheral_name: $peripherals
            ),*
        }

        impl $crate::define_peripherals::TakePeripherals<$group> for &mut $crate::hal::OptionalPeripherals {
            fn take_peripherals(&mut self) -> $group {
                $group {
                    $(
                        $(#[$inner])*
                        $peripheral_name: self.take_peripherals()
                    ),*
                }
            }
        }
    }
}

pub trait TakePeripherals<T> {
    fn take_peripherals(&mut self) -> T;
}
