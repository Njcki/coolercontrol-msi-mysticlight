// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux hidraw access to the MSI Mystic Light controller (USB 0db0:0076) with feature reports.

use crate::protocol::{self, PACKET_LEN, REPORT_ID};
use anyhow::{Context, Result, bail};
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

pub const VID: u16 = 0x0DB0;
pub const PID: u16 = 0x0076;
/// Virtual PID (first 4 chars of the serial on common-PID boards). OpenRGB board config for it:
/// "0 onboard, 2 JRGB, zones_set13 (JRGB1, JRGB2, JRAINBOW1, JRAINBOW2), per-LED direct".
pub const SUPPORTED_VIRTUAL_PIDS: &[&str] = &["7E16"]; // X670E GAMING PLUS WIFI (MS-7E16), the only board this plugin targets

#[derive(Debug, Clone)]
pub struct DeviceLocation {
    pub hidraw: PathBuf,
    pub uniq: Option<String>,
    pub phys: Option<String>,
}

impl DeviceLocation {
    pub fn virtual_pid(&self) -> Option<String> {
        self.uniq.as_ref().filter(|u| u.len() >= 4).map(|u| u[..4].to_ascii_uppercase())
    }
}

pub fn find_device() -> Option<DeviceLocation> {
    let wanted = format!("HID_ID=0003:{:08X}:{:08X}", VID, PID);
    let mut entries: Vec<_> = std::fs::read_dir("/sys/class/hidraw").ok()?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let Ok(content) = std::fs::read_to_string(entry.path().join("device/uevent")) else { continue };
        if !content.lines().any(|l| l.eq_ignore_ascii_case(&wanted)) {
            continue;
        }
        let field = |key: &str| {
            content.lines().find_map(|l| l.strip_prefix(key)).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
        };
        return Some(DeviceLocation {
            hidraw: Path::new("/dev").join(entry.file_name()),
            uniq: field("HID_UNIQ="),
            phys: field("HID_PHYS="),
        });
    }
    None
}

// _IOC(_IOC_READ|_IOC_WRITE, 'H', nr, len)
const fn hid_ioc(nr: u64, len: usize) -> u64 {
    (3u64 << 30) | ((len as u64) << 16) | ((b'H' as u64) << 8) | nr
}
const HIDIOCSFEATURE: u64 = hid_ioc(0x06, PACKET_LEN);
const HIDIOCGFEATURE: u64 = hid_ioc(0x07, PACKET_LEN);

pub struct MysticLight {
    file: File,
    pub location: DeviceLocation,
}

impl MysticLight {
    /// Opens the controller and checks it is the supported variant (read only checks).
    pub fn open(location: DeviceLocation) -> Result<Self> {
        match location.virtual_pid() {
            Some(v) if SUPPORTED_VIRTUAL_PIDS.contains(&v.as_str()) => {}
            other => bail!(
                "board virtual PID {:?} (serial {:?}) is not a known 185-byte JRAINBOW1/2 board, refusing to drive it",
                other,
                location.uniq
            ),
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&location.hidraw)
            .with_context(|| format!("opening {}", location.hidraw.display()))?;
        let dev = Self { file, location };
        let state = dev.read_state().context("reading feature report 0x52")?;
        protocol::validate(&state).map_err(anyhow::Error::msg)?;
        Ok(dev)
    }

    pub fn read_state(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; PACKET_LEN];
        buf[0] = REPORT_ID;
        // SAFETY: buffer is PACKET_LEN bytes as encoded in the ioctl number
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), HIDIOCGFEATURE as _, buf.as_mut_ptr()) };
        if r < 0 {
            return Err(std::io::Error::last_os_error()).context("HIDIOCGFEATURE");
        }
        buf.truncate(r as usize);
        protocol::validate(&buf).map_err(anyhow::Error::msg)?;
        Ok(buf)
    }

    pub fn write_state(&self, packet: &[u8]) -> Result<()> {
        protocol::validate(packet).map_err(anyhow::Error::msg)?;
        let mut buf = packet.to_vec();
        buf[protocol::OFF_SAVE] = 0; // never save to the controller flash
        // SAFETY: buffer is PACKET_LEN bytes as encoded in the ioctl number
        let r = unsafe { libc::ioctl(self.file.as_raw_fd(), HIDIOCSFEATURE as _, buf.as_ptr()) };
        if r < 0 {
            return Err(std::io::Error::last_os_error()).context("HIDIOCSFEATURE");
        }
        if r as usize != PACKET_LEN {
            bail!("short feature write: {r} of {PACKET_LEN} bytes");
        }
        Ok(())
    }

    /// Same sequence as OpenRGB's MSIMotherboard185Controller::Update(save=false) in zone mode:
    /// read the active settings, send them back, then send the new settings.
    pub fn update<F: FnOnce(&mut Vec<u8>)>(&self, change: F) -> Result<Vec<u8>> {
        let current = self.read_state()?;
        let mut new = current.clone();
        change(&mut new);
        self.write_state(&current)?;
        self.write_state(&new)?;
        Ok(new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ioctl_numbers() {
        // HIDIOCGFEATURE(185) = 0xC0B94807, HIDIOCSFEATURE(185) = 0xC0B94806
        assert_eq!(HIDIOCGFEATURE, 0xC0B9_4807);
        assert_eq!(HIDIOCSFEATURE, 0xC0B9_4806);
    }
}
