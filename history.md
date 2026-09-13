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

The bootloader stop at `0x00000a80` and its unwind warning are expected with `haltAfterReset`; continue past it to reach an application breakpoint.
