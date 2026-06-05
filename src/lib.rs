pub mod effects;

pub const VID: u16 = 0x1038;
pub const PID: u16 = 0x1500;

pub(crate) const REPORT_LEN: usize = 32;
pub(crate) const CMD_SET_RGB: u8 = 0x07;

#[cfg(target_os = "linux")]
#[path = "hid_linux.rs"]
mod hid;

#[cfg(target_os = "windows")]
#[path = "hid_windows.rs"]
mod hid;

pub use hid::Plumbob;
