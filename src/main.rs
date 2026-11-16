// SPDX-License-Identifier: GPL-3.0-or-later
//! CoolerControl device service plugin for the ARGB (JRAINBOW) headers of MSI Mystic Light
//! 185-byte motherboards (tested target: X670E GAMING PLUS WIFI, USB 0db0:0076).
//! Based on the CoolerControl device-service-template (GPL-3.0-or-later).
//! Protocol adapted from OpenRGB's MSIMotherboard185Controller (GPL-2.0-or-later).

mod controller;
mod hid;
mod protocol;
mod service;

use crate::controller::{Controller, save_backup};
use crate::device_service::v1::device_service_server::DeviceServiceServer;
use crate::protocol::{Zone, ZoneSetting};
use anyhow::{Context, Result, bail};
use clap::Parser;
use log::{LevelFilter, error, info};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use systemd_journal_logger::{JournalLog, connected_to_journal};
use tokio::net::UnixListener;
use tokio::signal::unix::{SignalKind, signal};
use tokio_stream::wrappers::UnixListenerStream;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;

pub const SERVICE_ID: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const ENV_CC_LOG: &str = "CC_LOG";
const DEFAULT_SOCKET: &str = "/run/coolercontrol-plugin-msi_mysticlight.sock";

pub mod models {
    pub mod v1 {
        tonic::include_proto!("coolercontrol.models.v1");
    }
}
pub mod device_service {
    pub mod v1 {
        tonic::include_proto!("coolercontrol.device_service.v1");
    }
}

/// CoolerControl device service for the MSI Mystic Light ARGB headers
#[derive(Parser, Debug)]
#[clap(version, about, long_about = None)]
struct Args {
    /// Enable debug logging
    #[clap(short, long)]
    debug: bool,
    /// Unix socket to listen on
    #[clap(long, default_value = DEFAULT_SOCKET)]
    socket: String,
    /// Folder for state backups (default: <plugin folder>/backups for the service, ./backups for the tools)
    #[clap(long)]
    backup_dir: Option<PathBuf>,
    /// Tool: read the controller state (feature report 0x52, read only), print it and save a backup
    #[clap(long)]
    dump: bool,
    /// Tool: backup, then JRAINBOW1+2 solid red for N s, rainbow wave for N s, then restore the backup
    #[clap(long, value_name = "SECONDS")]
    test: Option<u64>,
    /// Tool: write a backup file (.bin from --dump/--test) back to the controller
    #[clap(long, value_name = "FILE")]
    restore: Option<PathBuf>,
    /// Tool: set a mode, e.g. --set fixed --color ff0000 (see --list-modes)
    #[clap(long, value_name = "MODE")]
    set: Option<String>,
    /// Color for --set, RRGGBB
    #[clap(long)]
    color: Option<String>,
    /// Speed for --set: slow, medium, fast
    #[clap(long)]
    speed: Option<String>,
    /// Header for --set: 1, 2 or all
    #[clap(long, default_value = "all")]
    header: String,
    /// List the lighting modes and exit
    #[clap(long)]
    list_modes: bool,
}

fn setup_logging(debug: bool) -> Result<()> {
    let level = if debug {
        LevelFilter::Debug
    } else if let Ok(lvl) = std::env::var(ENV_CC_LOG) {
        LevelFilter::from_str(&lvl).unwrap_or(LevelFilter::Info)
    } else {
        LevelFilter::Info
    };
    if connected_to_journal() {
        JournalLog::new()?.with_extra_fields(vec![("VERSION", VERSION)]).install()?;
        log::set_max_level(level);
    } else {
        env_logger::Builder::new().filter_level(level).init();
    }
    Ok(())
}

fn open_device() -> Result<hid::MysticLight> {
    let loc = hid::find_device().context("MSI Mystic Light 0db0:0076 not found")?;
    println!("Device: {} serial={:?} phys={:?}", loc.hidraw.display(), loc.uniq, loc.phys);
    hid::MysticLight::open(loc)
}

fn parse_color(s: &str) -> Result<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        bail!("color must be RRGGBB");
    }
    let v = u32::from_str_radix(s, 16).context("color must be RRGGBB")?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

fn zones(header: &str) -> Result<Vec<Zone>> {
    Ok(match header {
        "1" => vec![Zone::JRainbow1],
        "2" => vec![Zone::JRainbow2],
        "all" => vec![Zone::JRainbow1, Zone::JRainbow2],
        _ => bail!("--header must be 1, 2 or all"),
    })
}

fn set_all(dev: &hid::MysticLight, zs: &[Zone], s: &ZoneSetting) -> Result<()> {
    let new = dev.update(|p| {
        for z in zs {
            protocol::apply_zone(p, *z, s);
        }
    })?;
    let back = dev.read_state()?;
    if back == new {
        println!("  read back OK");
    } else {
        println!("  note: read back differs from what was written:\n{}", protocol::describe(&back));
    }
    Ok(())
}

fn tools(args: &Args) -> Result<bool> {
    let dir = args.backup_dir.clone().unwrap_or_else(|| PathBuf::from("backups"));
    if args.list_modes {
        for m in protocol::MODES {
            println!("{:<28} {:<28} colors={:?} speed={}", m.name, m.label, m.colors, m.speed);
        }
        return Ok(true);
    }
    if args.dump {
        let dev = open_device()?;
        let state = dev.read_state()?;
        println!("{}\n\n{}", protocol::describe(&state), protocol::hex(&state));
        println!("Backup saved to {}", save_backup(&dir, "dump", &state)?.display());
        return Ok(true);
    }
    if let Some(path) = &args.restore {
        let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        protocol::validate(&data).map_err(anyhow::Error::msg)?;
        let dev = open_device()?;
        println!("Restoring:\n{}", protocol::describe(&data));
        dev.write_state(&data)?;
        println!("Done. Now:\n{}", protocol::describe(&dev.read_state()?));
        return Ok(true);
    }
    if let Some(seconds) = args.test {
        let dev = open_device()?;
        let backup = dev.read_state()?;
        println!("Current state:\n{}", protocol::describe(&backup));
        println!("Backup saved to {}", save_backup(&dir, "test", &backup)?.display());
        let all = [Zone::JRainbow1, Zone::JRainbow2];
        let result = (|| -> Result<()> {
            println!("1/2 JRAINBOW1+2: solid red for {seconds} s");
            set_all(&dev, &all, &ZoneSetting::from_request("fixed", None, &[[255, 0, 0]]).unwrap())?;
            std::thread::sleep(Duration::from_secs(seconds));
            println!("2/2 JRAINBOW1+2: rainbow wave for {seconds} s");
            set_all(&dev, &all, &ZoneSetting::from_request("rainbow_wave", Some("medium"), &[]).unwrap())?;
            std::thread::sleep(Duration::from_secs(seconds));
            Ok(())
        })();
        println!("Restoring the original state");
        dev.write_state(&backup)?;
        println!("Now:\n{}", protocol::describe(&dev.read_state()?));
        result?;
        return Ok(true);
    }
    if let Some(mode) = &args.set {
        let colors = args.color.as_deref().map(parse_color).transpose()?.into_iter().collect::<Vec<_>>();
        let s = ZoneSetting::from_request(mode, args.speed.as_deref(), &colors).map_err(anyhow::Error::msg)?;
        let dev = open_device()?;
        let backup = dev.read_state()?;
        println!("Backup saved to {}", save_backup(&dir, "set", &backup)?.display());
        set_all(&dev, &zones(&args.header)?, &s)?;
        println!("Now:\n{}", protocol::describe(&dev.read_state()?));
        return Ok(true);
    }
    Ok(false)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    setup_logging(args.debug)?;
    if tools(&args)? {
        return Ok(());
    }

    info!("Starting {SERVICE_ID} v{VERSION}");
    let backup_dir = match &args.backup_dir {
        Some(d) => d.clone(),
        None => std::env::current_exe()?.parent().context("executable folder")?.join("backups"),
    };
    let ctrl = Arc::new(Controller::new(Some(backup_dir)));
    {
        // read (only) the state once at start so a backup exists before CoolerControl writes anything
        let ctrl = ctrl.clone();
        tokio::task::spawn_blocking(move || {
            if !ctrl.connected() {
                error!("MSI Mystic Light not available yet; will retry on the next request");
            }
        });
    }

    let token = CancellationToken::new();
    {
        let token = token.clone();
        tokio::spawn(async move {
            let mut term = signal(SignalKind::terminate()).expect("signal handler");
            let mut int = signal(SignalKind::interrupt()).expect("signal handler");
            let mut quit = signal(SignalKind::quit()).expect("signal handler");
            tokio::select! {
                _ = term.recv() => {},
                _ = int.recv() => {},
                _ = quit.recv() => {},
            }
            info!("Shutting down");
            token.cancel();
        });
    }

    let _ = tokio::fs::remove_file(&args.socket).await;
    let uds = UnixListener::bind(&args.socket).map_err(|e| {
        error!("Failed to bind to socket {}: {e}", args.socket);
        e
    })?;
    info!("Listening on {}", args.socket);
    Server::builder()
        .add_service(DeviceServiceServer::new(service::MysticLightService::new(ctrl)))
        .serve_with_incoming_shutdown(UnixListenerStream::new(uds), token.cancelled())
        .await?;
    let _ = tokio::fs::remove_file(&args.socket).await;
    Ok(())
}
