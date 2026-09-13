# probe-rs Debugging Progress

The probe-rs debugging setup is validated end-to-end:

1. Debug Mate discovery
2. SWD communication
3. Firmware flashing
4. DAP server operation
5. Correct ELF/debug symbols
6. Core halt and continue
7. A verified hardware breakpoint in Rust source

## Optional Next Checks

1. Step over several statements.
2. Inspect `p`, `config`, and `cursor` in Variables/Watch.
3. Pause while running and inspect the call stack.
4. Break inside the UART command loop.
5. Trigger a deliberate panic and verify `panic-probe` reporting.
6. Add an SVD file and test Peripheral Viewer.
7. Enable RTT explicitly for `defmt` output.

## Key Findings & Gotchas

- **`embassy-nrf` `"rt"` Feature**: Must be enabled in [Cargo.toml](Cargo.toml). Without `"rt"`, interrupt vector handlers (such as `RTC1` / Interrupt #17 used by `time-driver-rtc1`) default to `DefaultHandler_`, causing an unhandled exception crash upon timer execution.
- **Bootloader Halt**: The stop at `0x00000a80` and its unwind warning are expected with `haltAfterReset`; continue past it to reach application breakpoints.
- **Async Functions & Variables View**: Functions marked with `#[embassy_executor::main]` or async tasks compile into compiler-generated state machines (`{async_fn_env#0}`). The Variables pane cannot directly decode this struct, showing `<unknown>` or unimplementation errors. To inspect local variables cleanly, select the inner closure frame in Call Stack or step into normal synchronous helper functions/methods.
