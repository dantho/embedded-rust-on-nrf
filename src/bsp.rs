use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::peripherals::{P1_11, P1_12, UARTE1};
use embassy_nrf::{Peri, Peripherals};

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
            },
            uarte1: p.UARTE1,
        }
    }
}
