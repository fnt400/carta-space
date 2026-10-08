# carta-gui

Experimental v0.2 graphical shell for Carta Space.

The shell uses Iced 0.14 for native desktop input/windowing and the `tiny-skia` software renderer, and consumes the canonical `carta-app` state and actions. It deliberately does not implement a second editor model.

During this first stage it is excluded from the main Rust 1.85 workspace because Iced 0.14 requires Rust 1.88 or newer. The shell has its own cross-platform CI and will join the main workspace only after the graphical path is stable enough to justify raising the v0.2 MSRV.

## Current scope

- open an existing Carta Archive supplied explicitly on the command line;
- render the current document, in-place Canon-style caret and Cat selection from `carta-app`, with narrow/wide/extended states and saved/dirty blink rates;
- translate physical Left Ctrl / Left Alt into the shared LEAP behavior;
- support Right Ctrl / Leap Again with the same semantics as the TUI;
- pass LEAP query text and basic editing keys through `carta_app::Action`;
- preserve Right Alt/AltGr as ordinary text input;
- show the shared command palette, prompts, confirmations, selectors and LEAP query;
- tick the shared scheduler while idle for autosave/checkpoints/sync;
- autosave dirty state when the shell exits.

This is not yet the finished editor surface. Visual vertical navigation, full multi-document view projection, clipboard integration and pointer-based editing remain subsequent GUI work. The current shell deliberately mirrors the proven TUI presentation: a centered monospace writing surface, generated document separators, transient modal overlays, and a persistent status bar.

### Cursor and font

The editor paints the character at the logical caret without inserting fake Unicode squares into the authored text. Narrow (at-point highlight), wide (highlight behind the insertion point) and extended highlight behaviors follow the Cat semantics in the shared application model. The blinking caret uses approximately 3 cycles/second after saving and 1 cycle/second with unsaved edits. At end-of-line or end-of-document a stable blank cell is reserved for caret visibility; the text itself is never modified. Pixel-perfect Cat raster geometry and multiline wrap fidelity will need a dedicated text renderer beyond this first rich-text shell.

Iosevka Regular is bundled directly with the GUI and loaded at startup, so users do not need to install fonts or depend on the host's font resolution. The checked-in base64 asset is decoded in memory rather than installing anything system-wide (see `LICENSES/OFL-iosevka.txt`). The original TUI remains unchanged; its font is selected by the terminal emulator. To match both visually, select **Iosevka** in the terminal as well.

## Run

```sh
cargo +stable run --manifest-path crates/carta-gui/Cargo.toml -- /path/to/archive
```

Rust 1.88 or newer is required.
