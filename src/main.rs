#![no_std]
#![no_main]

mod bsp;

use bsp::{Board, Color, Leds};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_nrf::bind_interrupts;
use embassy_nrf::peripherals;
use embassy_nrf::uarte::{self, Baudrate, Config, Uarte};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use heapless::String;
use panic_probe as _;

/// Outbound UART message type (supports string literals and dynamically formatted strings).
#[derive(Clone)]
pub enum UartTxMsg {
    Static(&'static str),
    Dyn(String<64>),
    Byte(u8),
}

impl UartTxMsg {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            UartTxMsg::Static(s) => s.as_bytes(),
            UartTxMsg::Dyn(s) => s.as_bytes(),
            UartTxMsg::Byte(b) => core::slice::from_ref(b),
        }
    }
}

impl From<&'static str> for UartTxMsg {
    fn from(s: &'static str) -> Self {
        UartTxMsg::Static(s)
    }
}

impl From<u8> for UartTxMsg {
    fn from(b: u8) -> Self {
        UartTxMsg::Byte(b)
    }
}

impl From<String<64>> for UartTxMsg {
    fn from(s: String<64>) -> Self {
        UartTxMsg::Dyn(s)
    }
}

// Many-to-one channel for LED color requests (queue capacity of 8)
static LED_CHANNEL: Channel<CriticalSectionRawMutex, Color, 8> = Channel::new();

// Many-to-one channel for outbound UART transmissions (queue capacity of 8)
static UART_TX_CHANNEL: Channel<CriticalSectionRawMutex, UartTxMsg, 8> = Channel::new();

// Channel for incoming UART received bytes (queue capacity of 64)
static UART_RX_CHANNEL: Channel<CriticalSectionRawMutex, u8, 64> = Channel::new();

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

/// Dedicated UART hardware management task: owns the UARTE peripheral,
/// continuously writing outbound messages from UART_TX_CHANNEL and
/// streaming incoming received bytes into UART_RX_CHANNEL.
#[embassy_executor::task]
async fn uart_task(uart: Uarte<'static>) {
    defmt::info!("UART task started.");
    let (mut tx, mut rx) = uart.split();

    let tx_loop = async {
        loop {
            let msg = UART_TX_CHANNEL.receive().await;
            let _ = tx.write(msg.as_bytes()).await;
        }
    };

    let rx_loop = async {
        let mut byte_buf = [0u8; 1];
        loop {
            if rx.read(&mut byte_buf).await.is_ok() {
                UART_RX_CHANNEL.send(byte_buf[0]).await;
            }
        }
    };

    join(tx_loop, rx_loop).await;
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_nrf::init(Default::default());
    let board = Board::init(p);

    // Spawn the stand-alone LED task, transferring ownership of board.leds
    spawner.must_spawn(led_task(board.leds));

    // Configure and create the UARTE driver
    let mut config = Config::default();
    config.baudrate = Baudrate::BAUD115200;

    let uart = Uarte::new(
        board.uarte1,
        board.pins.d7_rx,
        board.pins.d6_tx,
        Irqs,
        config,
    );

    // Spawn the stand-alone UART task, transferring ownership of the UARTE driver
    spawner.must_spawn(uart_task(uart));

    defmt::info!("Embassy initialized, starting blink sequence...");
    for cbits in 1..=8 {
        LED_CHANNEL.send(Color::from_bits(cbits)).await;
        let _ = Timer::after(Duration::from_millis(250)).await;
    }

    // Send a welcome message via the TX channel
    UART_TX_CHANNEL.send(UartTxMsg::from("Embassy CLI Ready. Type 'on', 'off', 'help', or 'panic'.\r\n")).await;
    // Binary of above:
    // "on" - 6f6e0a
    // "off" - 6f66660a
    // "help" - 68656c700a
    // "panic" - 70616e69630a

    // The line buffer to hold incoming characters
    let mut line_buffer = [0u8; 64];
    let mut cursor = 0;

    loop {
        // 1. Receive a single byte from the UART RX channel
        let byte = UART_RX_CHANNEL.receive().await;

        // 2. Echo the received character back via the UART TX channel
        UART_TX_CHANNEL.send(UartTxMsg::from(byte)).await;

        // 3. Process the whole command line if a newline is received
        if byte == b'\r' || byte == b'\n' {
            let command = str::from_utf8(&line_buffer[..cursor]).unwrap_or("");
            match command {
                "" => {
                    // Ignore empty commands from CRLF
                }
                "on" => {
                    LED_CHANNEL.send(Color::Cyan).await;
                    UART_TX_CHANNEL.send(UartTxMsg::from("LED turned on.\r\n")).await;
                    defmt::info!("Command received: ON");
                }
                "off" => {
                    LED_CHANNEL.send(Color::Off).await;
                    UART_TX_CHANNEL.send(UartTxMsg::from("LED turned on.\r\n")).await;
                    defmt::info!("Command received: OFF");
                }
                "help" => {
                    UART_TX_CHANNEL
                        .send(UartTxMsg::from("Available commands: on, off, help, panic\r\n")).await;
                    defmt::info!("Help command received via CLI!");
                }
                "panic" => {
                    UART_TX_CHANNEL.send(UartTxMsg::from("Triggering deliberate panic...\r\n")).await;
                    defmt::panic!("Deliberate test panic for panic-probe verification!");
                }
                _ => {
                    UART_TX_CHANNEL.send(UartTxMsg::from("Unknown command.\r\n")).await;
                    defmt::info!("Unknown command received via CLI!");
                }
            }
            // 4. Reset the cursor for the next command
            cursor = 0;
        } else {
            // Append the character to our buffer
            if cursor < line_buffer.len() {
                line_buffer[cursor] = byte;
                cursor += 1;
            } else {
                UART_TX_CHANNEL
                    .send(UartTxMsg::from("[Error] Input buffer is full.\r\n"))
                    .await;
                line_buffer.fill(0);
                cursor = 0;
            }
        }
    }
}
