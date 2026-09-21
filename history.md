# probe-rs Debugging Progress

The probe-rs debugging setup is validated end-to-end:

1. Debug Mate discovery
2. SWD communication
3. Firmware flashing
4. DAP server operation
5. Correct ELF/debug symbols
6. Core halt and continue
7. A verified hardware breakpoint in Rust source
8. RTT & `defmt` logging configured in launch configuration and Cargo settings
9. SVD file (`resources/nrf52840.svd`) and Peripheral Viewer configured

## Key Findings & Gotchas

- **Pre-Launch Build Task**: Added [.vscode/tasks.json](.vscode/tasks.json) with `cargo build` and configured `"preLaunchTask": "cargo build"` in [.vscode/launch.json](.vscode/launch.json), ensuring the binary is automatically recompiled before every debug flash/launch.
- **SVD & Peripheral Viewer**:
  - Downloaded official `nrf52840.svd` into `resources/nrf52840.svd`.
  - Configured `"svdFile": "${workspaceFolder}/resources/nrf52840.svd"` in [.vscode/launch.json](.vscode/launch.json).
  - During an active debug session, the **XPeripherals** / **Peripheral Viewer** tree in the Run & Debug sidebar parses MCU registers (GPIO, UARTE, RTC, etc.) directly from target memory.
- **`panic-probe` Deliberate Triggering**: Added a `"panic"` command to the CLI in [src/main.rs](src/main.rs) (`defmt::panic!(...)` or `panic!(...)`). When triggered, `panic-probe` outputs the formatted panic message and file/line location over `defmt` RTT, and then executes a hardware `BKPT` instruction to halt the core cleanly for the debugger.
- **RTT & `defmt` Setup**:
  - `launch.json`: Added `rttEnabled: true` and `rttChannelFormats: [{ "channelNumber": 0, "dataFormat": "Defmt" }]` under `coreConfigs`.
  - `Cargo.toml`: Added `defmt = "1.1.1"` dependency alongside `defmt-rtt = "1.3.0"`.
  - `.cargo/config.toml`: Added `[env] DEFMT_LOG = "info"` and link script `"-C", "link-arg=-Tdefmt.x"`.
- **`embassy-nrf` `"rt"` Feature**: Must be enabled in [Cargo.toml](Cargo.toml). Without `"rt"`, interrupt vector handlers (such as `RTC1` / Interrupt #17 used by `time-driver-rtc1`) default to `DefaultHandler_`, causing an unhandled exception crash upon timer execution.
- **Bootloader Halt**: The stop at `0x00000a80` and its unwind warning are expected with `haltAfterReset`; continue past it to reach application breakpoints.
- **Async Functions & Variables View**: Functions marked with `#[embassy_executor::main]` or async tasks compile into compiler-generated state machines (`{async_fn_env#0}`). The Variables pane cannot directly decode this struct, showing `<unknown>` or unimplementation errors. To inspect local variables cleanly, select the inner closure frame in Call Stack or step into normal synchronous helper functions/methods.
- **XIAO nRF52840 Pin Multiplexing & Red LED Conflict**: 
  - **Active-Low Hardware**: The onboard RGB LEDs are wired with anodes to 3.3V (VDD) and cathodes to GPIO pins. Therefore, `Level::Low` (`set_low()`) turns the LED **ON** (sinks current), while `Level::High` (`set_high()`) turns the LED **OFF**.
  - **Shared Pin Conflict (P0.26 / D7)**: On the Seeed Studio XIAO nRF52840 (Sense) board schematic, MCU pin **`P0.26`** is routed to two physical destinations at the same time: the cathode of the onboard **Red LED** and the external header pin labeled **`D7`**.
  - **Why the Red LED Failed with UART**: When an external serial/UART cable is plugged into header pin `D7` (often assumed to be RX), the external USB-UART adapter's driver line holds/pulls `P0.26` low externally. Because the pin is driven by external hardware, software calls like `led.set_high()` cannot pull the line high against the external adapter, leaving the Red LED stuck ON.
  - **Solution**: Avoid using `P0.26` (Red LED / D7) and `P0.30` (Green LED / D8) when those header pins are connected to external hardware. Use **`P0.06` (Blue LED)** instead, as it is dedicated exclusively to the internal LED and not exposed to any external header pins.
- **3-Bit Weighted Color Enum (`bsp::Color`)**:
  - Encapsulated in [src/bsp.rs](src/bsp.rs) with `BitOr` support (e.g. `Color::Red | Color::Green | Color::Blue == Color::White`).
  - Active-low polarity is handled inside `Leds::set_color(Color)`.

| Color | Bits (`B G R`) | Binary | Decimal | Combination |
| :--- | :---: | :---: | :---: | :--- |
| **`Off`** | `0 0 0` | `0b000` | `0` | No LEDs active |
| **`Red`** | `0 0 1` | `0b001` | `1` | Red |
| **`Green`** | `0 1 0` | `0b010` | `2` | Green |
| **`Yellow`** | `0 1 1` | `0b011` | `3` | Red + Green |
| **`Blue`** | `1 0 0` | `0b100` | `4` | Blue |
| **`Magenta`** | `1 0 1` | `0b101` | `5` | Red + Blue |
| **`Cyan`** | `1 1 0` | `0b110` | `6` | Green + Blue |
| **`White`** | `1 1 1` | `0b111` | `7` | **Red + Green + Blue** |

- **Decoupled Task Architecture (`embassy-sync` Channels)**:
  - **`LED_CHANNEL`**: MPSC channel for color updates processed sequentially by `led_task`.
  - **`UART_TX_CHANNEL`**: MPSC channel for outbound UART strings/bytes, processed by `uart_task`'s TX loop.
  - **`UART_RX_CHANNEL`**: SPSC/MPSC channel streaming received bytes from `uart_task`'s RX loop to consumer tasks (such as the CLI parser in `main`).
  - **`uart_task` Concurrency**: Splits `Uarte` into TX and RX sub-drivers and runs both concurrently via `embassy_futures::join::join(tx_loop, rx_loop)`.

# IMU Register CLI (Onboard LSM6DS3TR-C)

Added a minimal, inspectable CLI for the onboard 6-axis IMU on the Seeed Studio XIAO nRF52840 Sense: `imu-status`, `imu-r <reg>`, and `imu-w <reg> <value>`. Boot-time configuration was later added (see below) once the register CLI foundation was validated on hardware.

## Architecture

- New task module [src/main_imu.rs](src/main_imu.rs) owns the `Twim` I2C driver exclusively, following the same channel/task pattern as `main_leds.rs`: a `Channel<ImuCommand>` + `ImuSender`/`sender()`, and an `imu_task` that processes commands in a loop.
- The IMU task replies directly to UART via a new `main_uart::UartTxSender`/`tx_sender()`, rather than routing responses back through `cli_task` — this keeps `cli_task` a pure dispatcher and lets the IMU task own all response formatting.
- `src/bsp.rs` owns the onboard IMU's dedicated peripherals: `twispi1`, `imu_sda`, `imu_scl`, and `imu_power` (see gotchas below).

## Key Findings & Gotchas

- **Wrong I2C bus is a silent, indefinite hang, not a clean error.** The onboard LSM6DS3TR-C is wired to `TWISPI1` with SDA=`P0.07`/SCL=`P0.27` — **not** the exposed `D4`/`D5` header pins (`P0.04`/`P0.05`, which live on `TWISPI0`). Talking to the wrong/open bus caused `Twim::write_read(...).await` to hang forever: `Twim::async_wait()` waits for a `STOPPED`/`ERROR` interrupt event that never fires on a floating bus, unlike a real address-NACK which resolves almost instantly. Confirmed by cross-referencing a working sibling project (`../xiao-blinky`) for the same board.
- **The onboard IMU/mic power rail is gated by `P1.08`.** It must be driven `Level::High` with `OutputDrive::HighDrive` and held alive for the program's lifetime (never dropped — `main.rs` binds it to `let _imu_power = board.imu_power;`), plus a ~10ms settle delay before the first I2C transaction. Without this, the sensor has no power regardless of correct pins.
- **Internal SDA/SCL pull-ups were a red herring.** An earlier fix attempt enabled `Twim::Config`'s internal pull-ups as a defensive measure; the known-working reference leaves them at default (`false`/`false`), confirming pins + power were the real fix, not pull-ups.
- **I2C address**: probe `0x6A` first, then fall back to `0x6B` (`IMU_ADDR_PRIMARY`/`IMU_ADDR_SECONDARY`), and remember whichever address responded for subsequent `imu-r`/`imu-w` calls in the `imu_task`'s local state (`imu_addr: Option<u8>`).
- **`WHO_AM_I` (register `0x0F`) expected value is `0x6A`, not `0x69`, for this specific chip.** ST's plain LSM6DS3 reports `0x69`; the **LSM6DS3TR-C** variant actually populated on this board reports `0x6A`. Verified on real hardware. Note this numerically coincides with `IMU_ADDR_PRIMARY = 0x6A` (I2C address) — same value, different meaning, not a bug.
- **Defensive timeout**: every I2C transaction in `main_imu.rs` is wrapped in a 100ms `embassy_time::with_timeout`, distinguishing `ImuIoError::Timeout` from `ImuIoError::Bus(twim::Error)` (the latter logged via `defmt::warn!`). This means a genuine future wiring/config regression reports a clear UART error instead of hanging the IMU task forever.
- **`Twim::new(twim_peri, irq, sda, scl, config, tx_ram_buffer)`** — note `irq` comes before `sda`/`scl`. The `tx_ram_buffer: &mut [u8]` argument only matters for write buffers that aren't in RAM (e.g. static data placed in flash); since our writes are always built from stack arrays, an empty `&mut []` is sufficient.
- **`RAMBufferTooSmall` from a bare const array literal.** `twim.write_read(addr, &[OUT_START_REG], &mut buf)` failed with `twim::Error::RAMBufferTooSmall` even though `OUT_START_REG` is just a `u8`. Because `OUT_START_REG` is a `const`, the whole `&[OUT_START_REG]` array literal is an "rvalue eligible for static promotion" — the compiler is free to place it in flash (`.rodata`) instead of on the stack, and EasyDMA can't read flash directly. Fix: bind it to a local first (`let start_reg = [OUT_START_REG];` then `&start_reg`) — a named local is a real stack slot and is never promoted, regardless of being initialized from a const. Note `read_reg`/`write_reg` never hit this because their `reg`/`value` are ordinary function *parameters* (not literals), so `&[reg]` there was never promotable in the first place.
- **Boot-time configuration**, copied from the working `../xiao-blinky` reference: on `imu_task` startup, `init_imu()` probes for the sensor and, if found, writes `CTRL1_XL` (`0x10`) and `CTRL2_G` (`0x11`) to `0x40` — 104 Hz output data rate, ±2g accelerometer range, ±250dps gyro range. The result (`configured OK`/`config FAILED`/`not found`) is reported once over UART at boot. This intentionally reverses the earlier "no boot-time config" decision now that the register CLI foundation is validated on hardware; `imu-w` still allows overriding these registers manually afterward.

## Status

Confirmed working end-to-end on real hardware: `imu-status`, `imu-r`, `imu-w`, and `imu-read` all function against the onboard LSM6DS3TR-C. `imu-read` reads the 12-byte gyro+accel burst starting at `OUTX_L_G` (`0x22`, auto-incrementing per LSM6DS3TR-C default `IF_INC=1`), decodes six little-endian `i16` axes, and converts to physical units using the sensitivity for the configured full-scale range (±2g accel: `0.061 mg/LSB`; ±250dps gyro: `8.75 mdps/LSB`), e.g. `A:+0.02,-0.01,+1.00g G:+0.3,-0.1,+0.0dps`. Deferred/optional: bumping I2C frequency to `K400` (cosmetic parity with the reference project, not required for correctness); streaming/calibration remain out of scope by design.

Real sample captured while manually spinning the board about the Z axis:

```
A:-0.15,+0.66,+0.81g G:+125.5,+60.5,-181.6dps
```

The dominant gyro rate (`Gz=-181.6dps`) correctly lands on the axis being rotated, confirming axis mapping and sign are consistent with the physical rotation.

## Next Steps

- Bump I2C frequency to `K400` for cosmetic parity with the `../xiao-blinky` reference (not required for correctness).
- **Set up and detect IMU hardware interrupts** (via the LSM6DS3TR-C's `INT1`/`INT2` lines and `INT1_CTRL`/`INT2_CTRL`/`TAP_CFG`/`WAKE_UP_*`/`FREE_FALL` registers), specifically:
  - **Free-fall detection** — configure the free-fall threshold/duration registers and route the free-fall interrupt to an `INT` pin so the MCU can react to a true free-fall event instead of polling `imu-read`.
  - **Acceleration-above-threshold (wake-up) detection** — configure the wake-up threshold/duration registers to interrupt when acceleration exceeds a set level. This is intended to detect *exiting* free fall (impact) as well as to support startup-on-motion-detect (waking the app from an otherwise idle state).
- Streaming and calibration commands remain out of scope by design.
