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
    // Remembered from the last successful probe; defaults to the primary address until known.
    let mut imu_addr: Option<u8> = None;
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
