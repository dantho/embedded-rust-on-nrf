#![no_std]
#![no_main]

mod bsp;
#[path = "main_leds.rs"]
mod main_leds;
#[path = "main_uart.rs"]
mod main_uart;

use bsp::Board;
use embassy_executor::Spawner;
use embassy_nrf::uarte::{Baudrate, Config, Uarte};
use embassy_time::{Duration, Timer};
use defmt_rtt as _;
use panic_probe as _;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let board = Board::init(p);
    let led_sender = main_leds::sender();

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
    main_uart::spawn(spawner, uart, led_sender);

    defmt::info!("Embassy initialized, starting blink sequence...");
    for cbits in 1..=8 {
        led_sender.send(bsp::Color::from_bits(cbits)).await;
        let _ = Timer::after(Duration::from_millis(250)).await;
    }

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
