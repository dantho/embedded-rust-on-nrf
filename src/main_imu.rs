use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_nrf::bind_interrupts;
use embassy_nrf::peripherals;
use embassy_nrf::twim::{self, Twim};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use heapless::String;

use crate::main_uart::{UartTxMsg, UartTxSender};

bind_interrupts!(pub struct Irqs {
    TWISPI0 => twim::InterruptHandler<peripherals::TWISPI0>;
});

// Onboard LSM6DS3TR-C IMU on the XIAO nRF52840 Sense (SA0 tied low).
const LSM6DS3_ADDR: u8 = 0x6A;
const WHO_AM_I_REG: u8 = 0x0F;
const WHO_AM_I_EXPECTED: u8 = 0x69;

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
    loop {
        let command = IMU_CHANNEL.receive().await;
        let mut msg: String<64> = String::new();
        match command {
            ImuCommand::Status => match read_reg(&mut twim, WHO_AM_I_REG).await {
                Ok(value) if value == WHO_AM_I_EXPECTED => {
                    let _ = write!(msg, "IMU detected: WHO_AM_I=0x{:02X}\r\n", value);
                }
                Ok(value) => {
                    let _ = write!(
                        msg,
                        "IMU mismatch: WHO_AM_I=0x{:02X} (expected 0x{:02X})\r\n",
                        value, WHO_AM_I_EXPECTED
                    );
                }
                Err(_) => {
                    let _ = write!(msg, "IMU not responding on I2C.\r\n");
                }
            },
            ImuCommand::ReadReg { reg } => match read_reg(&mut twim, reg).await {
                Ok(value) => {
                    let _ = write!(msg, "IMU[0x{:02X}] = 0x{:02X}\r\n", reg, value);
                }
                Err(_) => {
                    let _ = write!(msg, "IMU read error at 0x{:02X}\r\n", reg);
                }
            },
            ImuCommand::WriteReg { reg, value } => match write_reg(&mut twim, reg, value).await {
                Ok(()) => {
                    let _ = write!(msg, "IMU[0x{:02X}] <= 0x{:02X}\r\n", reg, value);
                }
                Err(_) => {
                    let _ = write!(msg, "IMU write error at 0x{:02X}\r\n", reg);
                }
            },
        }
        uart_tx.send(UartTxMsg::from(msg)).await;
    }
}

async fn read_reg(twim: &mut Twim<'static>, reg: u8) -> Result<u8, twim::Error> {
    let mut buf = [0u8; 1];
    twim.write_read(LSM6DS3_ADDR, &[reg], &mut buf).await?;
    Ok(buf[0])
}

async fn write_reg(twim: &mut Twim<'static>, reg: u8, value: u8) -> Result<(), twim::Error> {
    twim.write(LSM6DS3_ADDR, &[reg, value]).await
}
