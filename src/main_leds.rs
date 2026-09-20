use crate::bsp::{Color, Leds};
use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};

static LED_CHANNEL: Channel<CriticalSectionRawMutex, Color, 8> = Channel::new();

pub type ColorSender = Sender<'static, CriticalSectionRawMutex, Color, 8>;

pub fn sender() -> ColorSender {
    LED_CHANNEL.sender()
}

pub fn spawn(spawner: Spawner, leds: Leds) {
    spawner.must_spawn(led_task(leds));
}

#[embassy_executor::task]
async fn led_task(mut leds: Leds) {
    defmt::info!("LED task started.");
    loop {
        let color = LED_CHANNEL.receive().await;
        defmt::info!("LED task applying color: {}", color);
        leds.set_color(color);
    }
}
