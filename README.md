# Paradox63

The Paradox63(aka 'P63') an original 8-bit retro CPU architecture, toolchain, and emulator written in Rust(and Paradox63 assembly), inspired by the design philosophy and personality of classic machines from the late 1970s and early 1980s, such as the Motorola 6809/HD6309, SNES's 6502 and Intel's 8080.

The P63 architecture is shaped by old-school hardware thinking mixed with modern software discipline and hindsight, designed by the idea of what it was like to wrestle with every bit.

## Overview

The Paradox63 aims to define, explore and tool an 8-bit architecture from the ground up, meant to give a strong focus on understanding the machine underneath the software:

- a fully custom ISA

- CPU/core logic

- assembler and disassembler tools

- CPU emulator

- direct machine control

- expressive instruction sets

## Usage

The release zip has `p63-sim.exe`, the signed `bios.bin`, example `.p63c` carts, and example
source programs in `games/` and `utils/`. Keep `bios.bin` next to the exe.

As of v0.5.0, to play a cartridge, open CMD and:
    `p63-sim play game.p63c`

The console boots to its Home BIOS screen; press Start/A to run the cartridge.
Hold `Select` during the startup logo to skip to the Home BIOS screen.
With no file it opens with an empty slot, and you can insert one from the Console menu. Includes memory(todo), and system settings.
First cold boot will create a `.nvram` file, the console's battery backed memory.
You can swap games at any time using shortcuts.

| Key | Button |
|-|-|
| Arrow keys | D-pad |
| Enter | Start |
| Backspace or C | Select |
| Z / X | A / B |
| Esc | Quit |

The Console menu inserts, ejects and resets cartridges, switches CPU speed
(Low 3.58 / Normal 7.16 / Turbo 14.32 MHz), and turns the power off and on.
Programs can switch speeds in the .p63 code.

Write your own: assemble a `.p63` source file into a cartridge, then play it.
Paradox63 has its proprietary key to sign 'Official P63 Cartridges', as shown in the Home with assets released.

    p63-sim asm hello.p63 -c hello.p63c
    p63-sim play hello.p63c

Other commands:

| Command | What it does |
|-|-|
| `asm FILE [-c out.p63c] [-o out.bin] [-s]` | Assemble; `-c` writes a cartridge, `-s` prints the symbol table |
| `run FILE [-f FRAMES] [--hold BUTTON] [--peek ADDR:LEN] [--screenshot out.bmp] [--wav out.wav]` | Run without a window and report what happened |
| `call FILE ROUTINE[,ROUTINE...] [A=$12 X=$2000 ...]` | Run routines and print every register and the cycle count |
| `test ROM... [--frames N]` | Run test ROMs and print PASS or FAIL |

Every command takes `--bios PATH` if the BIOS lives somewhere else. Run
`p63-sim help COMMAND` for the full options.

## Building

You need Rust (stable) from https://rustup.rs.

    cargo build --release -p p63-sim
    cargo test --workspace

The exe lands in `target/release/`. Put `bios.bin` from a release next to it.
