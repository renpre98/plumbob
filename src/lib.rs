use anyhow::{anyhow, Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub mod effects;

pub const VID: u16 = 0x1038;
pub const PID: u16 = 0x1500;

const REPORT_LEN: usize = 32;
const CMD_SET_RGB: u8 = 0x07;

pub struct Plumbob {
    dev: File,
}

impl Plumbob {
    pub fn open() -> Result<Self> {
        let path = find_hidraw(VID, PID)?
            .ok_or_else(|| anyhow!("Plumbob {VID:04x}:{PID:04x} not found"))?;
        let dev = OpenOptions::new()
            .write(true)
            .open(&path)
            .with_context(|| format!("open {}", path.display()))?;
        Ok(Self { dev })
    }

    pub fn set_rgb(&mut self, r: u8, g: u8, b: u8) -> Result<()> {
        // Device has no declared report IDs and interprets byte 0 as the command code.
        // Writing the full 32-byte payload directly to /dev/hidraw* works because the
        // kernel forwards the buffer as a single output report.
        let mut buf = [0u8; REPORT_LEN];
        buf[0] = CMD_SET_RGB;
        buf[2] = r;
        buf[3] = g;
        buf[4] = b;
        let n = self.dev.write(&buf).context("hidraw write failed")?;
        if n != REPORT_LEN {
            return Err(anyhow!("short write: {n}/{REPORT_LEN}"));
        }
        Ok(())
    }

    pub fn off(&mut self) -> Result<()> {
        self.set_rgb(0, 0, 0)
    }
}

fn find_hidraw(vid: u16, pid: u16) -> Result<Option<PathBuf>> {
    // /sys/class/hidraw/hidrawN/device/uevent contains a line like:
    //   HID_ID=0003:00001038:00001500
    let needle = format!("HID_ID=0003:{:08X}:{:08X}", vid as u32, pid as u32);

    for entry in fs::read_dir("/sys/class/hidraw").context("read /sys/class/hidraw")? {
        let entry = entry?;
        let uevent = entry.path().join("device/uevent");
        let Ok(contents) = fs::read_to_string(&uevent) else {
            continue;
        };
        if contents.lines().any(|l| l.eq_ignore_ascii_case(&needle)) {
            let name = entry.file_name();
            return Ok(Some(PathBuf::from("/dev").join(name)));
        }
    }
    Ok(None)
}
