use core::fmt::{Result, Write};

/// A dummy screen writing structure we can implement `Write` on
pub struct ScreenWriter;

impl Write for ScreenWriter {
    fn write_str(&mut self, s: &str) -> Result {
        crate::efi::output_string(s);
        Ok(())
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
