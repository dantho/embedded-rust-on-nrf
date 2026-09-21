use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_nrf::bind_interrupts;
use embassy_nrf::peripherals;
use embassy_nrf::twim::{self, Twim};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_time::{with_timeout, Duration};
use heapless::String;

use crate::main_uart::{UartTxMsg, UartTxSender};

bind_interrupts!(pub struct Irqs {
    TWISPI1 => twim::InterruptHandler<peripherals::TWISPI1>;
});

// Onboard LSM6DS3TR-C IMU on the XIAO nRF52840 Sense (internal I2C1 bus, not on the header).
const IMU_ADDR_PRIMARY: u8 = 0x6A;
const IMU_ADDR_SECONDARY: u8 = 0x6B;
const WHO_AM_I_REG: u8 = 0x0F;
const WHO_AM_I_EXPECTED: u8 = 0x6A;

// Boot-time config, copied from ../xiao-blinky: 104 Hz ODR, +/-2g accel FS, +/-250dps gyro FS.
const CTRL1_XL_REG: u8 = 0x10;
const CTRL2_G_REG: u8 = 0x11;
const ODR_104HZ_2G_250DPS: u8 = 0x40;

// Gyro X/Y/Z then accel X/Y/Z, 2 bytes each, auto-incrementing from OUTX_L_G.
const OUT_START_REG: u8 = 0x22;
// Sensitivity at the configured +/-2g / +/-250dps full-scale range (LSM6DS3TR-C datasheet).
const ACCEL_G_PER_LSB: f32 = 0.000061;
const GYRO_DPS_PER_LSB: f32 = 0.00875;

// A stuck/floating I2C bus never raises STOPPED/ERROR, so bound every transaction.
const I2C_TIMEOUT: Duration = Duration::from_millis(100);

#[derive(Clone, Copy)]
enum ImuIoError {
    Bus(twim::Error),
    Timeout,
}

#[derive(Clone, Copy)]
pub enum ImuCommand {
    Status,
    ReadReg { reg: u8 },
    WriteReg { reg: u8, value: u8 },
    Sample,
}

static IMU_CHANNEL: Channel<CriticalSectionRawMutex, ImuCommand, 4> = Channel::new();

pub type ImuSender = Sender<'static, CriticalSectionRawMutex, ImuCommand, 4>;

pub fn sender() -> ImuSender {
    IMU_CHANNEL.sender()
}

pub fn spawn(spawner: Spawner, twim: Twim<'static>, uart_tx: UartTxSender) {
    spawner.must_spawn(imu_task(twim, uart_tx));
}

#[embassy_executor::task]
async fn imu_task(mut twim: Twim<'static>, uart_tx: UartTxSender) {
    defmt::info!("IMU task started.");
    // Remembered from boot probe or the last `imu-status`; defaults to the primary address until known.
    let mut imu_addr: Option<u8> = init_imu(&mut twim, &uart_tx).await;
    loop {
        let command = IMU_CHANNEL.receive().await;
        let mut msg: String<64> = String::new();
        match command {
            ImuCommand::Status => match probe(&mut twim).await {
                Some((addr, value)) if value == WHO_AM_I_EXPECTED => {
                    imu_addr = Some(addr);
                    let _ = write!(
                        msg,
                        "IMU detected at 0x{:02X}: WHO_AM_I=0x{:02X}\n",
                        addr, value
                    );
                }
                Some((addr, value)) => {
                    imu_addr = Some(addr);
                    let _ = write!(
                        msg,
                        "IMU mismatch at 0x{:02X}: WHO_AM_I=0x{:02X} (expected 0x{:02X})\n",
                        addr, value, WHO_AM_I_EXPECTED
                    );
                }
                None => {
                    imu_addr = None;
                    let _ = write!(msg, "IMU not found at 0x6A or 0x6B.\n");
                }
            },
            ImuCommand::ReadReg { reg } => {
                let addr = imu_addr.unwrap_or(IMU_ADDR_PRIMARY);
                match read_reg(&mut twim, addr, reg).await {
                    Ok(value) => {
                        let _ = write!(msg, "IMU[0x{:02X}] = 0x{:02X}\n", reg, value);
                    }
                    Err(ImuIoError::Timeout) => {
                        let _ = write!(msg, "IMU read timed out at 0x{:02X}\n", reg);
                    }
                    Err(ImuIoError::Bus(e)) => {
                        defmt::warn!("IMU read bus error at {=u8:#04x}: {}", reg, e);
                        let _ = write!(msg, "IMU read error at 0x{:02X}\n", reg);
                    }
                }
            }
            ImuCommand::WriteReg { reg, value } => {
                let addr = imu_addr.unwrap_or(IMU_ADDR_PRIMARY);
                match write_reg(&mut twim, addr, reg, value).await {
                    Ok(()) => {
                        let _ = write!(msg, "IMU[0x{:02X}] <= 0x{:02X}\n", reg, value);
                    }
                    Err(ImuIoError::Timeout) => {
                        let _ = write!(msg, "IMU write timed out at 0x{:02X}\n", reg);
                    }
                    Err(ImuIoError::Bus(e)) => {
                        defmt::warn!("IMU write bus error at {=u8:#04x}: {}", reg, e);
                        let _ = write!(msg, "IMU write error at 0x{:02X}\n", reg);
                    }
                }
            }
            ImuCommand::Sample => {
                let addr = imu_addr.unwrap_or(IMU_ADDR_PRIMARY);
                match read_sample(&mut twim, addr).await {
                    Ok([gx, gy, gz, ax, ay, az]) => {
                        let _ = write!(
                            msg,
                            "A:{:+.2},{:+.2},{:+.2}g G:{:+.1},{:+.1},{:+.1}dps\n",
                            ax, ay, az, gx, gy, gz
                        );
                    }
                    Err(ImuIoError::Timeout) => {
                        let _ = write!(msg, "IMU sample read timed out\n");
                    }
                    Err(ImuIoError::Bus(e)) => {
                        defmt::warn!("IMU sample bus error: {}", e);
                        let _ = write!(msg, "IMU sample read error\n");
                    }
                }
            }
        }
        uart_tx.send(UartTxMsg::from(msg)).await;
    }
}

/// Probe the primary address, then the secondary, returning whichever answered.
async fn probe(twim: &mut Twim<'static>) -> Option<(u8, u8)> {
    if let Ok(value) = read_reg(twim, IMU_ADDR_PRIMARY, WHO_AM_I_REG).await {
        return Some((IMU_ADDR_PRIMARY, value));
    }
    if let Ok(value) = read_reg(twim, IMU_ADDR_SECONDARY, WHO_AM_I_REG).await {
        return Some((IMU_ADDR_SECONDARY, value));
    }
    None
}

/// Probe and apply the boot-time ODR/range config once, reporting the outcome over UART.
async fn init_imu(twim: &mut Twim<'static>, uart_tx: &UartTxSender) -> Option<u8> {
    let mut msg: String<64> = String::new();
    let imu_addr = match probe(twim).await {
        Some((addr, who_am_i)) => {
            match configure(twim, addr).await {
                Ok(()) => {
                    let _ = write!(
                        msg,
                        "IMU init: 0x{:02X} WHO_AM_I=0x{:02X} configured OK\n",
                        addr, who_am_i
                    );
                }
                Err(_) => {
                    let _ = write!(
                        msg,
                        "IMU init: 0x{:02X} WHO_AM_I=0x{:02X} config FAILED\n",
                        addr, who_am_i
                    );
                }
            }
            Some(addr)
        }
        None => {
            let _ = write!(msg, "IMU init: not found at 0x6A or 0x6B\n");
            None
        }
    };
    uart_tx.send(UartTxMsg::from(msg)).await;
    imu_addr
}

/// Set accelerometer/gyro output data rate and full-scale range.
async fn configure(twim: &mut Twim<'static>, addr: u8) -> Result<(), ImuIoError> {
    write_reg(twim, addr, CTRL1_XL_REG, ODR_104HZ_2G_250DPS).await?;
    write_reg(twim, addr, CTRL2_G_REG, ODR_104HZ_2G_250DPS).await?;
    Ok(())
}

/// Read the gyro+accel burst and convert to dps/g using the configured full-scale range.
async fn read_sample(twim: &mut Twim<'static>, addr: u8) -> Result<[f32; 6], ImuIoError> {
    let mut buf = [0u8; 12];
    // A bare `&[OUT_START_REG]` is a const array literal the compiler can place in flash,
    // which DMA can't read; bind it to a local so it's guaranteed to live in RAM.
    let start_reg = [OUT_START_REG];
    with_timeout(I2C_TIMEOUT, twim.write_read(addr, &start_reg, &mut buf))
        .await
        .map_err(|_| ImuIoError::Timeout)?
        .map_err(ImuIoError::Bus)?;

    let axis = |lo: usize| i16::from_le_bytes([buf[lo], buf[lo + 1]]) as f32;
    Ok([
        axis(0) * GYRO_DPS_PER_LSB,
        axis(2) * GYRO_DPS_PER_LSB,
        axis(4) * GYRO_DPS_PER_LSB,
        axis(6) * ACCEL_G_PER_LSB,
        axis(8) * ACCEL_G_PER_LSB,
        axis(10) * ACCEL_G_PER_LSB,
    ])
}

async fn read_reg(twim: &mut Twim<'static>, addr: u8, reg: u8) -> Result<u8, ImuIoError> {
    let mut buf = [0u8; 1];
    with_timeout(I2C_TIMEOUT, twim.write_read(addr, &[reg], &mut buf))
        .await
        .map_err(|_| ImuIoError::Timeout)?
        .map_err(ImuIoError::Bus)?;
    Ok(buf[0])
}

async fn write_reg(twim: &mut Twim<'static>, addr: u8, reg: u8, value: u8) -> Result<(), ImuIoError> {
    with_timeout(I2C_TIMEOUT, twim.write(addr, &[reg, value]))
        .await
        .map_err(|_| ImuIoError::Timeout)?
        .map_err(ImuIoError::Bus)
}
