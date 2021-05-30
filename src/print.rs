use core::fmt::{Result, Error, Write};

/// A dummy screen writing structure we can implement `Write` on
pub struct ScreenWriter;

impl Write for ScreenWriter {
    fn write_str(&mut self, s: &str) -> Result {
        crate::efi::output_string(s).map_err(|_| Error)
    }
}

/// The standard Rust `print!()` macro!
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        let _ = <$crate::print::ScreenWriter as core::fmt::Write>::write_fmt(
            &mut $crate::print::ScreenWriter,
            format_args!($($arg)*));
    }
}
