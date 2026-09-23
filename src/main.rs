#![no_std]
#![no_main]

mod bsp;
mod lsm6ds3;
mod main_leds;
mod main_uart;
mod main_imu;

use bsp::Board;
use embassy_executor::Spawner;
use embassy_nrf::twim::{self, Twim};
use embassy_nrf::uarte::{Baudrate, Config, Uarte};
use embassy_time::{Duration, Timer};
use defmt_rtt as _;
use panic_probe as _;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let board = Board::init(p);
    // Keep the IMU/mic power rail enabled for the program's lifetime.
    let _imu_power = board.imu_power;
    let led_sender = main_leds::sender();
    let imu_sender = main_imu::sender();

    main_leds::spawn(spawner, board.leds);

    let mut config = Config::default();
    config.baudrate = Baudrate::BAUD115200;
    let uart = Uarte::new(
        board.uarte1,
        board.pins.d7_rx,
        board.pins.d6_tx,
        main_uart::Irqs,
        config,
    );
    main_uart::spawn(spawner, uart, led_sender, imu_sender);

    // board.imu_power was already driven high in Board::init(); let the rail settle.
    Timer::after(Duration::from_millis(10)).await;
    let twim = Twim::new(
        board.twispi1,
        main_imu::Irqs,
        board.imu_sda,
        board.imu_scl,
        twim::Config::default(),
        &mut [],
    );
    main_imu::spawn(spawner, twim, main_uart::tx_sender());

    defmt::info!("Embassy initialized, starting blink sequence...");
    for cbits in 1..=8 {
        led_sender.send(bsp::Color::from_bits(cbits)).await;
        let _ = Timer::after(Duration::from_millis(250)).await;
    }

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
