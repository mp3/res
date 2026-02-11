# res

This is a learning-oriented Rust project that implements a **6502 CPU emulator** and runs an SDL2-rendered demo game (snake-like).

Although the window title in `main.rs` says `NES Emulator`, the current code is **not a full NES emulator** (no full PPU/APU/mapper pipeline). Instead, it loads game bytecode into a 6502 execution environment and runs it.

## What this project is

- Implements 6502 CPU registers, flags, stack behavior, and addressing modes in Rust
- Defines an opcode table in `opcodes.rs`
- Runs the CPU in `run_with_callback`, integrating input, random values, and rendering in the callback
- Displays a 32x32 pseudo frame buffer scaled by 10x via SDL2
- Runs a snake-like demo controlled by WASD input

## Project structure

- `src/lib.rs`  
  Library crate entry point that exposes the reusable CPU core modules (`cpu`, `opcodes`).
- `src/cpu.rs`  
  Core CPU implementation (registers, memory, instruction execution, stack, branching, instruction handlers) plus unit tests, exported via `lib.rs`.
- `src/opcodes.rs`  
  Opcode metadata definitions (mnemonic, length, cycles, addressing mode) and map construction, exported via `lib.rs`.
- `src/main.rs`  
  SDL2 demo frontend (window/input/render loop) that consumes the CPU library.

## How to run

### Requirements

- Rust (`cargo`)
- SDL2 development library (`libSDL2` available on the system)

### Build check

```bash
cargo check
```

### Run

```bash
cargo run
```

After launch:
- Use `W / A / S / D` for direction input
- Press `Esc` or close the window to quit

### Run with instruction trace (optional)

```bash
CPU_TRACE=1 cargo run
```

When `CPU_TRACE` is `1` or `true`, the runtime prints one line per executed instruction step (next opcode + registers/flags/stack pointer) from the CPU callback loop.

## Input and display memory map (demo behavior)

`main.rs` uses specific CPU memory addresses as I/O-style ports in the callback.

- `0x00FF`: keyboard input (`w/s/a/d` ASCII values)
- `0x00FE`: per-frame random value in range `1..15`
- `0x0200..0x05FF`: 32x32 display buffer (`1 byte = color index`)

Color indexes are converted to RGB in `main.rs` via `color()`.

## Currently implemented (high level)

- Major 6502 instruction groups (`LDA/STA/ADC/SBC/AND/EOR/ORA`, shifts/rotates, compare, branches, stack ops, etc.)
- Multiple addressing modes (Immediate / ZeroPage / Absolute / Indirect variants, etc.)
- Unit tests in `cpu.rs`

## Notes

- This is an educational/experimental implementation, not a complete NES emulator.
- Depending on your environment, `cargo test` / `cargo run` may require proper SDL2 linker setup.

---

## Development roadmap

詳細な不足機能と実装タスクは [`TASKLIST.md`](./TASKLIST.md) を参照してください。

This project tracks progress in 3 short milestones:

- **Sprint 1 (stabilization)**
  - `todo!()`-based unsupported opcode crash handling replaced with explicit errors
  - ✅ Start separating CPU core and SDL frontend (`lib.rs` + SDL frontend `main.rs`)
- **Sprint 2 (observability)**
  - ✅ Add instruction trace logging (toggleable)
  - Add cycle accounting (+ branch/page-cross penalties)
- **Sprint 3 (NES entry point)**
  - Add ROM loader (iNES header + PRG mapping)
  - Add Mapper 0 (NROM)
  - Add minimal PPU register/VRAM scaffold

### Definition of Done (per milestone)

- **Milestone complete** when all tasks in that sprint are checked in [`TASKLIST.md`](./TASKLIST.md).
- **Task complete** when:
  - behavior is implemented,
  - there is at least one test or reproducible manual check,
  - and README/TASKLIST status is updated.
