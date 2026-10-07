# Carta Space — v0.2 Architecture

This document records the implementation boundaries for the v0.2 transition. It is intentionally narrower than the product philosophy in `WHITEPAPER.md` and the archive rules in `SPECIFICATION.md`.

The goal is simple:

> one Carta behavior, multiple possible frontends.

## Stable layers

### carta-format

Owns the durable archive-format types, serialization, validation and compatibility rules.

It must not depend on any interactive frontend.

### carta-core

Owns canonical Archive/domain operations: Documents, Volumes, Works, Git-backed history, synchronization, Trash/Wipe, retrieval/LEAP search, links/backlinks and related validation.

It must not know about terminal rows, windows, pixels, native key codes or GUI toolkits.

### carta-publish

Owns publication transforms such as PDF generation. Publication is not part of the editor rendering loop.

### carta-cli

Provides conventional administrative/scriptable operations.

## Shared interactive layer: carta-app

`carta-app` is the reusable interactive application layer between the domain core and presentation frontends.

The v0.2 shared layer currently owns:

- `CompositeEditor`, `Cursor` and `Region`;
- Cat highlight, copy/move/erase and undo/redo behavior implemented by the editor;
- frontend-independent session values (`Session`, `SavedView`, `Position`);
- `View`;
- the autosave/checkpoint/sync `Scheduler`;
- palette matching;
- application modes for Editing, palette, prompt, confirmation, selector and LEAP;
- frontend-neutral `Action` values for text editing, LEAP input, horizontal character navigation, document navigation and document boundaries;
- the main `App` application controller and shared help/cheatsheet content;
- frontend-neutral prompt, confirmation, and selector editing/navigation through `ModeAction`.

`carta-tui/src/app.rs` and `carta-tui/src/help.rs` are now compatibility re-exports of `carta-app`; the terminal frontend no longer owns the canonical application controller or help content.

`carta-app` must not depend on Ratatui, Crossterm, Winit, a GUI widget toolkit, browser APIs, Android APIs or other frontend technology.

## Frontends

### carta-tui

The v0.1.2 TUI is frozen for feature development.

During the v0.2 migration it serves two purposes:

1. compatibility frontend for existing terminal users;
2. regression oracle showing that extracting shared code did not change established Carta behavior.

TUI-specific compatibility mechanisms, including Portable Keyboard Mode, remain inside the terminal adapter. Portable mode state and its palette toggle are not part of `carta-app`.

### carta-gui

The v0.2 canonical desktop frontend is graphical and targets Linux, Windows and macOS.

Before the full GUI is built, a small keyboard experiment must prove that the chosen input layer exposes:

- physical Left/Right Control separately;
- physical Left/Right Alt separately;
- key press and key release;
- normal text input;
- AltGr without accidental LEAP;
- Unicode/IME behavior sufficiently for Carta.

Toolkit choice follows this test, not the other way around.

## Intended event boundary

The target direction is:

```text
native input
    ↓
frontend adapter
    ↓
semantic Action
    ↓
carta-app
    ↓
updated shared state + optional Effect
    ↓
frontend/platform adapter
```

A physical `LeftCtrl DOWN` is a desktop input event. The reusable application layer should eventually receive the semantic meaning, for example "begin backward LEAP", not a Winit or Crossterm key object.

Likewise, a future mobile frontend may invoke the same semantic operation from touch controls without pretending that a physical Left Control exists.

## Effects

Some operations need the host platform: clipboard, file selection/save, opening external resources, window integration and similar services.

Do not create a large abstract platform framework in advance.

Introduce explicit effects only when a real v0.2 operation needs them. The eventual rule is that shared application logic describes the request and the host frontend performs it.

Archive persistence and Git-backed canonical operations remain domain concerns in `carta-core`; not every I/O call needs to become a frontend effect.

## Migration sequence

### Stage A — shared primitives

Status: initial slice complete.

The editor, session values, View, scheduler and palette matching have been extracted from `carta-tui` without intentionally changing v0.1.2 behavior.

### Stage B — semantic application actions

Status: complete for the pre-GUI preparation.

`carta-app::Action` now carries the first frontend-neutral editing intents and LEAP input intents. The TUI translates ordinary text-editing keys into these actions, and physical LEAP start/end/query handling already crosses the same semantic seam before invoking application behavior.

`carta-tui/src/input.rs` is now the explicit Crossterm adapter. It owns the terminal-specific pending/active LEAP modifier state, suppressed release bookkeeping, Right Control state, physical LeftCtrl/LeftAlt mapping, ordinary editing-key translation, and emergency-kill event recognition. These details must not migrate into `carta-app`.

That adapter state is now encapsulated: its representation is private and `main.rs` accesses it only through a small transition API (`pending`, `active`, `begin_pending`, `promote_pending`, `clear_*`, release suppression, and Right Control accessors). This makes later changes to terminal state handling local to the adapter.

`input.rs` now also owns the terminal modifier state machine. Physical press/release sequences are reduced to TUI-local intents such as `TapLeap`, `BeginLeap`, `LeapAgain`, `ExtendLastLeapHighlight`, and structural LEAP intents. `main.rs` applies those intents to the shared/application behavior and performs platform effects such as clipboard writes.

This distinction is deliberate: `TuiInputIntent` is not a second shared command model. It is an adapter-internal interpretation layer for Crossterm quirks. Only behavior that is genuinely frontend-independent should graduate to `carta-app::Action`.

`main.rs` now primarily owns terminal compatibility shortcuts, clipboard execution, the TUI-specific command palette extension, and rendering-dependent navigation. Prompt/confirmation/selector editing semantics are shared through `ModeAction`. The canonical `App` controller lives in `carta-app`. Further extraction should happen only when a second frontend provides a concrete shared requirement.

Visual Up/Down/Home/End/PageUp/PageDown are deliberately not generalized yet: their current semantics depend on visual layout width/height. Layout-independent character-left/right, previous/next-document and document-start/end operations already cross the shared `Action` boundary. Do not encode terminal columns or future GUI pixel/font assumptions into `Action` merely to move code. Decide the shared layout/navigation contract only when the graphical text renderer gives us the second concrete implementation.

Prompt, confirmation, and selector editing/navigation are now shared through `ModeAction`. The TUI-specific palette extension remains local because it includes Portable Keyboard Mode.

Do not move additional command/application behavior merely for architectural tidiness. Further shared abstractions should be driven by the second frontend.

Do not move terminal-only compatibility commands merely to make the crate diagram look cleaner. In particular, Portable Keyboard Mode remains a frozen TUI concern until a concrete second frontend demonstrates a genuinely shared need.

### Stage C — concrete platform effects

Status: intentionally deferred until the graphical frontend exists.

The shared controller still contains native filesystem-oriented import/export/package operations. This is acceptable for the current native desktop transition, but GUI file dialogs and any future web/mobile frontend must introduce the minimum explicit effect boundary required by real platform needs. Do not design that abstraction speculatively before the second frontend exists.

### Stage D — graphical shell

This is now the next architectural milestone and the point where user input is required for toolkit/input-layer selection. Add the desktop frontend only after the physical-keyboard probe succeeds.

Start with the smallest complete slice: window, text rendering, cursor/Cat highlight, text input and true momentary LEAP. Add palette, Views, Works and other surfaces by consuming the same shared application state.

## Web and mobile

Web/mobile reuse is a design constraint, not a promise that every current native crate must compile unchanged to WebAssembly.

Do not abstract Git/filesystem/storage pre-emptively.

The immediate requirement is to keep editing and interaction semantics reusable. Storage adapters for browser or mobile environments should be designed only when those frontends are actually implemented.

## Invariants

- No frontend owns a second editor implementation.
- No frontend owns a second LEAP state machine.
- Canonical Archive data and Draft 0.1 format remain unchanged by this refactor.
- `carta-app` contains no toolkit-specific event types or presentation geometry.
- Presentation choices such as theme, font size, pixel layout and window chrome are frontend concerns.
- Right Alt/AltGr must remain ordinary text input, not LEAP.
- The GUI may be modern without becoming a conventional panel-heavy editor.
- Refactors must remain incremental and testable.
- The public repository must contain only sanitized synthetic development/test data.

## Documentation authority

- `SPECIFICATION.md`: archive format and canonical persisted semantics.
- `INTERACTION-CONTRACT.md`: frozen v0.1.2 interaction baseline unless explicitly superseded by accepted v0.2 decisions.
- `DESIGN-DECISIONS.md`: accepted architectural/product decisions and rationale.
- `ARCHITECTURE.md`: current v0.2 code-boundary and migration plan.
- `AGENTS.md` and `DEVELOPMENT-WORKFLOW.md`: development-agent operating rules.
