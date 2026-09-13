#![no_std]
#![no_main]

mod bsp;

use bsp::{Board, Color, Leds};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::bind_interrupts;
use embassy_nrf::peripherals;
use embassy_nrf::uarte::{self, Baudrate, Config, Uarte};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_time::{Duration, Timer};
use panic_probe as _;

// Many-to-one channel for LED color requests (queue capacity of 8)
static LED_CHANNEL: Channel<CriticalSectionRawMutex, Color, 8> = Channel::new();

// 1. BIND INTERRUPTS
// The UART hardware generates interrupts when transmission finishes.
// We must link the specific peripheral (UARTE1) to the Embassy handler.
bind_interrupts!(struct Irqs {
    UARTE1 => uarte::InterruptHandler<peripherals::UARTE1>;
});

/// Dedicated LED management task: owns the Leds peripheral and executes
/// color changes sequentially as they arrive on the channel (many-to-one).
#[embassy_executor::task]
async fn led_task(mut leds: Leds) {
    defmt::info!("LED task started.");
    loop {
        let color = LED_CHANNEL.receive().await;
        defmt::info!("LED task applying color: {}", color);
        leds.set_color(color);
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let board = Board::init(p);

    // Spawn the stand-alone LED task, transferring ownership of board.leds
    spawner.must_spawn(led_task(board.leds));

    // Get a sender handle to the many-to-one channel
    let led_sender: Sender<'_, CriticalSectionRawMutex, Color, 8> = LED_CHANNEL.sender();

    defmt::info!("Embassy initialized, starting blink sequence...");
    for cbits in 1..=8 {
        led_sender.send(Color::from_bits(cbits)).await;
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

    // Send a welcome message
    let _ = uart.write(b"Embassy CLI Ready. Type 'on' or 'off'.\r\n").await.unwrap();

    // The line buffer to hold incoming characters
    let mut line_buffer = [0u8; 64];
    let mut cursor = 0;

    loop {
        // 1. Read a single byte
        let mut byte_buf = [0u8; 1];
        // This call yields until a character arrives.
        uart.read(&mut byte_buf).await.unwrap();

        // 2. Echo the received character back to the UART
        let _ = uart.write(&byte_buf).await.unwrap();

        // 3. Process the whole command line if a newline is received
        if byte_buf[0] == b'\r' || byte_buf[0] == b'\n' {
            let command = str::from_utf8(&line_buffer[..cursor]).unwrap_or("");
            match command {
                "" => {
                    // Ignore empty commands from CRLF
                }
                "on" => {
                    led_sender.send(Color::Cyan).await;
                    defmt::info!("Command received: ON");
                    let _ = uart.write(b"LED turned on.\r\n").await.unwrap();
                }
                "off" => {
                    led_sender.send(Color::Off).await;
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
                let _ = uart
                    .write(b"[Error] Input buffer is full.\r\n")
                    .await
                    .unwrap();
                line_buffer.fill(0);
                cursor = 0;
            }
        }
    }
}
