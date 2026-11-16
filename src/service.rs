// SPDX-License-Identifier: GPL-3.0-or-later
//! gRPC Device Service implementation (CoolerControl plugin API).

use crate::controller::Controller;
use crate::device_service::v1::device_service_server::DeviceService;
use crate::device_service::v1::{
    CustomFunctionOneRequest, CustomFunctionOneResponse, EnableManualFanControlRequest,
    EnableManualFanControlResponse, FixedDutyRequest, FixedDutyResponse, HealthRequest,
    HealthResponse, InitializeDeviceRequest, InitializeDeviceResponse, LcdRequest, LcdResponse,
    LightingRequest, LightingResponse, ListDevicesRequest, ListDevicesResponse,
    ResetChannelRequest, ResetChannelResponse, ShutdownRequest, ShutdownResponse,
    SpeedProfileRequest, SpeedProfileResponse, StatusRequest, StatusResponse, health_response,
};
use crate::models::v1::{ChannelInfo, Device, DeviceInfo, DriverInfo, LightingModes, channel_info, lighting_modes};
use crate::protocol::{self, Colors, Zone, ZoneSetting};
use crate::{SERVICE_ID, VERSION, hid};
use log::{error, info};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tonic::{Request, Response, Status};

pub const DEVICE_ID: &str = "msi_mysticlight";
pub const CHANNELS: &[(&str, &str, Zone)] = &[("jrainbow1", "JRAINBOW1 (ARGB 5V)", Zone::JRainbow1), ("jrainbow2", "JRAINBOW2 (ARGB 5V)", Zone::JRainbow2)];

pub struct MysticLightService {
    ctrl: Arc<Controller>,
    started: Instant,
}

impl MysticLightService {
    pub fn new(ctrl: Arc<Controller>) -> Self {
        Self { ctrl, started: Instant::now() }
    }

    fn device(&self) -> Device {
        let location = self.ctrl.location();
        let modes = LightingModes {
            lighting_mode: protocol::MODES
                .iter()
                .map(|m| {
                    let (min, max) = match m.colors {
                        Colors::One => (1, 1),
                        Colors::OneOrRandom => (0, 1),
                        Colors::Random | Colors::Off => (0, 0),
                    };
                    lighting_modes::LightingMode {
                        name: m.name.into(),
                        frontend_name: Some(m.label.into()),
                        min_colors: min,
                        max_colors: max,
                        speed_enabled: m.speed,
                        backward_enabled: false,
                    }
                })
                .collect(),
        };
        let channels = CHANNELS
            .iter()
            .map(|(id, label, _)| {
                (
                    id.to_string(),
                    ChannelInfo {
                        label: Some(label.to_string()),
                        options: Some(channel_info::Options::LightingModes(modes.clone())),
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        Device {
            id: DEVICE_ID.into(),
            name: "MSI Mystic Light".into(),
            uid_info: location.as_ref().and_then(|l| l.uniq.clone().or_else(|| l.phys.clone())),
            info: Some(DeviceInfo {
                channels,
                lighting_speeds: protocol::SPEEDS.iter().map(|s| s.to_string()).collect(),
                model: Some(format!("MSI Mystic Light 185-byte ({:04x}:{:04x})", hid::VID, hid::PID)),
                driver_info: Some(DriverInfo {
                    name: Some(SERVICE_ID.into()),
                    version: Some(VERSION.into()),
                    locations: location.iter().map(|l| l.hidraw.display().to_string()).collect(),
                }),
                ..Default::default()
            }),
        }
    }
}

fn zone_for(device_id: &str, channel_id: &str) -> Result<Zone, Status> {
    if device_id == DEVICE_ID {
        if let Some((_, _, z)) = CHANNELS.iter().find(|(id, _, _)| *id == channel_id) {
            return Ok(*z);
        }
    }
    Err(Status::not_found(format!("unknown device/channel {device_id}/{channel_id}")))
}

#[tonic::async_trait]
impl DeviceService for MysticLightService {
    async fn health(&self, _r: Request<HealthRequest>) -> Result<Response<HealthResponse>, Status> {
        let ctrl = self.ctrl.clone();
        let ok = tokio::task::spawn_blocking(move || ctrl.connected()).await.unwrap_or(false);
        let status = if ok { health_response::Status::Ok } else { health_response::Status::Warning };
        Ok(Response::new(HealthResponse {
            name: SERVICE_ID.to_string(),
            version: VERSION.to_string(),
            status: status.into(),
            uptime_seconds: self.started.elapsed().as_secs(),
        }))
    }

    async fn list_devices(&self, _r: Request<ListDevicesRequest>) -> Result<Response<ListDevicesResponse>, Status> {
        Ok(Response::new(ListDevicesResponse { devices: vec![self.device()] }))
    }

    async fn initialize_device(&self, _r: Request<InitializeDeviceRequest>) -> Result<Response<InitializeDeviceResponse>, Status> {
        Ok(Response::new(InitializeDeviceResponse {}))
    }

    async fn shutdown(&self, _r: Request<ShutdownRequest>) -> Result<Response<ShutdownResponse>, Status> {
        info!("Shutdown requested by CoolerControl");
        Ok(Response::new(ShutdownResponse {}))
    }

    async fn status(&self, _r: Request<StatusRequest>) -> Result<Response<StatusResponse>, Status> {
        Ok(Response::new(StatusResponse { status: Vec::new() }))
    }

    async fn reset_channel(&self, r: Request<ResetChannelRequest>) -> Result<Response<ResetChannelResponse>, Status> {
        let r = r.into_inner();
        let zone = zone_for(&r.device_id, &r.channel_id)?;
        let ctrl = self.ctrl.clone();
        tokio::task::spawn_blocking(move || ctrl.reset_zone(zone))
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .map_err(|e| {
                error!("Reset {} failed: {e:#}", zone.name());
                Status::internal(format!("{e:#}"))
            })?;
        info!("{} restored to the state found at plugin start", zone.name());
        Ok(Response::new(ResetChannelResponse {}))
    }

    async fn enable_manual_fan_control(&self, _r: Request<EnableManualFanControlRequest>) -> Result<Response<EnableManualFanControlResponse>, Status> {
        Err(Status::unimplemented("No Fans"))
    }

    async fn fixed_duty(&self, _r: Request<FixedDutyRequest>) -> Result<Response<FixedDutyResponse>, Status> {
        Err(Status::unimplemented("No Fans"))
    }

    async fn speed_profile(&self, _r: Request<SpeedProfileRequest>) -> Result<Response<SpeedProfileResponse>, Status> {
        Err(Status::unimplemented("No Firmware Profiles"))
    }

    async fn lighting(&self, r: Request<LightingRequest>) -> Result<Response<LightingResponse>, Status> {
        let r = r.into_inner();
        let zone = zone_for(&r.device_id, &r.channel_id)?;
        let setting = r.setting.ok_or_else(|| Status::invalid_argument("missing setting"))?;
        let colors: Vec<[u8; 3]> =
            setting.colors.iter().map(|c| [c.r.min(255) as u8, c.g.min(255) as u8, c.b.min(255) as u8]).collect();
        let zs = ZoneSetting::from_request(&setting.mode, setting.speed.as_deref(), &colors)
            .map_err(Status::invalid_argument)?;
        let ctrl = self.ctrl.clone();
        tokio::task::spawn_blocking(move || ctrl.set_zone(zone, &zs))
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .map_err(|e| {
                error!("Lighting on {} failed: {e:#}", zone.name());
                Status::internal(format!("{e:#}"))
            })?;
        info!("{}: mode={} speed={:?} colors={:?}", zone.name(), setting.mode, setting.speed, colors);
        Ok(Response::new(LightingResponse {}))
    }

    async fn lcd(&self, _r: Request<LcdRequest>) -> Result<Response<LcdResponse>, Status> {
        Err(Status::unimplemented("No LCD"))
    }

    async fn custom_function_one(&self, _r: Request<CustomFunctionOneRequest>) -> Result<Response<CustomFunctionOneResponse>, Status> {
        Err(Status::unimplemented("No Custom Function"))
    }
}
