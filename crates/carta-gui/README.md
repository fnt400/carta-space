# carta-gui

Experimental v0.2 graphical shell for Carta Space.

The shell uses Iced 0.14 for native desktop input/windowing/rendering and consumes the canonical `carta-app` state and actions. It deliberately does not implement a second editor model.

During this first stage it is excluded from the main Rust 1.85 workspace because Iced 0.14 requires Rust 1.88 or newer. The shell has its own cross-platform CI and will join the main workspace only after the graphical path is stable enough to justify raising the v0.2 MSRV.

## Current scope

- open an existing Carta Archive supplied explicitly on the command line;
- render the current document, caret and Cat selection from `carta-app`;
- translate physical Left Ctrl / Left Alt into the shared LEAP behavior;
- pass LEAP query text and basic editing keys through `carta_app::Action`;
- preserve Right Alt/AltGr as ordinary text input;
- show the shared command palette, prompts, confirmations, selectors and LEAP query;\n- tick the shared scheduler while idle for autosave/checkpoints/sync;\n- autosave dirty state when the shell exits.

This is not yet the finished editor surface. Visual vertical navigation, full multi-document view projection, clipboard integration, pointer-based editing and final styling remain subsequent GUI work.

## Run

```sh
cargo +stable run --manifest-path crates/carta-gui/Cargo.toml -- /path/to/archive
```

Rust 1.88 or newer is required.
