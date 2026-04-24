use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::{PinDriver, Pins};
use esp_idf_hal::i2c::I2C0;
use esp_idf_hal::peripherals;
use esp_idf_hal::sys::EspError;
use esp_idf_hal::{
    i2c::{I2cConfig, I2cDriver},
    prelude::*,
};
use esp_idf_svc::mqtt::client::{EspMqttClient, EspMqttConnection, EventPayload, QoS};
use esp_idf_svc::nvs::{EspNvs, NvsDefault};
use log::{error, info};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{get_nvs_str, sh20_sensor, ID};

pub fn run(
    client: &mut EspMqttClient<'_>,
    connection: &mut EspMqttConnection,
    pins: Pins,
    i2c: I2C0,
    esp_nvs: EspNvs<NvsDefault>,
) -> anyhow::Result<(), EspError> {
    let nvs_obj = Arc::new(Mutex::new(esp_nvs));
    let nvs_obj1 = nvs_obj.clone();
    let nvs_obj2 = nvs_obj.clone();
    let nvs_obj3 = nvs_obj.clone();
    let nvs_obj4 = nvs_obj.clone();

    std::thread::scope(|s| {
        info!("About to start the MQTT client");
        // Need to immediately start pumping the connection for messages, or else subscribe() and publish() below will not work
        // Note that when using the alternative constructor - `EspMqttClient::new_cb` - you don't need to
        // spawn a new thread, as the messages will be pumped with a backpressure into the callback you provide.
        // Yet, you still need to efficiently process each message in the callback without blocking for too long.
        //
        // Note also that if you go to http://tools.emqx.io/ and then connect and send a message to topic
        // "esp-mqtt-demo", the client configured here should receive it.
        let mut led = PinDriver::output(pins.gpio4).unwrap();
        let sda = pins.gpio8;
        let scl = pins.gpio9;
        let config = I2cConfig::new().baudrate(100.kHz().into());
        let i2c_obj = I2cDriver::new(i2c, sda, scl, &config)?;
        let mut sensor = sh20_sensor::Sht20::new(i2c_obj);

        std::thread::Builder::new()
            .stack_size(6000)
            .spawn_scoped(s, move || {
                info!("MQTT Listening for messages");

                while let std::result::Result::Ok(event) = connection.next() {
                    info!("[Queue] Event: {}", event.payload());
                    match event.payload() {
                        EventPayload::BeforeConnect => {}
                        EventPayload::Connected(_) => {}
                        EventPayload::Disconnected => {}
                        EventPayload::Subscribed(_) => {}
                        EventPayload::Unsubscribed(_) => {}
                        EventPayload::Published(_) => {}
                        EventPayload::Received {
                            id,
                            topic,
                            data,
                            details,
                        } => {
                            let s = str::from_utf8(data).unwrap_or_default();
                            let topic_s = topic.unwrap_or_default();
                            error!("{}:{}", topic_s, s);
                            if s == "1" {
                                let _ = nvs_obj1.lock().unwrap().set_u8("on", 1).unwrap();
                                led.set_high().unwrap();
                            } else {
                                let _ = nvs_obj1.lock().unwrap().set_u8("on", 0).unwrap();
                                led.set_low().unwrap();
                            }
                        }
                        EventPayload::Deleted(_) => {}
                        EventPayload::Error(_) => {}
                    }
                }

                info!("Connection closed");
            })
            .unwrap();
        let control_topic = format!("control/{}", ID);
        let status_topic = format!("status/{}", ID);

        loop {
            if let Err(e) = client.subscribe(&control_topic, QoS::AtMostOnce) {
                error!(
                    "Failed to subscribe to topic \"{}\": {}, retrying...",
                    &control_topic, e
                );
                // Re-try in 0.5s
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
            info!("Subscribed to topic \"{}\"", &control_topic);
            std::thread::sleep(Duration::from_millis(500));
            let switch_on = nvs_obj.lock().unwrap().get_u8("on")?.unwrap_or(1);
            log::warn!("switch_on:{switch_on}");
            // let payload = "1";
            let mut temp = nvs_obj2
                .lock()
                .unwrap()
                .get_u32("temp")
                .unwrap_or_default()
                .unwrap_or_default();
            let mut hum = nvs_obj3
                .lock()
                .unwrap()
                .get_u32("hum")
                .unwrap_or_default()
                .unwrap_or_default();
            let On = if switch_on == 1 { "开" } else { "关" };
            loop {
                match sensor.read_temperature() {
                    Ok(v) => {
                        println!("Temperature: {:.2} °C", v);
                        temp = (v * 100.) as u32;
                        let _ = nvs_obj4.lock().unwrap().set_u32("temp", temp).unwrap();
                    }
                    Err(e) => {
                        println!("Temperature read error: {:?}", e);
                    }
                }
                match sensor.read_humidity() {
                    Ok(v) => {
                        println!("Humidity: {:.2} % RH", v);
                        hum = (v * 100.) as u32;
                        let _ = nvs_obj4.lock().unwrap().set_u32("hum", hum).unwrap();
                    }
                    Err(e) => {
                        println!("Humidity read error: {:?}", e);
                    }
                }
                let s = format!(
                    r#"[
                    {{"k": "T","v":"{:.2}"}},
                    {{"k": "H","v":"{:.2}"}},
                    {{"k": "On","v":"{}"}}
                  ]"#,
                    (temp as f32) / 100.,
                    (hum as f32) / 100.,
                    On
                );
                if temp > 0 && hum > 0 {
                    client.enqueue(&status_topic, QoS::AtMostOnce, false, s.as_bytes())?;
                    info!("Published \"{s}\" to topic \"{}\"", &status_topic);
                    info!("Now sleeping for 60s...");
                }
                FreeRtos::delay_ms(60000);
            }
        }
    })
}
