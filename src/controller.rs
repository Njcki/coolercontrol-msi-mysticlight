// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared device state: opens the controller lazily, serializes writes, keeps the startup state.

use crate::hid::{self, MysticLight};
use crate::protocol::{self, Zone, ZoneSetting};
use anyhow::{Context, Result};
use log::{info, warn};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct Controller {
    inner: Mutex<Inner>,
    backup_dir: Option<PathBuf>,
}

struct Inner {
    dev: Option<MysticLight>,
    /// state read when the plugin first reached the device; used by ResetChannel
    baseline: Option<Vec<u8>>,
}

impl Controller {
    pub fn new(backup_dir: Option<PathBuf>) -> Self {
        Self { inner: Mutex::new(Inner { dev: None, baseline: None }), backup_dir }
    }

    pub fn location(&self) -> Option<hid::DeviceLocation> {
        let g = self.inner.lock().unwrap();
        g.dev.as_ref().map(|d| d.location.clone()).or_else(hid::find_device)
    }

    pub fn connected(&self) -> bool {
        self.with_dev(|_| Ok(())).is_ok()
    }

    fn with_dev<T>(&self, f: impl FnOnce(&MysticLight) -> Result<T>) -> Result<T> {
        let mut g = self.inner.lock().unwrap();
        if g.dev.is_none() {
            let loc = hid::find_device().context("MSI Mystic Light 0db0:0076 not found")?;
            let dev = MysticLight::open(loc)?;
            let state = dev.read_state()?;
            info!("MSI Mystic Light at {} (serial {:?})", dev.location.hidraw.display(), dev.location.uniq);
            info!("Current state:\n{}", protocol::describe(&state));
            if g.baseline.is_none() {
                if let Some(dir) = &self.backup_dir {
                    match save_backup(dir, "plugin-start", &state) {
                        Ok(p) => info!("Saved controller state backup to {}", p.display()),
                        Err(e) => warn!("Could not save state backup: {e:#}"),
                    }
                }
                g.baseline = Some(state);
            }
            g.dev = Some(dev);
        }
        let r = f(g.dev.as_ref().unwrap());
        if r.is_err() {
            g.dev = None; // reopen on next call
        }
        r
    }

    pub fn set_zone(&self, zone: Zone, setting: &ZoneSetting) -> Result<()> {
        self.with_dev(|d| d.update(|p| protocol::apply_zone(p, zone, setting)).map(|_| ()))
    }

    pub fn reset_zone(&self, zone: Zone) -> Result<()> {
        let baseline = {
            let _ = self.with_dev(|_| Ok(()));
            self.inner.lock().unwrap().baseline.clone()
        };
        let Some(baseline) = baseline else { anyhow::bail!("no baseline state") };
        self.with_dev(|d| d.update(|p| protocol::restore_zone(p, &baseline, zone)).map(|_| ()))
    }
}

pub fn save_backup(dir: &Path, tag: &str, state: &[u8]) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let path = dir.join(format!("mysticlight-{tag}-{secs}.bin"));
    std::fs::write(&path, state).with_context(|| format!("writing {}", path.display()))?;
    let txt = path.with_extension("txt");
    let _ = std::fs::write(&txt, format!("{}\n\n{}\n", protocol::describe(state), protocol::hex(state)));
    // when run with sudo, hand the files back to the invoking user
    if let (Ok(uid), Ok(gid)) = (std::env::var("SUDO_UID"), std::env::var("SUDO_GID")) {
        if let (Ok(uid), Ok(gid)) = (uid.parse::<u32>(), gid.parse::<u32>()) {
            for p in [dir, path.as_path(), txt.as_path()] {
                if let Ok(c) = std::ffi::CString::new(p.as_os_str().as_encoded_bytes()) {
                    // SAFETY: valid NUL terminated path
                    unsafe { libc::chown(c.as_ptr(), uid, gid) };
                }
            }
        }
    }
    Ok(path)
}
