use core::fmt::{self, Write};

use crate::{fast_terminal::TERMINAL, graphics::GLOBAL_DISPLAY};

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::logging::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => ($crate::logging::_log(log::Level::Trace, format_args!($($arg)*)));
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => ($crate::logging::_log(log::Level::Debug, format_args!($($arg)*)));
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => ($crate::logging::_log(log::Level::Info, format_args!($($arg)*)));
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => ($crate::logging::_log(log::Level::Warn, format_args!($($arg)*)));
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => ($crate::logging::_log(log::Level::Error, format_args!($($arg)*)));
}

pub fn _print(args: fmt::Arguments) {
    if let Some(disp) = GLOBAL_DISPLAY.lock().as_mut() {
        if let Some(term) = TERMINAL.lock().as_mut() {
            // Use the Writer struct to bridge Terminal and Display with core::fmt::Write
            struct Writer<'a, 'b> {
                t: &'a mut crate::fast_terminal::Terminal,
                d: &'b mut crate::graphics::Display,
            }
            impl Write for Writer<'_, '_> {
                fn write_str(&mut self, s: &str) -> fmt::Result {
                    self.t.print_str(self.d, s);
                    Ok(())
                }
            }
            let mut writer = Writer {
                t: term,
                d: disp,
            };
            let _ = writer.write_fmt(args);
        }
    }
}

pub fn _log(level: log::Level, args: fmt::Arguments) {
    if let Some(disp) = GLOBAL_DISPLAY.lock().as_mut() {
        if let Some(term) = TERMINAL.lock().as_mut() {
            // Use the Writer struct to bridge Terminal and Display with core::fmt::Write
            struct Writer<'a, 'b> {
                t: &'a mut crate::fast_terminal::Terminal,
                d: &'b mut crate::graphics::Display,
            }
            impl Write for Writer<'_, '_> {
                fn write_str(&mut self, s: &str) -> fmt::Result {
                    self.t.print_str(self.d, s);
                    Ok(())
                }
            }
            let mut writer = Writer {
                t: term,
                d: disp,
            };
            let _ = write!(writer, "[{}] {}", level, args);
        }
    }
}

pub struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            _log(record.level(), *record.args());
        }
    }

    fn flush(&self) {}
}

pub static LOGGER: Logger = Logger;

pub fn init() {
    if let Err(_e) = log::set_logger(&LOGGER) {
        // Logging initialization failed, but we can continue without it
        // In a kernel, we might want to handle this more carefully
        // For now, we'll just ignore it since logging is not critical for boot
    }
    log::set_max_level(log::LevelFilter::Trace);
}
