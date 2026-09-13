#![no_std]
#![no_main]

// use core::default::Default;
use core::str;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::bind_interrupts;
use embassy_nrf::gpio::{Level, Output};
use embassy_nrf::peripherals;
use embassy_nrf::uarte::{self, Baudrate, Config, Uarte};
use embassy_time::{Duration, Timer};

// 1. BIND INTERRUPTS
// The UART hardware generates interrupts when transmission finishes.
// We must link the specific peripheral (UARTE1) to the Embassy handler.
bind_interrupts!(struct Irqs {
    UARTE1 => uarte::InterruptHandler<peripherals::UARTE1>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());

    // // Dan's own Board Support Package from the Seeed Studio XIAO nRF52840 Sense Schematic v1.1
    // let _red_led = p.P0_26;
    // let _green_led = p.P0_30;
    // let _blue_led = p.P0_06;

    // // Initialize the LED (User LED on PA5)
    // let mut led = Output::new(_green_led, Level::Low, embassy_nrf::gpio::OutputDrive::Standard);

    //Initialize the USART2 peripheral with DMA for CLI listening
    let mut config = Config::default();
    config.baudrate = Baudrate::BAUD115200; // Redundant

    // We initialize the UARTE peripheral with RX/TX pins and interrupt binding.
    let mut uart = Uarte::new(
        p.UARTE1, p.P1_12, // RX pin (D7 on XIAO nRF52840 Sense)
        p.P1_11, // TX pin (D6 on XIAO nRF52840 Sense)
        Irqs,    // Interrupts for UARTE1
        config,
    );

    // // Send a welcome message
    let _ = uart.write(b"Embassy CLI Ready. Type 'on' or 'off'.\r\n").await.unwrap();

    // The line bugger to hold incoming characters
    let mut line_buffer = [0u8; 64];
    let mut cursor = 0;

    // led.toggle();
    // // let _ = uart.write(b"LED on.\r\n").await.unwrap();
    // let _ = Timer::after(Duration::from_millis(500)).await;

    // led.toggle();
    // let _ = uart.write(b"LED off.\r\n").await.unwrap();
    // let _ = Timer::after(Duration::from_millis(500)).await;

    // led.toggle();
    // let _ = uart.write(b"LED on.\r\n").await.unwrap();
    // let _ = Timer::after(Duration::from_millis(500)).await;

    // led.toggle();
    // let _ = uart.write(b"LED off.\r\n").await.unwrap();
    // let _ = Timer::after(Duration::from_millis(500)).await;

    // led.toggle();
    // let _ = uart.write(b"LED on.\r\n").await.unwrap();
    // let _ = Timer::after(Duration::from_millis(500)).await;

    // led.toggle();
    // let _ = uart.write(b"LED off.\r\n").await.unwrap();
    // let _ = Timer::after(Duration::from_millis(500)).await;

    // led.toggle();
    // let _ = uart.write(b"LED on.\r\n").await.unwrap();
    // let _ = Timer::after(Duration::from_millis(500)).await;

    // led.toggle();
    // let _ = uart.write(b"LED off.\r\n").await.unwrap();
    // let _ = Timer::after(Duration::from_millis(500)).await;

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
        if byte_buf[0] == b'\r' || byte_buf[0] == b'\n' {
            // Process the command
            let command = str::from_utf8(&line_buffer[..cursor]).unwrap_or("");
            match command {
                "" => {
                    // Ignore empty commands
                }
                "on" => {
                    // led.set_high();
                    let _ = uart.write(b"LED turned on.\r\n").await.unwrap();
                }
                "off" => {
                    // led.set_low();
                    let _ = uart.write(b"LED turned off.\r\n").await.unwrap();
                }
                "help" => {
                    let _ = uart
                        .write(b"Available commands: on, off, help\r\n")
                        .await
                        .unwrap();
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
}
