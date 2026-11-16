//! Development helper: talks to a running plugin over its socket like coolercontrold does.
//! cargo run --example probe -- <socket> [channel mode [RRGGBB]]
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
use device_service::v1::device_service_client::DeviceServiceClient;
use device_service::v1::*;
use hyper_util::rt::TokioIo;
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let socket = args.get(1).cloned().unwrap_or_else(|| "/run/coolercontrol-plugin-msi_mysticlight.sock".into());
    let channel = Endpoint::try_from("http://[::]:50051")?
        .connect_with_connector(service_fn(move |_: Uri| {
            let socket = socket.clone();
            async move { Ok::<_, std::io::Error>(TokioIo::new(tokio::net::UnixStream::connect(socket).await?)) }
        }))
        .await?;
    let mut client = DeviceServiceClient::new(channel);
    println!("Health: {:?}", client.health(HealthRequest {}).await?.into_inner());
    let devices = client.list_devices(ListDevicesRequest {}).await?.into_inner();
    for d in &devices.devices {
        let info = d.info.as_ref().unwrap();
        println!("Device {} '{}' uid={:?} model={:?} speeds={:?} driver={:?}", d.id, d.name, d.uid_info, info.model, info.lighting_speeds, info.driver_info);
        for (id, ch) in &info.channels {
            println!("  channel {id}: {:?}", ch.label);
            if let Some(models::v1::channel_info::Options::LightingModes(m)) = &ch.options {
                println!("    {} modes: {}", m.lighting_mode.len(), m.lighting_mode.iter().map(|x| format!("{}[{}-{}]", x.name, x.min_colors, x.max_colors)).collect::<Vec<_>>().join(", "));
            }
        }
    }
    if let (Some(ch), Some(mode)) = (args.get(2), args.get(3)) {
        let colors = args.get(4).map(|c| {
            let v = u32::from_str_radix(c, 16).unwrap();
            vec![Rgb { r: (v >> 16) & 255, g: (v >> 8) & 255, b: v & 255 }]
        }).unwrap_or_default();
        let r = client
            .lighting(LightingRequest {
                device_id: "msi_mysticlight".into(),
                channel_id: ch.clone(),
                setting: Some(LightingSetting { mode: mode.clone(), speed: None, backward: None, colors }),
            })
            .await;
        println!("Lighting: {r:?}");
    }
    Ok(())
}
