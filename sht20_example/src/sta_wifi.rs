use esp_idf_hal::{delay::FreeRtos, peripheral::Peripheral};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault},
};
use std::sync::{Arc, Mutex};
//
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use log::{error, warn};

use crate::{get_nvs_str, reboot};

pub fn main(
    wifi: Arc<Mutex<BlockingWifi<EspWifi>>>,
    nvs: Arc<Mutex<EspNvs<NvsDefault>>>,
) -> anyhow::Result<bool> {
    let mut wifi = wifi.lock().unwrap();
    if wifi.is_connected()? {
        warn!("wifi已连接");
    }
    let mut retry = 3;
    loop {
        if retry < 0 {
            error!("retry {retry} times, connect wifi failed");
            nvs.clone().lock().unwrap().set_u8("net_mode", 1)?;
            reboot();
        }
        let ssid = get_nvs_str(nvs.clone(), "ssid")?;
        let pwd = get_nvs_str(nvs.clone(), "pwd")?;
        let ssid = &ssid[..];
        let pwd = &pwd[..];
        let _ = wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: ssid.try_into().unwrap(),
            auth_method: AuthMethod::WPA2Personal,
            password: pwd.try_into().unwrap(),
            ..Default::default()
        }))?;
        warn!("启动Wi-Fi");
        wifi.start()?;
        warn!("连接Wi-Fi");
        match wifi.connect() {
            Ok(_) => {
                warn!("等待网络接口启动");
                wifi.wait_netif_up()?;
                warn!("sta IP信息: {:?}", wifi.wifi().sta_netif().get_ip_info()?);
                break;
                // let mut i = 0;
                // loop {
                //     FreeRtos::delay_ms(1000);
                //     i += 1;
                //     println!("run {i}")
                // }
            }
            Err(e) => {
                error!("wifi connect error,retry {retry} :{e}");
                retry -= 1;
                FreeRtos::delay_ms(1000);
                continue;
            }
        }
    }
    Ok(true)
}

pub fn connect_wifi(
    ssid: &str,
    psk: &str,
    modem: impl Peripheral<P = esp_idf_svc::hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<Box<EspWifi<'static>>> {
    // 初始化EspWifi实例。
    let mut esp_wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;
    // 将EspWifi封装为BlockingWifi，以便可以使用阻塞模式的API。
    let mut wifi = BlockingWifi::wrap(&mut esp_wifi, sysloop)?;

    // 配置Wi-Fi连接参数，包括SSID、认证方法和密码。
    let configuration = Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().unwrap(),
        auth_method: AuthMethod::WPA2Personal,
        password: psk.try_into().unwrap(),
        ..Default::default()
    });
    // 应用配置。
    wifi.set_configuration(&configuration)?;
    // 启动Wi-Fi模块。
    log::info!("启动Wi-Fi");
    wifi.start()?;
    // 扫描可用的Wi-Fi网络。
    log::info!("扫描Wi-Fi");
    let access_point_infos = wifi.scan()?;
    // 打印扫描结果。
    log::info!("扫描到的Wi-Fi数量: {}", access_point_infos.len());
    access_point_infos
        .into_iter()
        .for_each(|info| println!("{:#?}", info));

    // 尝试连接到配置的Wi-Fi网络。
    log::info!("连接Wi-Fi");
    wifi.connect()?;
    // 确认Wi-Fi连接已建立。
    log::info!("Wi-Fi已连接");
    // 等待网络接口启动。
    wifi.wait_netif_up()?;

    // 获取连接后的IP地址信息。
    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    // 打印IP地址信息。
    log::info!("IP信息: {:?}", ip_info);

    // 返回封装了Wi-Fi模块的实例。
    Ok(Box::new(esp_wifi))
}
