# carta-gui

Experimental v0.2 graphical shell for Carta Space.

The shell uses Iced 0.14 for native desktop input/windowing and the `tiny-skia` software renderer, and consumes the canonical `carta-app` state and actions. It deliberately does not implement a second editor model.

During this first stage it is excluded from the main Rust 1.85 workspace because Iced 0.14 requires Rust 1.88 or newer. The shell has its own cross-platform CI and will join the main workspace only after the graphical path is stable enough to justify raising the v0.2 MSRV.

## Current scope

- open an existing Carta Archive supplied explicitly on the command line;
- render the current document, a permanently visible Cat caret, and shared narrow/wide/extended Cat highlights;
- translate physical Left Ctrl / Left Alt into the shared LEAP behavior;
- support Right Ctrl / Leap Again with the same semantics as the TUI;
- pass forward/backward LEAP queries using native physical modifiers and layout-aware key text (including Ctrl-suppressed text for backward LEAP);
- preserve Right Alt/AltGr as ordinary text input;
- show the shared command palette, prompts, confirmations, selectors and LEAP query;
- support Left/Right/Up/Down editor navigation with a visual-width adapter, and center the caret near two-thirds of the available editor height;
- support Ctrl-Right+C Cat Copy or clipboard paste, and automatically export newly extended Cat selections to the system clipboard;
- tick the shared scheduler at 2-second intervals instead of repainting the window at animation frame rates;
- autosave dirty state when the shell exits.

This is not yet the finished editor surface. Pixel-exact soft wrapping, complete non-editable View projections and pointer-based editing remain subsequent GUI work. The current shell deliberately mirrors the proven TUI presentation: a centered monospace writing surface, generated document separators, transient modal overlays, and a persistent status bar.

### Cursor and font

The editor paints the character at the logical caret without inserting fake Unicode squares into the authored text. Narrow (at-point highlight), wide (highlight behind the insertion point) and extended highlight behaviors follow the Cat semantics in the shared application model. The caret is permanently visible, without a blink timer. At end-of-line or end-of-document a stable blank cell is reserved for caret visibility; the text itself is never modified. Auto-follow scrolling uses a presentation-only visual-row estimate based on the fixed Iosevka advance and line height. It adds virtual blank space above and below the rendered text and does not modify the Archive. Exact positioning for tabs, East Asian wide characters, grapheme clusters and mixed font metrics awaits a renderer-measured layout.

Iosevka Regular is bundled directly with the GUI and loaded at startup, so users do not need to install fonts or depend on the host's font resolution. The checked-in base64 asset is decoded in memory rather than installing anything system-wide (see `LICENSES/OFL-iosevka.txt`). The original TUI remains unchanged; its font is selected by the terminal emulator. To match both visually, select **Iosevka** in the terminal as well.

## Run

```sh
cargo run --manifest-path crates/carta-gui/Cargo.toml -- /path/to/archive
```

Rust 1.88 or newer is required.

## Focused manual checks

After local Clippy/test/build verification, confirm on a Wayland/X11 desktop:

1. Leave the GUI idle for a minute and measure CPU usage; moving the cursor should not restart a background blink loop.
2. Hold physical Left Control and type a pattern: the backward LEAP query should receive ordinary characters without executing desktop shortcuts or inserting them into the Document; compare with Left Alt forward LEAP and check AltGr.
3. Use each arrow key through soft-wrapped paragraphs and across Document boundaries, resize the window, and confirm that the writing point remains visible near two-thirds of the viewport. On mixed-width Unicode, report drift separately from ASCII text.
4. In Work View, verify an uninterrupted generated separator across the writing width, without changing or copying authored text.
5. Extend a Cat highlight with both LEAP keys, paste the selection into another application, use Right Control+C on an extended highlight for Cat COPY, and use Right Control+C without one to paste clipboard text, including CRLF line endings.

