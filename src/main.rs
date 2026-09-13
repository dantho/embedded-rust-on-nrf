#![no_std]
#![no_main]

mod bsp;

use bsp::{Board, Color};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::bind_interrupts;
use embassy_nrf::peripherals;
use embassy_nrf::uarte::{self, Baudrate, Config, Uarte};
use embassy_time::{Duration, Timer};
use panic_probe as _;

// 1. BIND INTERRUPTS
// The UART hardware generates interrupts when transmission finishes.
// We must link the specific peripheral (UARTE1) to the Embassy handler.
bind_interrupts!(struct Irqs {
    UARTE1 => uarte::InterruptHandler<peripherals::UARTE1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let mut board = Board::init(p);

    defmt::info!("Embassy initialized, starting blink sequence...");
    for cbits in 1..=8 {
        board.leds.set_color(Color::from_bits(cbits));
        let _ = Timer::after(Duration::from_millis(250)).await;
    }

    // # UART Initialization
    defmt::info!("Starting UART initialization...");

    let mut config = Config::default();
    config.baudrate = Baudrate::BAUD115200;

    // Initialize UARTE using pins from the Board struct
    let mut uart = Uarte::new(
        board.uarte1,
        board.pins.d7_rx,
        board.pins.d6_tx,
        Irqs,
        config,
    );

    // // Send a welcome message
    let _ = uart.write(b"Embassy CLI Ready. Type 'on' or 'off'.\r\n").await.unwrap();

    // The line bugger to hold incoming characters
    let mut line_buffer = [0u8; 64];
    let mut cursor = 0;

    loop {
        // 1. Read a single byte
        // We use a small buffer for the immediate read -- one byte
        let mut byte_buf = [0u8; 1];
        // This call yields until a character arrives.
        // The system is free to do other work while waiting.
        uart.read(&mut byte_buf).await.unwrap();

        // 2. Echo the received character back to the UART
        let _ = uart.write(&byte_buf).await.unwrap();

        // 3. Process the whole command line if a newline is received, otherwise accumulate characters in the buffer
        // "on\n" is 6f6e0a in hex
        // "off\n" is 6f66660a in hex
        if byte_buf[0] == b'\r' || byte_buf[0] == b'\n' {
            // Process the command
            let command = str::from_utf8(&line_buffer[..cursor]).unwrap_or("");
            match command {
                "" => {
                    // Ignore empty commands, which will occur due to naive processing of windows newlines (\r\n)
                }
                "on" => {
                    board.leds.set_color(Color::Cyan);
                    defmt::info!("Command received: ON");
                    let _ = uart.write(b"LED turned on.\r\n").await.unwrap();
                }
                "off" => {
                    board.leds.set_color(Color::Magenta);
                    defmt::info!("Command received: OFF");
                    let _ = uart.write(b"LED turned off.\r\n").await.unwrap();
                }
                "help" => {
                    let _ = uart
                        .write(b"Available commands: on, off, help, panic\r\n")
                        .await
                        .unwrap();
                }
                "panic" => {
                    let _ = uart.write(b"Triggering deliberate panic...\r\n").await.unwrap();
                    defmt::error!("Deliberate panic triggered via CLI command!");
                    defmt::panic!("Deliberate test panic for panic-probe verification!");
                }
                _ => {
                    let _ = uart.write(b"Unknown command.\r\n").await.unwrap();
                }
            }
            // 4. Reset the cursor for the next command
            cursor = 0;
        } else {
            // Append the character to our buffer
            if cursor < line_buffer.len() {
                line_buffer[cursor] = byte_buf[0];
                cursor += 1;
            } else {
                // Buffer is full. In a real application, you might want to handle this more gracefully,
                // We will send an error message back to the user and then clear the buffer and discard the input.
                let _ = uart
                    .write(b"[Error] Input buffer is full.\r\n")
                    .await
                    .unwrap();
                line_buffer.fill(0);
                cursor = 0; // Reset the cursor to start fresh
            }
        }
    }
    // defmt::info!("Exiting main loop.");
}
