//! # ESP32 TFT Dashboard (Display Only)
//!
//! A bare-metal `no_std` Rust application for the ESP32 that renders a
//! live dashboard on a 128×128 ST7735s TFT display via SPI.
//!
//! ## Display layout (128×128 px)
//! ```
//! ┌────────────────────────┐  y=0
//! │  ESP32  DASHBOARD      │  header bar (navy)
//! ├════════════════════════┤  y=16  cyan accent rule
//! │ STATUS        READY    │  y=18
//! │ UP          00:04:32   │  y=32  uptime since boot (HH:MM:SS)
//! ├────────────────────────┤  y=44  section divider
//! │ DISPLAY                │  y=46  section label
//! │ WIDTH             128  │  y=58
//! │ HEIGHT            128  │  y=70
//! ├────────────────────────┤  y=82  section divider
//! │ LAST UPDATE            │  y=84  section label
//! │       tick N           │  y=96  loop counter (large, cyan)
//! └────────────────────────┘  y=128
//! ```
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

// ── Imports ───────────────────────────────────────────────────────────────

use embedded_graphics::{
    mono_font::{
        MonoTextStyle,
        ascii::{FONT_6X10, FONT_10X20},
    },
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Arc, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Text, TextStyleBuilder},
};

use mipidsi::{
    Builder,
    interface::SpiInterface,
    models::ST7735s,
    options::ColorOrder,
};

use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
    spi::{
        Mode,
        master::{Config, Spi},
    },
    time::Rate,
};

// core::fmt::Write provides write!() for heapless::String formatting.
use core::fmt::Write;

use esp_hal::clock::CpuClock;
use esp_hal::peripherals::Peripherals;
use esp_hal::time::{Duration, Instant};
use esp_println::{self as _, println};

// ── Panic handler ─────────────────────────────────────────────────────────

/// Minimal panic handler. Spins forever on unrecoverable errors.
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// ── Colour palette (Rgb565) ───────────────────────────────────────────────
//
// Rgb565::new(r, g, b):  r: 0–31 (5-bit), g: 0–63 (6-bit), b: 0–31 (5-bit)

/// Background colour — pure black.
const BG: Rgb565 = Rgb565::BLACK;

/// Header bar background — very dark navy blue.
const HEADER_BG: Rgb565 = Rgb565::new(29, 0, 7);

/// Primary accent — bright cyan.
const ACCENT_CYAN: Rgb565 = Rgb565::new(0, 63, 31);

/// Success accent — lime green.
const ACCENT_LIME: Rgb565 = Rgb565::new(0, 59, 12);

/// Primary text — white.
const TEXT_WHITE: Rgb565 = Rgb565::WHITE;

/// Dimmed text — dark grey-green.
const TEXT_DIM: Rgb565 = Rgb565::new(12, 22, 12);

/// Section divider line colour.
const DIVIDER: Rgb565 = Rgb565::new(4, 10, 6);

// ── Dashboard state ───────────────────────────────────────────────────────

/// Runtime values needed to render one frame of the dashboard.
struct DashState {
    /// Elapsed time since boot: (hours, minutes, seconds).
    uptime_hms: (u8, u8, u8),
    /// Main-loop iteration counter — shown large in the bottom panel.
    tick: u32,
}

// Generates the app descriptor block expected by the ESP-IDF second-stage
// bootloader. Without this the bootloader may refuse to run the image.
esp_bootloader_esp_idf::esp_app_desc!();

// ── Entry point ───────────────────────────────────────────────────────────

/// Application entry point.
///
/// Initialises hardware, plays the boot animation, then enters the main
/// display loop — updating the dashboard once per second, forever.
#[main]
fn main() -> ! {
    let peripherals = init_hardware();

    let mut delay = Delay::new();

    // ── GPIO setup ────────────────────────────────────────────────────
    // DC (Data/Command): LOW = command byte, HIGH = pixel data.
    // RST (Reset): active-low; initialised HIGH (not in reset).
    // CS  (Chip Select): active-low; initialised HIGH (deselected).
    let dc  = Output::new(peripherals.GPIO2, Level::Low,  OutputConfig::default());
    let rst = Output::new(peripherals.GPIO4, Level::High, OutputConfig::default());
    let cs  = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());

    // ── SPI bus ───────────────────────────────────────────────────────
    // SPI2 (HSPI) in Mode 0 at 40 MHz. SCK on GPIO18, MOSI on GPIO23.
    // No MISO — the display is write-only.
    let spi = Spi::new(
        peripherals.SPI2,
        Config::default()
            .with_frequency(Rate::from_mhz(40))
            .with_mode(Mode::_0),
    )
    .unwrap()
    .with_sck(peripherals.GPIO18)
    .with_mosi(peripherals.GPIO23);

    // ExclusiveDevice wraps SPI bus + CS pin into a single SpiDevice that
    // automatically asserts/deasserts CS around each transaction.
    let spi_device = embedded_hal_bus::spi::ExclusiveDevice::new(spi, cs, delay).unwrap();

    let mut buffer = [0u8; 512];
    let di = SpiInterface::new(spi_device, dc, &mut buffer);

    // ── Display initialisation ────────────────────────────────────────
    let mut display = Builder::new(ST7735s, di)
        .reset_pin(rst)
        .display_size(128, 128)
        .color_order(ColorOrder::Bgr)
        .init(&mut delay)
        .unwrap();

    display.clear(BG).unwrap();

    // ── Boot animation → initial dashboard ───────────────────────────
    boot_animation(&mut display, &mut delay);
    draw_static_ui(&mut display);
    draw_dynamic_ui(&mut display, &DashState { uptime_hms: (0, 0, 0), tick: 0 });

    // Record boot time for uptime calculation.
    let start = Instant::now();
    let mut tick: u32 = 0;
    let mut spin_frame: u8 = 0;

    // ── Main loop ─────────────────────────────────────────────────────
    loop {
        tick = tick.wrapping_add(1);

        let elapsed = (Instant::now() - start).as_secs();
        let h = (elapsed / 3600) as u8;
        let m = ((elapsed % 3600) / 60) as u8;
        let s = (elapsed % 60) as u8;

        draw_dynamic_ui(&mut display, &DashState {
            uptime_hms: (h, m, s),
            tick,
        });

        println!("tick={} uptime={:02}:{:02}:{:02}", tick, h, m, s);

        // Animate the spinner while waiting ~1 second before the next update.
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            draw_spinner(&mut display, spin_frame);
            spin_frame = spin_frame.wrapping_add(1) % 8;
            delay.delay_millis(250);
        }
    }
}

// ── Spinner animation ─────────────────────────────────────────────────────

/// Draws one frame of a rotating arc spinner at the bottom-right corner.
///
/// The arc is 150° wide and advances 45° per frame, completing one full
/// revolution every 8 frames (~2 seconds at 250 ms/frame).
fn draw_spinner<DI, MODEL>(
    display: &mut mipidsi::Display<DI, MODEL, Output<'_>>,
    frame: u8,
)
where
    DI: mipidsi::interface::Interface,
    MODEL: mipidsi::models::Model<ColorFormat = Rgb565>,
    Rgb565: mipidsi::interface::InterfacePixelFormat<DI::Word>,
{
    const DIAMETER: u32 = 13;
    const TOP_LEFT: Point = Point::new(115, 108);

    // Erase previous frame before drawing new position.
    Rectangle::new(TOP_LEFT, Size::new(DIAMETER + 1, DIAMETER + 1))
        .into_styled(PrimitiveStyle::with_fill(BG))
        .draw(display).unwrap();

    let start_deg = (frame as f32) * 45.0;

    Arc::new(TOP_LEFT, DIAMETER, start_deg.deg(), 150.0_f32.deg())
        .into_styled(
            PrimitiveStyleBuilder::new()
                .stroke_color(ACCENT_CYAN)
                .stroke_width(2)
                .build(),
        )
        .draw(display).unwrap();
}

// ── Boot animation ────────────────────────────────────────────────────────

/// Hacker-style boot animation displayed once at startup before the dashboard.
///
/// Total duration: approximately 3.5 seconds. Sequence:
///
/// 1. **Flash** — screen briefly goes white then cuts to black.
/// 2. **Scanline sweep** — a dim green band races from top to bottom.
/// 3. **Typewriter** — `"INITIALIZING..."` types out with a cursor block.
/// 4. **Glitch title** — project name starts as random noise and resolves
///    left-to-right using a deterministic LCG (no hardware RNG needed).
/// 5. **Progress bar** — cyan-outlined bar fills with lime green over 20 steps.
/// 6. **Wipe** — 16 black rows sweep top-to-bottom, clearing the screen.
fn boot_animation<DI, MODEL>(
    display: &mut mipidsi::Display<DI, MODEL, Output<'_>>,
    delay: &mut Delay,
)
where
    DI: mipidsi::interface::Interface,
    MODEL: mipidsi::models::Model<ColorFormat = Rgb565>,
    Rgb565: mipidsi::interface::InterfacePixelFormat<DI::Word>,
{
    let centered = TextStyleBuilder::new().alignment(Alignment::Center).build();
    let left     = TextStyleBuilder::new().alignment(Alignment::Left).build();

    // ── 1. Flash ─────────────────────────────────────────────────────
    display.clear(Rgb565::BLACK).unwrap();
    delay.delay_millis(60);
    display.clear(BG).unwrap();
    delay.delay_millis(40);

    // ── 2. Scanline sweep ─────────────────────────────────────────────
    let scan_color = Rgb565::new(4, 40, 20);
    for step in 0u16..=16 {
        if step > 0 {
            let prev_y = ((step - 1) * 8) as i32;
            Rectangle::new(Point::new(0, prev_y), Size::new(128, 8))
                .into_styled(PrimitiveStyle::with_fill(BG))
                .draw(display).unwrap();
        }
        let y = (step * 8) as i32;
        if y < 128 {
            Rectangle::new(Point::new(0, y), Size::new(128, 3))
                .into_styled(PrimitiveStyle::with_fill(scan_color))
                .draw(display).unwrap();
        }
        delay.delay_millis(18);
    }
    display.clear(BG).unwrap();
    delay.delay_millis(120);

    // ── 3. Typewriter ─────────────────────────────────────────────────
    let init_msg = b"INITIALIZING...";
    let mut typed: heapless::String<20> = heapless::String::new();

    for &ch in init_msg {
        Rectangle::new(Point::new(0, 54), Size::new(128, 12))
            .into_styled(PrimitiveStyle::with_fill(BG))
            .draw(display).unwrap();

        typed.push(ch as char).ok();

        Text::with_text_style(
            typed.as_str(),
            Point::new(4, 64),
            MonoTextStyle::new(&FONT_6X10, ACCENT_LIME),
            left,
        ).draw(display).unwrap();

        let cursor_x = 4 + (typed.len() as i32) * 6;
        Rectangle::new(Point::new(cursor_x, 55), Size::new(5, 9))
            .into_styled(PrimitiveStyle::with_fill(ACCENT_LIME))
            .draw(display).unwrap();

        delay.delay_millis(45);
    }
    delay.delay_millis(200);

    // ── 4. Glitch title ───────────────────────────────────────────────
    // LCG seed 0xDEAD_BEEF: deterministic, so no hardware RNG peripheral needed.
    let target       = "TFT Dashboard";
    let target_bytes = target.as_bytes();
    let len          = target_bytes.len();
    let mut lcg: u32 = 0xDEAD_BEEF;

    for frame in 0u8..12 {
        Rectangle::new(Point::new(0, 36), Size::new(128, 22))
            .into_styled(PrimitiveStyle::with_fill(BG))
            .draw(display).unwrap();

        let mut glitch: heapless::String<20> = heapless::String::new();
        let resolved_up_to = (frame as usize * len) / 11;

        for i in 0..len {
            if i < resolved_up_to {
                glitch.push(target_bytes[i] as char).ok();
            } else {
                // LCG: Numerical Recipes parameters (mod 2^32)
                lcg = lcg.wrapping_mul(1664525).wrapping_add(1013904223);
                let c = ((lcg >> 16) & 0x3E) as u8 + 0x21;
                glitch.push(c as char).ok();
            }
        }

        let color = if frame % 2 == 0 { ACCENT_CYAN } else { ACCENT_LIME };
        Text::with_text_style(
            glitch.as_str(),
            Point::new(64, 52),
            MonoTextStyle::new(&FONT_10X20, color),
            centered,
        ).draw(display).unwrap();

        delay.delay_millis(55);
    }

    // Final resolved title in solid white.
    Rectangle::new(Point::new(0, 36), Size::new(128, 22))
        .into_styled(PrimitiveStyle::with_fill(BG))
        .draw(display).unwrap();

    Text::with_text_style(
        target,
        Point::new(64, 52),
        MonoTextStyle::new(&FONT_10X20, TEXT_WHITE),
        centered,
    ).draw(display).unwrap();

    delay.delay_millis(300);

    // ── 5. Progress bar ───────────────────────────────────────────────
    Text::with_text_style(
        "BOOTING",
        Point::new(64, 82),
        MonoTextStyle::new(&FONT_6X10, TEXT_DIM),
        centered,
    ).draw(display).unwrap();

    Rectangle::new(Point::new(14, 88), Size::new(100, 8))
        .into_styled(PrimitiveStyle::with_stroke(ACCENT_CYAN, 1))
        .draw(display).unwrap();

    for step in 1u32..=20 {
        let fill_w = (step * 96) / 20;
        Rectangle::new(Point::new(16, 90), Size::new(fill_w, 4))
            .into_styled(PrimitiveStyle::with_fill(ACCENT_LIME))
            .draw(display).unwrap();
        delay.delay_millis(35);
    }

    delay.delay_millis(250);

    // ── 6. Wipe to black ─────────────────────────────────────────────
    for row in 0u32..16 {
        Rectangle::new(Point::new(0, (row * 8) as i32), Size::new(128, 8))
            .into_styled(PrimitiveStyle::with_fill(BG))
            .draw(display).unwrap();
        delay.delay_millis(8);
    }
}

// ── Static UI chrome ──────────────────────────────────────────────────────

/// Draws the fixed chrome of the dashboard (header, dividers, labels, static values).
///
/// Called once after the boot animation. Only dynamic value areas are repainted
/// on each loop iteration by [`draw_dynamic_ui`], keeping SPI traffic minimal.
fn draw_static_ui<DI, MODEL>(display: &mut mipidsi::Display<DI, MODEL, Output<'_>>)
where
    DI: mipidsi::interface::Interface,
    MODEL: mipidsi::models::Model<ColorFormat = Rgb565>,
    Rgb565: mipidsi::interface::InterfacePixelFormat<DI::Word>,
{
    let left     = TextStyleBuilder::new().alignment(Alignment::Left).build();
    let right    = TextStyleBuilder::new().alignment(Alignment::Right).build();
    let centered = TextStyleBuilder::new().alignment(Alignment::Center).build();

    // Header bar (y=0..15)
    Rectangle::new(Point::new(0, 0), Size::new(128, 16))
        .into_styled(PrimitiveStyle::with_fill(HEADER_BG))
        .draw(display).unwrap();

    // Cyan accent rule below header (y=16)
    Rectangle::new(Point::new(0, 16), Size::new(128, 1))
        .into_styled(PrimitiveStyle::with_fill(ACCENT_CYAN))
        .draw(display).unwrap();

    // Title text — centred in the header
    Text::with_text_style(
        "ESP32  DASHBOARD",
        Point::new(64, 12),
        MonoTextStyle::new(&FONT_6X10, ACCENT_CYAN),
        centered,
    ).draw(display).unwrap();

    // Section divider 1 (y=44)
    Rectangle::new(Point::new(0, 44), Size::new(128, 1))
        .into_styled(PrimitiveStyle::with_fill(DIVIDER))
        .draw(display).unwrap();

    // "DISPLAY" section label
    Text::with_text_style(
        "DISPLAY",
        Point::new(4, 56),
        MonoTextStyle::new(&FONT_6X10, TEXT_DIM),
        left,
    ).draw(display).unwrap();

    // Static display info: width and height never change
    Text::with_text_style(
        "WIDTH",
        Point::new(4, 68),
        MonoTextStyle::new(&FONT_6X10, ACCENT_CYAN),
        left,
    ).draw(display).unwrap();

    Text::with_text_style(
        "128",
        Point::new(124, 68),
        MonoTextStyle::new(&FONT_6X10, TEXT_WHITE),
        right,
    ).draw(display).unwrap();

    Text::with_text_style(
        "HEIGHT",
        Point::new(4, 80),
        MonoTextStyle::new(&FONT_6X10, ACCENT_CYAN),
        left,
    ).draw(display).unwrap();

    Text::with_text_style(
        "128",
        Point::new(124, 80),
        MonoTextStyle::new(&FONT_6X10, TEXT_WHITE),
        right,
    ).draw(display).unwrap();

    // Section divider 2 (y=82)
    Rectangle::new(Point::new(0, 82), Size::new(128, 1))
        .into_styled(PrimitiveStyle::with_fill(DIVIDER))
        .draw(display).unwrap();

    // "LAST UPDATE" section label
    Text::with_text_style(
        "LAST UPDATE",
        Point::new(4, 94),
        MonoTextStyle::new(&FONT_6X10, TEXT_DIM),
        left,
    ).draw(display).unwrap();
}

// ── Dynamic UI ────────────────────────────────────────────────────────────

/// Repaints all dynamic (changing) content of the dashboard.
///
/// Called once per second with the current [`DashState`]. Each value area is
/// cleared with a black rectangle before the new value is drawn, preventing
/// ghost text when a shorter string replaces a longer one.
fn draw_dynamic_ui<DI, MODEL>(
    display: &mut mipidsi::Display<DI, MODEL, Output<'_>>,
    state: &DashState,
)
where
    DI: mipidsi::interface::Interface,
    MODEL: mipidsi::models::Model<ColorFormat = Rgb565>,
    Rgb565: mipidsi::interface::InterfacePixelFormat<DI::Word>,
{
    let left     = TextStyleBuilder::new().alignment(Alignment::Left).build();
    let right    = TextStyleBuilder::new().alignment(Alignment::Right).build();
    let centered = TextStyleBuilder::new().alignment(Alignment::Center).build();

    // ── STATUS row (y=18..30) ─────────────────────────────────────────
    Rectangle::new(Point::new(0, 18), Size::new(128, 13))
        .into_styled(PrimitiveStyle::with_fill(BG))
        .draw(display).unwrap();

    Text::with_text_style(
        "STATUS",
        Point::new(4, 28),
        MonoTextStyle::new(&FONT_6X10, ACCENT_CYAN),
        left,
    ).draw(display).unwrap();

    Text::with_text_style(
        "READY",
        Point::new(124, 28),
        MonoTextStyle::new(&FONT_6X10, ACCENT_LIME),
        right,
    ).draw(display).unwrap();

    // ── Uptime row (y=32..44) ─────────────────────────────────────────
    Rectangle::new(Point::new(0, 32), Size::new(128, 13))
        .into_styled(PrimitiveStyle::with_fill(BG))
        .draw(display).unwrap();

    Text::with_text_style(
        "UP",
        Point::new(4, 42),
        MonoTextStyle::new(&FONT_6X10, ACCENT_LIME),
        left,
    ).draw(display).unwrap();

    let (h, m, s) = state.uptime_hms;
    let mut uptime_str: heapless::String<12> = heapless::String::new();
    write!(uptime_str, "{:02}:{:02}:{:02}", h, m, s).ok();

    Text::with_text_style(
        uptime_str.as_str(),
        Point::new(124, 42),
        MonoTextStyle::new(&FONT_6X10, TEXT_WHITE),
        right,
    ).draw(display).unwrap();

    // ── Tick counter (y=96..119) — large centred cyan number ─────────
    Rectangle::new(Point::new(0, 96), Size::new(128, 24))
        .into_styled(PrimitiveStyle::with_fill(BG))
        .draw(display).unwrap();

    let mut tick_str: heapless::String<16> = heapless::String::new();
    write!(tick_str, "tick {}", state.tick).ok();

    Text::with_text_style(
        tick_str.as_str(),
        Point::new(64, 118),
        MonoTextStyle::new(&FONT_10X20, ACCENT_CYAN),
        centered,
    ).draw(display).unwrap();
}

// ── Hardware init ─────────────────────────────────────────────────────────

/// Initialises the ESP32 hardware and returns the peripheral singleton.
///
/// Sets the CPU to maximum frequency. No heap allocator is needed since
/// WiFi/smoltcp have been removed — all allocations are stack-based.
fn init_hardware() -> Peripherals {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    esp_hal::init(config)
}