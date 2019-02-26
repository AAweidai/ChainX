#[cfg(feature = "std")]
pub use log::{debug as debug_m, error as error_m, info as info_m, warn as warn_m};

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => (
        #[cfg(feature = "std")]
        $crate::logger::error_m!(target: "runtime", "[runtime|{}|{}L] {}", module_path!(), line!(), format!($($arg)*));
    )
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => (
        #[cfg(feature = "std")]
        $crate::logger::warn_m!(target: "runtime", "[runtime|{}] {}", module_path!(), format!($($arg)*));
    )
}
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => (
        #[cfg(feature = "std")]
        $crate::logger::info_m!(target: "runtime", "[runtime|{}] {}", module_path!(), format!($($arg)*));
    )
}
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => (
        #[cfg(feature = "std")]
        $crate::logger::debug_m!(target: "runtime", "[runtime|{}] {}", module_path!(), format!($($arg)*));
    )
}
