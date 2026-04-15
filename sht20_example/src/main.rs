use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::{PinDriver, Pins};

use log::{error, info};
use std::time::Duration;

use anyhow::Result;
use esp_idf_hal::{
    i2c::{I2cConfig, I2cDriver},
    prelude::*,
};

// SHT20 的默认 I2C 地址 (7-bit)
const SHT20_ADDR: u8 = 0x40;
// 命令定义
const TRIGGER_TEMP_MEASURE_HOLD: u8 = 0xE3;
const TRIGGER_HUMID_MEASURE_HOLD: u8 = 0xE5;

/// SHT20 驱动结构体
pub struct Sht20<'a> {
    i2c: I2cDriver<'a>,
}

impl<'a> Sht20<'a> {
    pub fn new(i2c: I2cDriver<'a>) -> Self {
        Sht20 { i2c }
    }

    /// 读取原始数据 (16位)
    fn read_raw(&mut self, cmd: u8) -> Result<u16> {
        // 1. 发送测量命令
        self.i2c.write(SHT20_ADDR, &[cmd], 1000)?;

        // 2. 等待测量完成 (SHT20 典型测量时间约 30-85ms)
        // 这里等待 100ms 以确保数据就绪
        std::thread::sleep(std::time::Duration::from_millis(100));

        // 3. 读取 3 字节数据: [高8位, 低8位, CRC校验]
        let mut buffer = [0u8; 3];
        self.i2c.read(SHT20_ADDR, &mut buffer, 1000)?;

        let raw = ((buffer[0] as u16) << 8) | (buffer[1] as u16);
        // 去除状态位 (SHT20 数据为 14bit 有效，低两位是状态位)
        Ok(raw & 0xFFFC)
    }

    /// 读取温度 (°C)
    pub fn read_temperature(&mut self) -> Result<f32> {
        let raw = self.read_raw(TRIGGER_TEMP_MEASURE_HOLD)?;
        // 转换公式: T = -46.85 + (175.72 * raw / 2^16)
        let temp = -46.85 + (175.72 * (raw as f32) / 65536.0);
        Ok(temp)
    }

    /// 读取湿度 (% RH)
    pub fn read_humidity(&mut self) -> Result<f32> {
        let raw = self.read_raw(TRIGGER_HUMID_MEASURE_HOLD)?;
        // 转换公式: RH = -6.0 + (125.0 * raw / 2^16)
        let humidity = -6.0 + (125.0 * (raw as f32) / 65536.0);
        // 湿度值限制在 0-100% 之间
        Ok(humidity.clamp(0.0, 100.0))
    }
}

fn main() -> anyhow::Result<()> {
    info!("Hello, world!");
    let (sysloop, peripherals, nvs) = led_esp32c3::init().unwrap();
    // let mut led = PinDriver::output(peripherals.pins.gpio4)?;
    // loop {
    //     led.set_high()?;
    //     FreeRtos::delay_ms(1000);
    //     led.set_low()?;
    //     FreeRtos::delay_ms(1000);
    // }
    let sda = peripherals.pins.gpio8;
    let scl = peripherals.pins.gpio9;
    let config = I2cConfig::new().baudrate(100.kHz().into());
    let i2c = I2cDriver::new(peripherals.i2c0, sda, scl, &config)?;

    let mut sensor = Sht20::new(i2c);

    loop {
        match sensor.read_temperature() {
            Ok(temp) => println!("Temperature: {:.2} °C", temp),
            Err(e) => println!("Temperature read error: {:?}", e),
        }

        match sensor.read_humidity() {
            Ok(hum) => println!("Humidity: {:.2} % RH", hum),
            Err(e) => println!("Humidity read error: {:?}", e),
        }

        println!("---");
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
