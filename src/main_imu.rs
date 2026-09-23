use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_nrf::twim::Twim;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use heapless::String;

use crate::lsm6ds3::{ImuIoError, Lsm6ds3};
use crate::main_uart::{UartTxMsg, UartTxSender};

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
async fn imu_task(twim: Twim<'static>, uart_tx: UartTxSender) {
    defmt::info!("IMU task started.");
    let mut imu = Lsm6ds3::new(twim);
    uart_tx.send(UartTxMsg::from(init_report(&mut imu).await)).await;

    loop {
        let command = IMU_CHANNEL.receive().await;
        let mut msg: String<64> = String::new();
        match command {
            ImuCommand::Status => match imu.probe().await {
                Some(result) if result.matches_expected() => {
                    let _ = write!(
                        msg,
                        "IMU detected at 0x{:02X}: WHO_AM_I=0x{:02X}\n",
                        result.addr, result.who_am_i
                    );
                }
                Some(result) => {
                    let _ = write!(
                        msg,
                        "IMU mismatch at 0x{:02X}: WHO_AM_I=0x{:02X} (expected 0x{:02X})\n",
                        result.addr, result.who_am_i, result.expected
                    );
                }
                None => {
                    let _ = write!(msg, "IMU not found at 0x6A or 0x6B.\n");
                }
            },
            ImuCommand::ReadReg { reg } => match imu.read_reg(reg).await {
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
            },
            ImuCommand::WriteReg { reg, value } => match imu.write_reg(reg, value).await {
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
            },
            ImuCommand::Sample => match imu.read_sample().await {
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
            },
        }
        uart_tx.send(UartTxMsg::from(msg)).await;
    }
}

/// Probe and apply the boot-time ODR/range config once, reporting the outcome over UART.
async fn init_report(imu: &mut Lsm6ds3) -> String<64> {
    let mut msg: String<64> = String::new();
    match imu.init().await {
        Some((probe, Ok(()))) => {
            let _ = write!(
                msg,
                "IMU init: 0x{:02X} WHO_AM_I=0x{:02X} configured OK\n",
                probe.addr, probe.who_am_i
            );
        }
        Some((probe, Err(_))) => {
            let _ = write!(
                msg,
                "IMU init: 0x{:02X} WHO_AM_I=0x{:02X} config FAILED\n",
                probe.addr, probe.who_am_i
            );
        }
        None => {
            let _ = write!(msg, "IMU init: not found at 0x6A or 0x6B\n");
        }
    }
    msg
}

