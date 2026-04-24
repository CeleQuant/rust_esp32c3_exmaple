use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};
use esp_idf_svc::nvs::EspNvs;
use led_esp32c3::env::*;
use led_esp32c3::mqtt_client;
use log::info;
use std::time::Duration;

use esp_idf_hal::prelude::*;

fn main() -> anyhow::Result<()> {
    info!("Hello, world!");
    let (sysloop, peripherals, nvs) = led_esp32c3::init()?;
    let esp_nvs = EspNvs::new(nvs.clone(), "esp32", true)?;
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
    // let _wifi = led_esp32c3::sta_wifi::connect_wifi("FHCPE-XIAO", "19941023", modem, sysloop, nvs)?;
    let _wifi = led_esp32c3::sta_wifi::connect_wifi(WIFI_NAME, WIFI_PWD, modem, sysloop, nvs)?;
    let mqtt_config = MqttClientConfiguration {
        client_id: Some(ID.into()),
        reconnect_timeout: Some(Duration::from_secs(10)),
        username: Some(MQTT_USER),
        password: Some(MQTT_PWD),
        ..Default::default()
    };
    let (mut client, mut conn) = EspMqttClient::new(MQTT_NODE, &mqtt_config)?;
    mqtt_client::run(&mut client, &mut conn, pins, i2c0, esp_nvs)?;
    loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}
