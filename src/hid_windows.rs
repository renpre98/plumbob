use anyhow::{anyhow, Context, Result};
use hidapi::{HidApi, HidDevice};

use crate::{CMD_SET_RGB, PID, REPORT_LEN, VID};

pub struct Plumbob {
    dev: HidDevice,
}

impl Plumbob {
    pub fn open() -> Result<Self> {
        let api = HidApi::new().context("hidapi init failed")?;
        let dev = api
            .open(VID, PID)
            .map_err(|e| anyhow!("Plumbob {:04x}:{:04x} not found: {e}", VID, PID))?;
        Ok(Self { dev })
    }

    pub fn set_rgb(&mut self, r: u8, g: u8, b: u8) -> Result<()> {
        // hidapi expects byte 0 of the buffer to be the report ID. This device
        // declares no report IDs, so byte 0 must be 0x00 and the actual 32-byte
        // payload follows — its own byte 0 is the command code (0x07).
        let mut buf = [0u8; 1 + REPORT_LEN];
        buf[1] = CMD_SET_RGB;
        buf[3] = r;
        buf[4] = g;
        buf[5] = b;
        let n = self.dev.write(&buf).context("hidapi write failed")?;
        if n != buf.len() {
            return Err(anyhow!("short write: {n}/{}", buf.len()));
        }
        Ok(())
    }

    pub fn off(&mut self) -> Result<()> {
        self.set_rgb(0, 0, 0)
    }
}
