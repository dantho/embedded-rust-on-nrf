use core::str;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_nrf::bind_interrupts;
use embassy_nrf::peripherals;
use embassy_nrf::uarte::{self, Uarte};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use heapless::String;

use crate::bsp::Color;
use crate::main_leds::ColorSender;

bind_interrupts!(pub struct Irqs {
    UARTE1 => uarte::InterruptHandler<peripherals::UARTE1>;
});

#[derive(Clone)]
pub enum UartTxMsg {
    Static(&'static str),
    Dyn(String<64>),
    Byte(u8),
}

impl UartTxMsg {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Static(message) => message.as_bytes(),
            Self::Dyn(message) => message.as_bytes(),
            Self::Byte(byte) => core::slice::from_ref(byte),
        }
    }
}

impl From<&'static str> for UartTxMsg {
    fn from(message: &'static str) -> Self {
        Self::Static(message)
    }
}

impl From<u8> for UartTxMsg {
    fn from(byte: u8) -> Self {
        Self::Byte(byte)
    }
}

impl From<String<64>> for UartTxMsg {
    fn from(message: String<64>) -> Self {
        Self::Dyn(message)
    }
}

static UART_TX_CHANNEL: Channel<CriticalSectionRawMutex, UartTxMsg, 8> = Channel::new();
static UART_RX_CHANNEL: Channel<CriticalSectionRawMutex, u8, 64> = Channel::new();

pub fn spawn(spawner: Spawner, uart: Uarte<'static>, led_sender: ColorSender) {
    spawner.must_spawn(uart_task(uart));
    spawner.must_spawn(cli_task(led_sender));
}

#[embassy_executor::task]
async fn uart_task(uart: Uarte<'static>) {
    defmt::info!("UART transport task started.");
    let (mut tx, mut rx) = uart.split();

    let tx_loop = async {
        loop {
            let message = UART_TX_CHANNEL.receive().await;
            let _ = tx.write(message.as_bytes()).await;
        }
    };

    let rx_loop = async {
        let mut byte_buffer = [0u8; 1];
        loop {
            if rx.read(&mut byte_buffer).await.is_ok() {
                UART_RX_CHANNEL.send(byte_buffer[0]).await;
            }
        }
    };

    join(tx_loop, rx_loop).await;
}

#[embassy_executor::task]
async fn cli_task(led_sender: ColorSender) {
    defmt::info!("UART CLI task started.");
    UART_TX_CHANNEL
        .send(UartTxMsg::from(
            "Embassy CLI Ready. Type 'on', 'off', 'help', or 'panic'.\r\n",
        ))
        .await;

    let mut line_buffer = [0u8; 64];
    let mut cursor = 0;

    loop {
        let byte = UART_RX_CHANNEL.receive().await;
        UART_TX_CHANNEL.send(UartTxMsg::from(byte)).await;

        if byte == b'\r' || byte == b'\n' {
            let command = str::from_utf8(&line_buffer[..cursor]).unwrap_or("");
            match command {
                "" => {}
                "on" => {
                    led_sender.send(Color::Cyan).await;
                    UART_TX_CHANNEL
                        .send(UartTxMsg::from("LED turned on.\r\n"))
                        .await;
                    defmt::info!("Command received: ON");
                }
                "off" => {
                    led_sender.send(Color::Off).await;
                    UART_TX_CHANNEL
                        .send(UartTxMsg::from("LED turned off.\r\n"))
                        .await;
                    defmt::info!("Command received: OFF");
                }
                "help" => {
                    UART_TX_CHANNEL
                        .send(UartTxMsg::from(
                            "Available commands: on, off, help, panic\r\n",
                        ))
                        .await;
                    defmt::info!("Help command received via CLI!");
                }
                "panic" => {
                    UART_TX_CHANNEL
                        .send(UartTxMsg::from("Triggering deliberate panic...\r\n"))
                        .await;
                    defmt::panic!("Deliberate test panic for panic-probe verification!");
                }
                _ => {
                    UART_TX_CHANNEL
                        .send(UartTxMsg::from("Unknown command.\r\n"))
                        .await;
                    defmt::info!("Unknown command received via CLI!");
                }
            }
            cursor = 0;
        } else if cursor < line_buffer.len() {
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
