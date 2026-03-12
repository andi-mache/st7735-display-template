//! # ESP32 ST7735s Display Template
//!
//! Bare-metal `no_std` starting point for any ESP32 + 128×128 ST7735s TFT project.
//! Initialises the display, clears it, then hands control to `draw()` in a loop.
//!
//! ## Hardware wiring
//! | Signal | ESP32 GPIO |
//! |--------|------------|
//! | DC     | GPIO2      |
//! | RST    | GPIO4      |
//! | CS     | GPIO5      |
//! | SCK    | GPIO18     |
//! | MOSI   | GPIO23     |
//!
//! ## Building
//! ```sh
//! cargo run --release
//! ```

#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use embedded_graphics::{pixelcolor::Rgb565, prelude::*};

use mipidsi::{Builder, interface::SpiInterface, models::ST7735s, options::ColorOrder};

use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::{
        Mode,
        master::{Config, Spi},
    },
    time::Rate,
};

use esp_println::{self as _, println};

// ── Panic handler ─────────────────────────────────────────────────────────

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

// ── Entry point ───────────────────────────────────────────────────────────

#[main]
fn main() -> ! {
    // Initialise ESP32 at max clock speed.
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));

    let mut delay = Delay::new();

    // GPIO pins for the display.
    let dc = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    let rst = Output::new(peripherals.GPIO4, Level::High, OutputConfig::default());
    let cs = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());

    // SPI2 at 40 MHz, Mode 0. SCK = GPIO18, MOSI = GPIO23.
    let spi = Spi::new(
        peripherals.SPI2,
        Config::default()
            .with_frequency(Rate::from_mhz(40))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO18)
    .with_mosi(peripherals.GPIO23);

    let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new(spi, cs, delay).unwrap();

    let mut buffer = [0u8; 512];
    let di = SpiInterface::new(spi_device, dc, &mut buffer);

    // Initialise the ST7735s display controller.
    let mut display = Builder::new(ST7735s, di)
        .reset_pin(rst)
        .display_size(128, 128)
        .color_order(ColorOrder::Bgr)
        .init(&mut delay)
        .unwrap();

    // Clear to black.
    display.clear(Rgb565::BLACK).unwrap();

    println!("Display ready");

    // ── Main loop ─────────────────────────────────────────────────────
    loop {
        draw(&mut display, &mut delay);
    }
}

// ── Your drawing code goes here ───────────────────────────────────────────

/// Called repeatedly from the main loop.
///
/// Add your `embedded-graphics` drawing calls here.
/// `display` accepts any `DrawTarget<Color = Rgb565>` primitive.
///
/// Useful imports to add at the top as needed:
/// ```
/// use embedded_graphics::{
///     mono_font::{ascii::FONT_6X10, MonoTextStyle},
///     primitives::{Rectangle, PrimitiveStyle},
///     text::{Text, TextStyleBuilder, Alignment},
///     prelude::*,
/// };
/// ```
fn draw<DI, MODEL>(_display: &mut mipidsi::Display<DI, MODEL, Output<'_>>, delay: &mut Delay)
where
    DI: mipidsi::interface::Interface,
    MODEL: mipidsi::models::Model<ColorFormat = Rgb565>,
    Rgb565: mipidsi::interface::InterfacePixelFormat<DI::Word>,
{
    // TODO: draw here

    delay.delay_millis(16); // ~60 fps cap
}
