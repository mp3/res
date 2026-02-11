# res

This is a learning-oriented Rust project that implements a **6502 CPU emulator core** and an SDL2-rendered demo frontend.

Although the window title says `NES Emulator`, this repository is still **not a full NES emulator** (no full PPU/APU/mapper pipeline yet).

## Workspace layout

- `crates/res-core`
  - SDL-free reusable library crate (`res_core`)
  - Contains CPU/opcodes/ROM/mapper/PPU core modules and unit tests
- `crates/res-sdl`
  - SDL2 demo frontend binary crate (`res-sdl`)
  - Consumes `res_core` and provides window/input/render loop

## Requirements

- Rust (`cargo`)
- SDL2 development library (`libSDL2`) only for building/running `res-sdl`

## Commands

### Test core (SDL2 not required)

```bash
cargo test
```

`default-members` is configured to run core tests by default.

### Build SDL frontend

```bash
cargo build -p res-sdl
```

### Run demo

```bash
cargo run -p res-sdl
```

After launch:
- Use `W / A / S / D` for direction input
- Press `Esc` or close the window to quit

### Run with iNES ROM (Mapper 0 / NROM only)

```bash
cargo run -p res-sdl -- path/to/game.nes
```

When no ROM path is provided, it falls back to built-in demo bytecode.

### Run with instruction trace (optional)

```bash
CPU_TRACE=1 cargo run -p res-sdl
```

## Input and display memory map (demo behavior)

`crates/res-sdl/src/main.rs` uses these CPU memory addresses as I/O-style ports:

- `0x00FF`: keyboard input (`w/s/a/d` ASCII values)
- `0x00FE`: per-frame random value in range `1..15`
- `0x0200..0x05FF`: 32x32 display buffer (`1 byte = color index`)

## Notes

- This is an educational/experimental implementation.
- Full NES compatibility is out of scope for current state.

## Development roadmap

Detailed tasks are tracked in [`TASKLIST.md`](./TASKLIST.md).

- **Sprint 1 (stabilization)**
  - ✅ Memory space fix (`[u8; 0x10000]`)
  - ✅ Unsupported opcode handling via explicit errors
  - ✅ Initial CPU/SDL separation
- **Sprint 2 (observability)**
  - ✅ Instruction trace logging
  - ✅ Cycle accounting (+ branch/page-cross penalties)
  - ✅ Typo cleanup and naming consistency
- **Sprint 3 (NES entry point)**
  - ✅ ROM loader (iNES + mapper 0 validation)
  - ✅ Mapper 0 (NROM)
  - ✅ Minimal PPU scaffold
