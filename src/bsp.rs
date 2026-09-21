use core::ops::BitOr;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::peripherals::{P0_04, P0_05, P1_11, P1_12, TWISPI0, UARTE1};
use embassy_nrf::{Peri, Peripherals};

/// 3-bit weighted RGB color enum representing all 8 additive color combinations.
///
/// Bit 0 (`0b001`) = Red
/// Bit 1 (`0b010`) = Green
/// Bit 2 (`0b100`) = Blue
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, defmt::Format)]
#[repr(u8)]
pub enum Color {
    /// No LEDs active (`0b000`)
    Off = 0b000,
    /// Red only (`0b001`)
    Red = 0b001,
    /// Green only (`0b010`)
    Green = 0b010,
    /// Red + Green (`0b011`)
    Yellow = 0b011,
    /// Blue only (`0b100`)
    Blue = 0b100,
    /// Red + Blue (`0b101`)
    Magenta = 0b101,
    /// Green + Blue (`0b110`)
    Cyan = 0b110,
    /// Red + Green + Blue (`0b111`)
    White = 0b111,
}

#[allow(dead_code)]
impl Color {
    pub const fn red_active(self) -> bool {
        (self as u8 & 0b001) != 0
    }

    pub const fn green_active(self) -> bool {
        (self as u8 & 0b010) != 0
    }

    pub const fn blue_active(self) -> bool {
        (self as u8 & 0b100) != 0
    }

    pub const fn from_bits(bits: u8) -> Self {
        match bits & 0b111 {
            1 => Color::Red,
            2 => Color::Green,
            3 => Color::Yellow,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::White,
            _ => Color::Off,
        }
    }
}

impl BitOr for Color {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Color::from_bits((self as u8) | (rhs as u8))
    }
}

/// Onboard RGB LEDs (Active-Low: LOW = ON, HIGH = OFF)
#[allow(dead_code)]
pub struct Leds {
    /// Red LED (P0.26 - NOTE: shared on PCB with external header pin D7)
    pub red: Output<'static>,
    /// Green LED (P0.30 - NOTE: shared on PCB with external header pin D8)
    pub green: Output<'static>,
    /// Blue LED (P0.06 - dedicated onboard LED, no external pin conflict)
    pub blue: Output<'static>,
}

#[allow(dead_code)]
impl Leds {
    /// Set all three onboard LEDs according to the given composite color.
    pub fn set_color(&mut self, color: Color) {
        // Active-low: LOW enables the LED, HIGH disables it
        if color.red_active() { self.red.set_low(); } else { self.red.set_high(); }
        if color.green_active() { self.green.set_low(); } else { self.green.set_high(); }
        if color.blue_active() { self.blue.set_low(); } else { self.blue.set_high(); }
    }
}

/// External header pins for UART and I2C communication
pub struct HeaderPins {
    /// TX Pin (D6 on XIAO Sense / MCU P1.11)
    pub d6_tx: Peri<'static, P1_11>,
    /// RX Pin (D7 on XIAO Sense / MCU P1.12)
    pub d7_rx: Peri<'static, P1_12>,
    /// I2C SDA Pin (D4 on XIAO Sense / MCU P0.04) - shared with the onboard IMU
    pub d4_sda: Peri<'static, P0_04>,
    /// I2C SCL Pin (D5 on XIAO Sense / MCU P0.05) - shared with the onboard IMU
    pub d5_scl: Peri<'static, P0_05>,
}

/// Board Support Package abstraction for Seeed Studio XIAO nRF52840 (Sense)
pub struct Board {
    pub leds: Leds,
    pub pins: HeaderPins,
    pub uarte1: Peri<'static, UARTE1>,
    /// I2C/TWI peripheral shared with the onboard LSM6DS3TR-C IMU
    pub twispi0: Peri<'static, TWISPI0>,
}

impl Board {
    /// Initialize the XIAO Sense board peripherals from the HAL singleton.
    pub fn init(p: Peripherals) -> Self {
        Self {
            leds: Leds {
                // Initialize all LEDs as OFF (High)
                red: Output::new(p.P0_26, Level::High, OutputDrive::Standard),
                green: Output::new(p.P0_30, Level::High, OutputDrive::Standard),
                blue: Output::new(p.P0_06, Level::High, OutputDrive::Standard),
            },
            pins: HeaderPins {
                d6_tx: p.P1_11,
                d7_rx: p.P1_12,
                d4_sda: p.P0_04,
                d5_scl: p.P0_05,
            },
            uarte1: p.UARTE1,
            twispi0: p.TWISPI0,
        }
    }
}
