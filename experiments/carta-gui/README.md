# Carta GUI experiment

This is the first deliberately small graphical frontend for Carta Space.

It reuses the existing `carta-tui::App`, `CompositeEditor`, `carta-core::Archive`, autosave, session and LEAP logic instead of creating a second editor implementation. The experiment is a separate Cargo workspace so that it does not change the Carta v0.1 release build, root lockfile or Nix package while the interface is being evaluated.

## Scope

The current prototype targets GNOME/Wayland and uses `eframe`/`egui` 0.32.3, the last egui generation compatible with Carta's Rust 1.85 MSRV.

Implemented:

- open the normal Carta archive, or an explicit archive path;
- the same Creation Date, Modification Date, Work and Search editor projections loaded by `App`;
- a centered 80-column graphical text surface drawn directly from `CompositeEditor`;
- normal text insertion, newline, backspace, erase, cursor movement, undo/redo and autosave;
- Cat highlight rendering and the existing Carta LEAP engine;
- portable LEAP keys, which do not consume Alt/AltGr;
- simple buttons for New Document, Creation Date View, Modification Date View, Undo, Redo and Save;
- the normal Carta session file, so the current view/document position can be restored.

LEAP uses the same physical keys as the TUI:

- physical **Left Ctrl**: LEAP backward;
- physical **Left Alt**: LEAP forward;
- tapping a LEAP key preserves the Cat single-step behavior;
- pressing both LEAP keys preserves Cat highlight extension;
- **Right Ctrl + LEAP** invokes LEAP Again;
- releasing the active LEAP key ends the query, as in the TUI.

The event loop reads the physical `winit::keyboard::KeyCode` before egui translates keyboard input. While a LEAP is pending or active, its query keystrokes are consumed by Carta's LEAP state machine rather than becoming ordinary GUI shortcuts. **Right Alt / AltGr is not a LEAP key** and is left to normal text input.

The prototype intentionally does not yet reproduce every TUI dialog, palette, trash/history/conflict screen or Work selector.

The GUI does not use `egui::TextEdit`: Carta's existing editor owns cursor, selection, Cat highlight and editing semantics.

## Run

From the repository root:

```sh
./scripts/carta-gui.sh
```

To open another archive:

```sh
./scripts/carta-gui.sh /path/to/archive
```

To create a test archive explicitly:

```sh
./scripts/carta-gui.sh --create /path/to/new-archive
```

If the normal default archive does not exist, initialize/import it with the normal Carta TUI first. The GUI experiment does not silently create a new default archive.
