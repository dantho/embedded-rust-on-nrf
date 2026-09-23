use embassy_nrf::twim::{self, Twim};
use embassy_time::{with_timeout, Duration};

// Onboard LSM6DS3TR-C IMU on the XIAO nRF52840 Sense (internal I2C1 bus, not on the header).
const IMU_ADDR_PRIMARY: u8 = 0x6A;
const IMU_ADDR_SECONDARY: u8 = 0x6B;
const WHO_AM_I_REG: u8 = 0x0F;
// Chip ID, coincidentally equal to IMU_ADDR_PRIMARY but otherwise unrelated to it.
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
pub enum ImuIoError {
    Bus(twim::Error),
    Timeout,
    /// No address is known yet; probing was never done or found nothing.
    NotDetected,
}

/// Outcome of a WHO_AM_I probe: the address that answered and the value it returned.
#[derive(Clone, Copy)]
pub struct ProbeResult {
    pub addr: u8,
    pub who_am_i: u8,
    pub expected: u8,
}

impl ProbeResult {
    pub fn matches_expected(&self) -> bool {
        self.who_am_i == self.expected
    }
}

/// Async driver for the onboard LSM6DS3TR-C accelerometer/gyroscope.
pub struct Lsm6ds3 {
    twim: Twim<'static>,
    /// Remembered from the last successful probe; defaults to the primary address until known.
    addr: Option<u8>,
}

impl Lsm6ds3 {
    pub fn new(twim: Twim<'static>) -> Self {
        Self { twim, addr: None }
    }

    /// Probe the primary address, then the secondary, remembering whichever answered.
    pub async fn probe(&mut self) -> Option<ProbeResult> {
        for addr in [IMU_ADDR_PRIMARY, IMU_ADDR_SECONDARY] {
            if let Ok(who_am_i) = self.read_reg_at(addr, WHO_AM_I_REG).await {
                self.addr = Some(addr);
                return Some(ProbeResult {
                    addr,
                    who_am_i,
                    expected: WHO_AM_I_EXPECTED,
                });
            }
        }
        self.addr = None;
        None
    }

    /// Probe and apply the boot-time ODR/range config in one step.
    pub async fn init(&mut self) -> Option<(ProbeResult, Result<(), ImuIoError>)> {
        let probe = self.probe().await?;
        let config_result = self.configure().await;
        Some((probe, config_result))
    }

    /// Set accelerometer/gyro output data rate and full-scale range.
    pub async fn configure(&mut self) -> Result<(), ImuIoError> {
        let addr = self.addr.ok_or(ImuIoError::NotDetected)?;
        self.write_reg_at(addr, CTRL1_XL_REG, ODR_104HZ_2G_250DPS).await?;
        self.write_reg_at(addr, CTRL2_G_REG, ODR_104HZ_2G_250DPS).await?;
        Ok(())
    }

    pub async fn read_reg(&mut self, reg: u8) -> Result<u8, ImuIoError> {
        let addr = self.addr.ok_or(ImuIoError::NotDetected)?;
        self.read_reg_at(addr, reg).await
    }

    pub async fn write_reg(&mut self, reg: u8, value: u8) -> Result<(), ImuIoError> {
        let addr = self.addr.ok_or(ImuIoError::NotDetected)?;
        self.write_reg_at(addr, reg, value).await
    }

    /// Read the gyro+accel burst and convert to dps/g using the configured full-scale range.
    /// Returns `[gx, gy, gz, ax, ay, az]`.
    pub async fn read_sample(&mut self) -> Result<[f32; 6], ImuIoError> {
        let addr = self.addr.ok_or(ImuIoError::NotDetected)?;
        let mut buf = [0u8; 12];
        // A bare `&[OUT_START_REG]` is a const array literal the compiler can place in flash,
        // which DMA can't read; bind it to a local so it's guaranteed to live in RAM.
        let start_reg = [OUT_START_REG];
        with_timeout(I2C_TIMEOUT, self.twim.write_read(addr, &start_reg, &mut buf))
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

    async fn read_reg_at(&mut self, addr: u8, reg: u8) -> Result<u8, ImuIoError> {
        let mut buf = [0u8; 1];
        with_timeout(I2C_TIMEOUT, self.twim.write_read(addr, &[reg], &mut buf))
            .await
            .map_err(|_| ImuIoError::Timeout)?
            .map_err(ImuIoError::Bus)?;
        Ok(buf[0])
    }

    async fn write_reg_at(&mut self, addr: u8, reg: u8, value: u8) -> Result<(), ImuIoError> {
        with_timeout(I2C_TIMEOUT, self.twim.write(addr, &[reg, value]))
            .await
            .map_err(|_| ImuIoError::Timeout)?
            .map_err(ImuIoError::Bus)
    }
}
