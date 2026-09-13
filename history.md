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
