# chip8-emu

A small CHIP-8 emulator, written in Rust.
It uses the nightly feature `duration_saturating_ops`, so Rust nightly is required until this feature is stabilized.
It's been tested with a few programs including [corax89's chip8-test-rom](https://github.com/corax89/chip8-test-rom).
It should be invoked as:

```
chip8-emu <scale-factor> <clock-speed> <program-path>
```

For example:

```
chip8-emu 8 400 my_rom.ch8
```

The original 4x4 hexadecimal keypad is mapped as follows:

```
╔═══╦═══╦═══╦═══╗    ╔═══╦═══╦═══╦═══╗
║ 1 ║ 2 ║ 3 ║ C ║    ║ 1 ║ 2 ║ 3 ║ 4 ║
╠═══╬═══╬═══╬═══╣    ╠═══╬═══╬═══╬═══╣
║ 4 ║ 5 ║ 6 ║ D ║    ║ Q ║ W ║ E ║ R ║
╠═══╬═══╬═══╬═══╣ -> ╠═══╬═══╬═══╬═══╣
║ 7 ║ 8 ║ 9 ║ E ║    ║ A ║ S ║ D ║ F ║
╠═══╬═══╬═══╬═══╣    ╠═══╬═══╬═══╬═══╣
║ A ║ 0 ║ B ║ F ║    ║ Z ║ X ║ C ║ V ║
╚═══╩═══╩═══╩═══╝    ╚═══╩═══╩═══╩═══╝
```

Thanks to [Matthew Mikolay's CHIP-8 technical reference](https://github.com/mattmikolay/chip-8/) and anybody who contributed to the [CHIP-8 Wikipedia page](https://en.wikipedia.org/wiki/CHIP-8).
