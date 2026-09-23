use core::ops::BitOr;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::peripherals::{P1_11, P1_12, TWISPI1, UARTE1};
use embassy_nrf::twim::{self, Twim};
use embassy_nrf::{bind_interrupts, Peri, Peripherals};
use embassy_time::{Duration, Timer};

bind_interrupts!(struct Irqs {
    TWISPI1 => twim::InterruptHandler<TWISPI1>;
});

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

/// External header pins for UART communication
pub struct HeaderPins {
    /// TX Pin (D6 on XIAO Sense / MCU P1.11)
    pub d6_tx: Peri<'static, P1_11>,
    /// RX Pin (D7 on XIAO Sense / MCU P1.12)
    pub d7_rx: Peri<'static, P1_12>,
}

/// Board Support Package abstraction for Seeed Studio XIAO nRF52840 (Sense)
pub struct Board {
    pub leds: Leds,
    pub pins: HeaderPins,
    pub uarte1: Peri<'static, UARTE1>,
    /// Internal I2C bus wired to the onboard LSM6DS3TR-C IMU (not exposed on the header)
    pub twim: Twim<'static>,
    /// IMU/mic power rail enable (MCU P1.08) - must be driven high before the IMU responds
    pub imu_power: Output<'static>,
}

impl Board {
    /// Initialize the XIAO Sense board peripherals from the HAL singleton.
    pub async fn init(p: Peripherals) -> Self {
        let imu_power = Output::new(p.P1_08, Level::High, OutputDrive::HighDrive);
        // Let the IMU/mic power rail settle before the bus is used.
        Timer::after(Duration::from_millis(10)).await;
        let twim = Twim::new(
            p.TWISPI1,
            Irqs,
            p.P0_07,
            p.P0_27,
            twim::Config::default(),
            &mut [],
        );

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
            },
            uarte1: p.UARTE1,
            twim,
            imu_power,
        }
    }
}
