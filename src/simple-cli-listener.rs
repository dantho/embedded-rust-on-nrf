#![no_std]
#![no_main]

use panic_halt as _;
use defmt_rtt as _;

use core::str;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embassy_nrf::peripherals;
use embassy_nrf::uarte::{self, Config, Uarte};
use embassy_nrf::bind_interrupts;
use embassy_nrf::gpio::{Level, Output, Speed};
use embassy_nrf::{bind_interrupt, usart};

// Bind interrupt for USART
bind_interrupt!(struct Irqs {
    USART2 => usart::interruptHandler<peripherals::USART2>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());

    // Initialize the LED (User LED on PA5)
    let mut led = Output::new(p.PA5, Level::Low, Speed::Low);

    //Initialize the USART2 peripheral with DMA for CLI listening
    let mut config = Config::default();
    config.baudrate = 115200;
    // Book text has this a mut
    let uart = Uart::new(p.USART2, Irqs, p.DMA1_CH6, p.DMA1_CH7, config);

    // Send a welcome message
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

        // 3. Process the char (and maybe the whole command line if a newline is received)
        if byte_buf[0] == b'\r' || byte_buf[0] == b'\n' {
        // 3b. End of line received, process the command in the else block
            // 3. Process the command
            let command = str::from_utf8(&line_buffer[..cursor]).unwrap_or("");
            match command {
                "" => {
                    // Ignore empty commands
                }
                "on" => {
                    led.set_high();
                    let _ = uart.write(b"LED turned on.\r\n").await.unwrap();
                }
                "off" => {
                    led.set_low();
                    let _ = uart.write(b"LED turned off.\r\n").await.unwrap();
                }
                "help" => {
                    let _ = uart.write(b"Available commands: on, off, help\r\n").await.unwrap();
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
                let _ = uart.write(b"[Error] Input buffer is full.\r\n").await.unwrap();
                line_buffer.fill(0);
                cursor = 0; // Reset the cursor to start fresh
            }
        }
    }
}