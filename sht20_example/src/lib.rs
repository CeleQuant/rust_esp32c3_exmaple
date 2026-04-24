use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::{PinDriver, Pins};
use esp_idf_svc::eventloop::{self, EspSystemEventLoop};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_sys;
use std::sync::{Arc, Mutex};
use std::time::Duration;
//
use esp_idf_hal::io::Write;
use esp_idf_svc::http::server::{Configuration as HttpServerConfig, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi,
};
use log::{error, warn};
use urlencoding::{decode, encode};
pub mod env;
pub mod mqtt_client;
pub mod sh20_sensor;
pub mod sta_wifi;

use env::*;

/**
 * 系统初始化函数。
 *
 * 该函数负责初始化系统级的服务和硬件外设，为应用程序提供基础的运行环境。
 *
 * @return Result<(EspSystemEventLoop, Peripherals, EspDefaultNvsPartition)> 初始化完成后的系统事件循环、外设句柄和默认NVS分区。
 */
pub fn init() -> anyhow::Result<(EspSystemEventLoop, Peripherals, EspDefaultNvsPartition)> {
    // 链接SDK中的补丁，以修正某些功能的兼容性问题。
    esp_idf_svc::sys::link_patches();

    // 初始化日志系统，为后续的调试和错误追踪提供支持。
    esp_idf_svc::log::EspLogger::initialize_default();

    // 获取系统事件循环实例，用于处理系统级别的事件。
    let sysloop = esp_idf_svc::eventloop::EspSystemEventLoop::take()?;

    // 获取外设句柄，用于访问和控制硬件资源。
    let peripherals = Peripherals::take()?;

    // 获取默认的NVS分区，用于存储配置数据和运行时信息。
    let nvs = EspDefaultNvsPartition::take()?;

    // 返回初始化完成的系统事件循环、外设句柄和默认NVS分区。
    Ok((sysloop, peripherals, nvs))
}

pub fn get_nvs_str(nvs: Arc<Mutex<EspNvs<NvsDefault>>>, name: &str) -> anyhow::Result<String> {
    let mut buf = [0u8; 256];
    match nvs.lock().unwrap().get_str(name, &mut buf)? {
        Some(s) => {
            let trimmed_s = s.trim_end_matches(char::from(0));
            Ok(trimmed_s.to_string())
        }
        None => Ok("".to_string()),
    }
}

pub fn reboot() {
    error!("reboot.....");
    unsafe {
        esp_idf_sys::esp_restart();
    }
}

pub fn mode0(
    peripherals: Peripherals,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<()> {
    // let peripherals = Arc::new(Mutex::new(peripherals));
    let Peripherals {
        pins,
        uart0,
        uart1,
        i2c0,
        i2s0,
        spi1,
        spi2,
        adc1,
        adc2,
        can,
        ledc,
        rmt,
        modem,
        temp_sensor,
        timer00,
        timer10,
        twdt,
        usb_serial,
    } = peripherals;
    let nvs_obj = Arc::new(Mutex::new(EspNvs::new(nvs.clone(), "esp32", true)?));
    let ssid = get_nvs_str(nvs_obj.clone(), "ssid")?;
    let pwd = get_nvs_str(nvs_obj.clone(), "pwd")?;
    let ssid = &ssid[..];
    let pwd = &pwd[..];
    let _wifi = sta_wifi::connect_wifi(ssid, pwd, modem, sysloop, nvs)?;
    let (mut client, mut conn) = EspMqttClient::new(
        MQTT_NODE,
        &MqttClientConfiguration {
            client_id: Some(ID.into()),
            reconnect_timeout: Some(Duration::from_secs(10)),
            username: Some(MQTT_USER),
            password: Some(MQTT_PWD),
            ..Default::default()
        },
    )?;
    // mqtt_client::run(&mut client, &mut conn, pins, peripherals)?;
    // let mut server = EspHttpServer::new(&HttpServerConfig::default())?;
    // server.fn_handler("/", Method::Get, move |req| {
    //     let mut resp = req.into_ok_response()?;
    //     let template = include_str!("index.html").to_string();
    //     resp.write_all(template.as_bytes())?;
    //     anyhow::Ok(())
    // })?;
    // warn!("HTTP server running");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
    Ok(())
}

// 配网模式
pub fn mode1(
    peripherals: Peripherals,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<()> {
    let wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs.clone()))?,
        sysloop,
    )?;
    let wifi_obj = Arc::new(Mutex::new(wifi));
    //
    let esp_nvs = EspNvs::new(nvs.clone(), AP_SSID, true)?;
    let nvs_obj = Arc::new(Mutex::new(esp_nvs));
    // nvs_obj.lock().unwrap().remove("ssid")?;
    // nvs_obj.lock().unwrap().remove("pwd")?;
    //
    let nvs_temp = nvs_obj.clone();
    let wifi_temp = wifi_obj.clone();
    let ssids: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    ap_sta_mode(wifi_temp.clone(), nvs_temp.clone(), ssids.clone())?;
    let ssids_temp = ssids.clone();
    //
    // warn!("{:?}", ssids.lock().unwrap());
    //
    let mut server = EspHttpServer::new(&HttpServerConfig::default())?;
    server
        .fn_handler("/", Method::Get, move |req| {
            let mut resp = req.into_ok_response()?;
            let template = include_str!("index.html").to_string();
            let s = format!("{:?}", ssids_temp.lock().unwrap());
            let html = template.replace("{{ssids}}", &s);
            resp.write_all(html.as_bytes())?;
            anyhow::Ok(())
        })?
        .fn_handler("/wifi", Method::Post, move |mut req| {
            let mut buf = [0u8; 256];
            let bytes_read = req.read(&mut buf)?;
            let received_data = String::from_utf8_lossy(&buf[..bytes_read]);
            let split1: Vec<&str> = received_data.split("&").collect();
            let ssid_split: Vec<&str> = split1[0].split("ssid=").collect();
            let pwd_split: Vec<&str> = split1[1].split("password=").collect();
            warn!("{:?},{:?}", ssid_split, pwd_split);
            let ssid_s = decode(ssid_split[1])?.to_string();
            let pwd_s = decode(pwd_split[1])?.to_string();
            warn!("Received POST data: {},{},{}", received_data, ssid_s, pwd_s);
            nvs_temp.lock().unwrap().set_str("ssid", &ssid_s)?;
            nvs_temp.lock().unwrap().set_str("pwd", &pwd_s)?;
            nvs_temp.lock().unwrap().set_u8("net_mode", 0)?;
            reboot();
            let mut resp = req.into_ok_response()?;
            let success_html = r#"
            <!DOCTYPE html><html><head>
                <meta charset='UTF-8'>
                <meta name='viewport' content='width=device-width, initial-scale=1.0'>
                <meta http-equiv='content-type' content='text/html; charset=UTF-8' />
            </head>
            <h1>设置成功</h1>
            <h1>Success</h1>
            </html>
            "#;
            resp.write_all(&success_html.as_bytes())?;
            // ap_sta_mode(wifi_temp.clone(), nvs_temp.clone())?;
            anyhow::Ok(())
        })?;
    warn!("HTTP server running");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
    Ok(())
}

fn ap_sta_mode(
    wifi: Arc<Mutex<BlockingWifi<EspWifi>>>,
    nvs: Arc<Mutex<EspNvs<NvsDefault>>>,
    ssids: Arc<Mutex<Vec<String>>>,
) -> anyhow::Result<()> {
    let mut wifi = wifi.lock().unwrap();
    let mut retry = 0;
    loop {
        if retry < 0 {
            error!("retry {retry} times, connect wifi failed");
            break;
        }
        let ssid = get_nvs_str(nvs.clone(), "ssid")?;
        let pwd = get_nvs_str(nvs.clone(), "pwd")?;
        let ssid = &ssid[..];
        let pwd = &pwd[..];
        if ssid.len() == 0 || pwd.len() == 0 {
            break;
        }
        error!("set mixed wifi, sta: {ssid}:{pwd},ap:{AP_SSID}:{AP_PWD}");
        let _ = wifi.set_configuration(&Configuration::Mixed(
            ClientConfiguration {
                ssid: ssid.try_into().unwrap(),
                auth_method: AuthMethod::WPA2Personal,
                password: pwd.try_into().unwrap(),
                ..Default::default()
            },
            AccessPointConfiguration {
                ssid: AP_SSID.try_into().unwrap(),
                password: AP_SSID.try_into().unwrap(),
                channel: 1,
                auth_method: AuthMethod::WPA2Personal,
                max_connections: 4,
                ..Default::default()
            },
        ));
        warn!("启动Wi-Fi");
        wifi.start()?;
        // 扫描可用的Wi-Fi网络。
        log::info!("扫描Wi-Fi");
        let mut arrs: Vec<String> = vec![];
        let access_point_infos = wifi.scan()?;
        // 打印扫描结果。
        log::info!("扫描到的Wi-Fi数量: {}", access_point_infos.len());
        access_point_infos.into_iter().for_each(|info| {
            println!("{:#?}", info.ssid);
            let s = info.ssid.to_string();
            if !ssids.lock().unwrap().contains(&s) {
                ssids.lock().unwrap().push(s);
            }
        });
        warn!("连接Wi-Fi");

        match wifi.connect() {
            Ok(_) => {}
            Err(e) => {
                error!("wifi connect error,max retry {retry}:{e}");
                retry -= 1;
                FreeRtos::delay_ms(1000);
                continue;
            }
        }
        warn!("等待网络接口启动");
        wifi.wait_netif_up()?;
        warn!("ap IP信息: {:?}", wifi.wifi().ap_netif().get_ip_info()?);
        warn!("sta IP信息: {:?}", wifi.wifi().sta_netif().get_ip_info()?);
        break;
    }
    Ok(())
}
