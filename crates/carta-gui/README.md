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
- color Markdown headings, emphasis, strong text, links, quotes and code **without hiding markup or modifying authored text**; a versioned syntax cache avoids parsing unchanged Documents;
- zoom the writing font using physical Right Ctrl + minus (smaller) or Right Ctrl + equals (larger), from 12 to 32 pixels in 2-pixel steps, adjusting the screen-space row geometry without changing document content;
- position the caret by clicking, and select with left-button drag, sharing the canonical editor selection model rather than maintaining a second text buffer;
- restore the previous View, document and UTF-8 cursor byte position from the same XDG session sidecar used by the TUI (saved when the application closes and during maintenance);
- tick the shared scheduler only as needed: about once per second while dirty, every two seconds for a transient status, and every 15 seconds otherwise;
- autosave dirty state when the shell exits.

This is not yet the finished editor surface. Pixel-exact soft wrapping, complete non-editable View projections and pointer-based editing remain subsequent GUI work. The current shell deliberately mirrors the proven TUI presentation: a centered monospace writing surface, generated document separators, transient modal overlays, and a persistent status bar.

### Cursor and font

The editor paints the character at the logical caret without inserting fake Unicode squares into the authored text. Narrow (at-point highlight), wide (highlight behind the insertion point) and extended highlight behaviors follow the Cat semantics in the shared application model. The caret is permanently visible, without a blink timer. At end-of-line or end-of-document a stable blank cell is reserved for caret visibility; the text itself is never modified. Auto-follow scrolling uses a presentation-only visual-row estimate based on the fixed Iosevka advance and line height. It adds virtual blank space above and below the rendered text and does not modify the Archive. Exact positioning for tabs, East Asian wide characters, grapheme clusters and mixed font metrics awaits a renderer-measured layout.

### GUI interactions and persistence

PageUp/PageDown move to the beginning of the preceding/following Document without wrapping. Like Home/End, these are structural LEAP movements (also while holding a physical LEAP key). Collapse View renders only the first three **visual** rows of each Document, with separators intact. The status bar keeps a view-specific color: neutral gray for Creation Date, blue for Modification Date, and each Work's stored accent with automatically selected contrasting text. Navigation and mode/collapse transitions re-anchor the writing window on the caret.

The source remains Markdown; syntax color applies to the source text and
delimiters. Selection and the Cat caret take precedence over syntax colors.
Only the editor writing font is zoomed; modal/status typography remains fixed.
Font zoom resets to 18 px on a new launch.

A mouse press positions the caret; dragging a held left button extends a
standard selection. This shares the same editor semantics as keyboard
selection. Clicks on generated separators snap to the nearest document's
beginning, without making separators editable. Pointer hit testing currently
uses the approximate single-width Iosevka geometry used for soft-wrap
navigation; tabs, wide Unicode graphemes, ligatures and mixed-width text may
require more accurate renderer-level hit testing. Dragging beyond the
viewport does not yet auto-scroll.

The last View/Document/cursor is loaded and saved at
`$XDG_DATA_HOME/carta/session-<archive-id>.json` (or
`~/.local/share/carta/session-<archive-id>.json`). This is exactly the
existing TUI sidecar format. The file is *outside* the Archive, never
synchronized or included in portable exports. Save errors appear in the
status line, and shutdown performs a final best-effort save. The kill switch
does not persist state.

**Performance**: The virtual View renderer indexes visual rows and
reuses Markdown syntax across cursor-only interactions. Verify in release
mode with a large Archive, extended click-drag operations and at least
fifteen minutes of writing, rather than treating automated tests as proof
of sustained responsiveness.

Iosevka Regular is bundled directly with the GUI and loaded at startup, so users do not need to install fonts or depend on the host's font resolution. The checked-in base64 asset is decoded in memory rather than installing anything system-wide (see `LICENSES/OFL-iosevka.txt`). The original TUI remains unchanged; its font is selected by the terminal emulator. To match both visually, select **Iosevka** in the terminal as well.

## Run and performance checks

For evaluating responsiveness, always test the optimized release build. The
debug build has unoptimized software rasterization (`tiny-skia`) and is not a
representative performance benchmark.

```sh
cargo run --release --manifest-path crates/carta-gui/Cargo.toml -- /path/to/archive
```

Rust 1.88 or newer is required.

### Automatic push after every commit (non-blocking)

Git synchronization belongs to the Archive, not the graphical renderer.
**Every Carta checkpoint queues an outbound push immediately.** The GUI
starts its blocking Tokio worker even during uninterrupted writing, while
a prompt is open, or while the current Document is an empty provisional
draft. Only the already committed Git HEAD is published; unsaved content
is neither uploaded nor overwritten. If another checkpoint happens while
a push is in flight, the newer generation remains pending and is published
by the next worker rather than being incorrectly acknowledged.

The worker checks the remote; already-synced and fast-forward pushes require
no private clone. When the remote has changed, an owner-private temporary
clone performs the Git fetch/merge/push. Incoming changes are applied to
the live Archive **only when the editor has been idle and is clean**, with
HEAD, remote-configuration and worktree checks. If the local base advanced
during the worker, Carta discards the stale staged result and retries.

A failed push/pull, an unresolved merge conflict, or a commit waiting
without a configured remote makes the **entire GUI status bar red**, with a
persistent error message (independent of expiring editor status messages). Failed transfers remain queued, retry
automatically with bounded backoff, and clear the red warning only after
successful synchronization. Periodic checks every three minutes also
detect commits made outside Carta. Explicit `Sync Now` uses the same worker.

**Safe Quit (GUI v0.2):** Window Close and the Quit command save and checkpoint
locally, then keep the window open while the final Git HEAD is published and
verified asynchronously. If another push is already in flight, Carta finishes
that worker and explicitly starts a fresh publication for the final checkpoint.
An error, conflict, or unverifiable result displays a persistent warning with
**Retry push**, **Continue editing**, and **Exit without push (risk)** choices.
No normal configured-remote Quit may silently leave an unpushed final commit.
Only an explicitly confirmed offline exit or emergency/system termination
bypasses that safeguard. Explicit local-only Archives exit without network
access. After confirmed publication, Carta advances the local
`refs/remotes/carta-sync/carta` tracking ref when the fetch endpoint was a
confirmed push destination: `git status` must not falsely report a missing
push merely because the tracking ref was stale.

The reference TUI retains its previous automatic synchronization scheduler;
there is no new Iced dependency in the shared Archive model.

**Known limits:** Network operations run off-thread; local checkpoints
and application of remote changes still perform short synchronous disk/Git
operations during a quiet period and have not been proven imperceptible on
enormous Archives. Incoming changes cannot currently be applied while an
empty provisional Document is open, so pull can wait for that draft to be
finalized. Automatic three-way conflict resolution is not implemented:
both histories are preserved and the status bar stays red until the
conflict is resolved. These are distinct from the now-unblocked outbound
push of previously committed snapshots.


### Virtual View renderer — sustained responsiveness

The GUI no longer builds one giant `rich_text` widget containing every
Document in the current View. That approach made text shaping, styling, and
software rasterization proportional to the **entire View**, so a View with
many Documents became progressively unusable.

The GUI now builds a disposable, font-size-dependent index of visual rows
and generated Document separators. Iced receives only the rows in/near the
visible viewport (about three screens including overscan). The rest of the
Archive is represented by empty virtual spacer heights to preserve native
scrollbar and mouse-wheel behavior. Source text is still held canonically in
`carta-app` and is never duplicated to make a View.

The row index is initially built once for a newly loaded View or after
changing font size. Cursor-only navigation reuses it. A text mutation updates
the affected Document's rows, not every Document in the View. A Fenwick
prefix-sum index locates global rows and updates Document block heights in
O(log N) for N Documents. Undo/redo also invalidate the affected Document's
projection. Markdown syntax is compiled into non-overlapping, indexed ranges
once per changed Document and never rescanned on routine navigation; the
disposable syntax cache is capped at 128 Documents.

This removes the all-Document reconstruction/shaping/rasterization path
that caused the View-size-dependent slowdown. It does **not** remove the
initial cost of loading the Archive and building the row index, nor does
it make editing one extremely large individual Document fully incremental.
Those are separate, measurable scaling cases and should not be confused
with redraw behavior.

**Regression acceptance:** Build with `--release`, compare typing latency,
LEAP, native wheel scrolling, scrollbar dragging, mouse selection and zoom
for a small View and a large View with thousands of synthetic Documents.
Exercise the application for at least 30 minutes, including Undo/Redo,
switching Views, changing font size, reaching the beginning/end, and cursor
restore after restart. Measure CPU/RSS and the optional profile timing.
No automated test can certify the physical GUI experience.

### Opt-in profiling

To distinguish application handling from text reconstruction, run:

```sh
CARTA_GUI_PROFILE=1 cargo run --release --manifest-path crates/carta-gui/Cargo.toml -- /path/to/archive
```

Every 15 seconds of reported activity, the GUI prints anonymized timing
counts, average and worst times (milliseconds) for `input` dispatch,
`view` widget construction, `scroll` row calculations and `maintenance` handling. It never prints archive
text, names, paths, search queries or keys. There is no profiling overhead
unless the environment flag is enabled.

A fast `view` timer does **not** establish a fast render: layout, shaping and
`tiny-skia` rasterization happen downstream. A system profiler (e.g.
`perf`) and an actual long-lived GUI session are required to identify a
remaining compositor/rasterization bottleneck. Compare idle/typing behavior
near startup and after at least ten minutes on the same test Archive.


## Focused manual checks

After local Clippy/test/build verification, confirm on a Wayland/X11 desktop:

1. Leave the GUI idle for a minute and measure CPU usage; moving the cursor should not restart a background blink loop.
2. Hold physical Left Control and type a pattern: the backward LEAP query should receive ordinary characters without executing desktop shortcuts or inserting them into the Document; compare with Left Alt forward LEAP and check AltGr.
3. Use each arrow key through soft-wrapped paragraphs and across Document boundaries, resize the window, and confirm that the writing point remains visible near two-thirds of the viewport. On mixed-width Unicode, report drift separately from ASCII text.
4. In Work View, verify an uninterrupted generated separator across the writing width, without changing or copying authored text.
5. Extend a Cat highlight with both LEAP keys, paste the selection into another application, use Right Control+C on an extended highlight for Cat COPY, and use Right Control+C without one to paste clipboard text, including CRLF line endings.

