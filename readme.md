# ESP32 TFT Dashboard

A bare-metal `no_std` Rust application for the ESP32 that displays a live dashboard on a 128×128 ST7735s TFT screen over SPI. No WiFi, no OS, no heap allocator — just the chip driving pixels.

---

## Features

- Hacker-style boot animation (~3.5 s): CRT flash, scanline sweep, typewriter text, glitch-resolve title, progress bar, and a wipe transition
- Live dashboard showing device status, uptime (HH:MM:SS), display info, and a loop tick counter
- Rotating arc spinner in the bottom-right corner so you can always tell the device is alive
- All rendering via [embedded-graphics](https://github.com/embedded-graphics/embedded-graphics) and [mipidsi](https://github.com/almindor/mipidsi) — no framebuffer, direct SPI writes only

---

## Hardware

**MCU:** ESP32 (Xtensa LX6)

**Display:** 128×128 ST7735s TFT (BGR panel variant)

### Wiring

| Signal | ESP32 GPIO |
|--------|------------|
| DC     | GPIO22      |
| RST    | GPIO19      |
| CS     | GPIO21      |
| SCK    | GPIO18     |
| MOSI   | GPIO23     |


|Pin Name   |	Description                                 |
|-----------|-----------------------------------------------|
|VCC        |	Power supply (2.8V to 3.3V)					|
|GND        |	Ground										|
|SCL (CLK)  |	Serial Clock (SPI clock input)				|
|SDA (MOSI) |	Serial Data (SPI data input)				|
|RES (RST)  |	Reset pin (active low)						|
|DC (A0)    |	Data/Command control pin					|
|CS         |	Chip Select (active low)					|
|LED        |	Backlight control (connect to power or PWM) |

SPI clock: 40 MHz, Mode 0 (CPOL=0, CPHA=0). The display is write-only so no MISO connection is needed.

---

## Display Layout

```
┌────────────────────────┐  y=0
│  ESP32  DASHBOARD      │  header bar (navy)
├════════════════════════┤  y=16  cyan accent rule
│ STATUS        READY    │  y=18
│ UP          00:04:32   │  y=32  uptime HH:MM:SS
├────────────────────────┤  y=44  section divider
│ DISPLAY                │  y=46  section label
│ WIDTH             128  │  y=58
│ HEIGHT            128  │  y=70
├────────────────────────┤  y=82  section divider
│ LAST UPDATE            │  y=84  section label
│       tick N           │  y=96  loop counter (large, cyan)
│                     ◑  │  y=121 rotating spinner
└────────────────────────┘  y=128
```

---

## Prerequisites

- Rust toolchain with the `xtensa-esp32-none-elf` target
- [`espup`](https://github.com/esp-rs/espup) to install the Xtensa Rust fork
- [`espflash`](https://github.com/esp-rs/espflash) to flash the chip

```sh
cargo install espup espflash
espup install
```

Then activate the environment (the `espup` installer will tell you the exact command for your shell, e.g. `. $HOME/export-esp.sh`).

---

## Building & Flashing

```sh
cargo build --release
espflash flash --monitor target/xtensa-esp32-none-elf/release/<binary-name>
```

Or with `cargo run` if `espflash` is configured as the runner in `.cargo/config.toml`:

```sh
cargo run --release
```

---

## Project Structure

```
src/
└── bin/
    └── main.rs       # Everything: hardware init, boot animation, UI, main loop
Cargo.toml
.cargo/
    config.toml       # Target, runner, and linker settings
```

### Key functions in `main.rs`

| Function | Purpose |
|---|---|
| `main()` | Hardware init, display setup, boot animation, main loop |
| `boot_animation()` | ~3.5 s startup sequence |
| `draw_static_ui()` | Draws chrome that never changes (header, dividers, labels) |
| `draw_dynamic_ui()` | Repaints value areas once per second |
| `draw_spinner()` | Animates a rotating arc in the bottom-right corner |
| `init_hardware()` | Configures CPU clock, returns peripheral singleton |

---

## Dependencies

| Crate | Role |
|---|---|
| `esp-hal` | Bare-metal HAL — SPI, GPIO, delay, timers |
| `mipidsi` | ST7735s display driver |
| `embedded-graphics` | 2D drawing, text, primitives |
| `embedded-hal-bus` | `ExclusiveDevice` SPI + CS wrapper |
| `heapless` | Stack-allocated `String` for text formatting |
| `esp-println` | UART-backed `println!` for debug output |
| `esp-bootloader-esp-idf` | App descriptor for the IDF second-stage bootloader |

---

## Colour Palette

All colours are `Rgb565`. The panel uses BGR byte order, handled automatically by mipidsi's `ColorOrder::Bgr` option.

| Name | Hex (approx.) | Used for |
|---|---|---|
| `BG` | `#000000` | Background |
| `HEADER_BG` | `#001020` | Header bar |
| `ACCENT_CYAN` | `#00FFFF` | Titles, labels, tick counter |
| `ACCENT_LIME` | `#10C810` | Uptime label, status, progress bar |
| `TEXT_WHITE` | `#FFFFFF` | Dynamic values |
| `TEXT_DIM` | `#304830` | Section labels |
| `DIVIDER` | `#082010` | Rule lines |

---

<div style="position: relative; width: 100%; padding-top: calc(max(56.25%, 400px));">
  <iframe src="https://app.cirkitdesigner.com/project/98b09322-5431-4346-82f7-0c296b2164f0?view=interactive_preview" style="position: absolute; top: 0; left: 0; width: 100%; height: 100%; border: none;"></iframe>
</div>
<!--Please include the following link, which help us continue to improve and support the embed, making it a valuable tool for your audience.--> <p style= "margin-top: 5px;" >Edit this project interactively in <a href="https://app.cirkitdesigner.com/project/98b09322-5431-4346-82f7-0c296b2164f0" target = "_blank">Cirkit Designer</a>.</p>

https://docs.cirkitdesigner.com/component/b84f56c9-bce5-469e-80eb-11e99bceaf9d

## License

MIT
