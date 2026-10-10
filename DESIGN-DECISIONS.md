# Carta Space — Design Decisions
## Draft 0.1

This document records architectural decisions and the reasoning behind them.

It is intentionally non-normative. `SPECIFICATION.md` is authoritative for the format.

---

## DD-001 — Keep the core deliberately small

**Decision:** Carta Space 0.x will implement only the concepts required to test the central model: Documents, monthly Volumes, Works, CommonMark semantics, Git history, and content-oriented navigation.

**Why:** The project is explicitly about reducing conceptual overhead. Adding tags, assets, bibliography, synchronization, semantic search, or AI before demonstrated need would undermine that goal and multiply implementation failure modes.

**Rule of thumb:** Complexity must be earned by a demonstrated problem.

---

## DD-002 — Use standard technologies rather than inventing a low-level format

**Decision:** Use UTF-8, CommonMark, JSON, UUIDv7, Git, and ZIP.

**Why:** Carta Space's contribution should be the information model and interaction model, not another parser, identifier scheme, archive format, or revision-control engine.

This also maximizes recoverability with generic tools.

---

## DD-003 — CommonMark 0.31.2 is the canonical Document body syntax

**Decision:** `content.md` uses CommonMark 0.31.2 without extensions in Draft 0.1.

**Alternatives considered:**

- original informal Markdown;
- GitHub Flavored Markdown;
- Pandoc Markdown;
- Org mode;
- custom rich-text metadata.

**Why CommonMark:** It has a precise specification and test suite while staying close to simple readable Markdown.

**Why not GFM:** Its additional syntax solves GitHub-specific and collaboration-oriented problems that Carta Space does not yet have.

**Why not Pandoc Markdown:** It is powerful precisely because it has many extensions. Adopting the whole dialect would prematurely enlarge the language.

**Why not Org:** The storage format must remain independent of any editor or frontend. Emacs may later be supported as an alternate frontend, but it is not the reference implementation.

**Why not separate rich-text ranges:** Range metadata becomes fragile when text is edited outside the reference frontend.

Pandoc remains a strong candidate for conversion and publishing.

---

## DD-004 — Administrative metadata stays outside Markdown

**Decision:** Document administration uses `meta.json`; no required YAML or JSON front matter is placed in `content.md`.

**Why:** An extracted `content.md` should contain authored content, not archive bookkeeping.

Document title is content and can therefore be expressed with a Markdown heading. UUID and creation timestamp are administrative facts and belong in metadata.

---

## DD-005 — Use JSON sidecars

**Decision:** Metadata uses RFC 8259 JSON.

**Alternatives considered:** YAML, TOML, XML, custom key/value syntax.

**Why:** JSON is standardized, language-independent, ubiquitous, small enough for the required structures, and suitable for data written primarily by software rather than by humans.

The core metadata model is intentionally too small to justify a more elaborate format.

---

## DD-006 — Use UUIDv7 for identities

**Decision:** Archives, Documents, and Works use UUIDv7.

**Why:** Stable opaque identities allow content to move through different logical views without identity depending on filenames, titles, or text.

UUIDv7 is standardized by RFC 9562 and includes temporal ordering properties useful for implementation while remaining globally unique.

Creation time is still stored explicitly. The UUID is identity, not the sole semantic timestamp.

---

## DD-007 — Monthly Volumes are automatic chronological partitions

**Decision:** Physical Document directories are grouped under `volumes/YYYY/MM/` according to creation month.

**Alternatives considered:**

- manually created Volumes;
- size-limited Volumes inspired directly by floppy capacity;
- topic folders;
- one unlimited directory.

**Why monthly:** Calendar months provide predictable bounded contexts without requiring the user to classify or maintain them.

A Volume answers "when did this enter my archive?", not "what is this about?"

Documents remain in their creation Volume after later edits.

---

## DD-008 — Works are ordered references, not containers of copied text

**Decision:** A Work stores an ordered array of Document UUIDs.

**Why:** This separates intellectual order from chronological storage.

The design is analogous in spirit to EPUB's separation between publication resources and its ordered spine.

A Document can therefore appear in more than one Work without duplication.

---

## DD-009 — Document boundaries in composite Views are protected

**Decision:** Boundaries shown between Documents in a Work View or chronological View are generated structural elements, not characters stored in Markdown.

**Why:** If a separator could be erased by Backspace, ordinary text editing could accidentally destroy archive structure.

Merging, splitting, and reordering are explicit operations.

---

## DD-010 — Git is the canonical history mechanism

**Decision:** A writable Draft 0.1 Archive is a Git repository.

**Earlier alternative:** Leave the choice of version-control engine to each frontend.

**Why it was rejected:** Frontends using incompatible history systems would interoperate only on current content, defeating the goal of a common archival format.

Git is widely implemented, mature, efficient for text, and independently inspectable.

The normal Carta Space interface must nevertheless hide Git concepts from users.

---

## DD-011 — Autosave and history are different

**Decision:** Current files are saved frequently; Git checkpoints occur separately.

**Why:** A commit for every keystroke would be wasteful and conceptually wrong. Autosave protects recent work from failure. History records meaningful recoverable states.

Exact checkpoint policy is intentionally deferred until prototype use provides evidence.

---

## DD-012 — Recoverable Trash and permanent Wipe are separate

**Decision:** Trash removes an object from the active Archive while preserving historical recoverability. Wipe permanently removes a trashed Document from Carta-managed history and derived state on the current device.

**Why:** Version history is valuable only if ordinary removal remains recoverable, but users also need an explicit way to remove sensitive or unwanted Document content from Carta Space itself.

Wipe cannot promise deletion from independent external copies, backups, clones, other devices, filesystem snapshots, or physical storage remnants outside Carta Space's control.

---

## DD-013 — Git history belongs to the portable archive

**Decision:** Draft 0.1 `.cat` packages include archive history.

**Why:** Two copies of the same Carta Space archive should not silently have different notions of its past merely because one was exported.

**Open issue:** A future specification may prefer a Git bundle over raw `.git/` metadata for cleaner interchange. The initial prototype should test the simplest representation first.

---

## DD-014 — Working directory and portable package are different forms

**Decision:** Editing happens in an ordinary directory tree. `.cat` is a ZIP-based transport package.

**Why:** Continually rewriting a compressed archive would complicate autosave, failure recovery, Git, and external inspection.

The package is created atomically when required.

---

## DD-015 — Semantic authoring precedes presentation

**Decision:** The core records semantic text structure, not page typography.

**Why:** Authors should state that something is a heading, quotation, emphasis, or list without deciding its exact font and geometry during composition.

Printing and publication are rendering operations.

The first built-in PDF backend uses Typst with one fixed Carta Classic publication profile and a controlled embedded font set. The backend is isolated in `carta-publish`: Typst is a renderer, not part of the canonical storage model, and may be replaced later without changing Archive semantics or authored Markdown.

---

## DD-016 — No tags in the initial core

**Decision:** Draft 0.1 has no tag or keyword system.

**Why:** LEAP, full-text content, chronological scope, Git history, and Works should first be tested as retrieval mechanisms.

If real use demonstrates a recurring retrieval problem that tags solve, they can be designed from evidence.

---

## DD-017 — No synchronization or concurrent editing initially

**Decision:** The first implementation is local, single-user, single-writer.

**Why:** Synchronization would introduce merge conflicts, distributed identity, locking, causality, and network failure before the central interaction model has been validated.

The underlying format should avoid choices that make future synchronization impossible, but it does not solve that problem now.

---

## DD-018 — Unknown future data must survive old frontends

**Decision:** Frontends should preserve unknown metadata members and unknown archive resources whenever possible.

**Why:** Extensibility is useful only if adding a new feature does not make older tools destructive.

Future capabilities should be additive where practical.

No empty `assets`, `tags`, `bibliography`, `ai`, or similar directories are created pre-emptively.

---

## DD-019 — Canonical authored data must not depend on an index

**Decision:** Search indexes, databases, caches, embeddings, or other acceleration structures are always derived data unless a future specification explicitly changes this rule. This includes lexical full-text indexes, link/backlink indexes, and semantic vector indexes.

**Why:** The archive must remain reconstructable from human-readable canonical files. Losing an index may temporarily reduce speed or optional retrieval capabilities, but must not lose authored text, identities, Work structure, or history.

---

## DD-020 — LEAP is an interface concept, not a storage-format property

**Decision:** Carta Space defines two momentary LEAP controls, backward and forward. The reference terminal frontend maps them to physical Left Control for backward and physical Left Alt for forward.

**Why:** Dogfooding showed that the left-hand pair is more useful ergonomically for the current reference interaction. The binding depends on modern terminal keyboard reporting that distinguishes physical Left Alt from Right Alt/AltGr. Right Alt/AltGr remains reserved for ordinary international text entry.

Exact key bindings remain outside the storage-format specification. A frontend on a platform that cannot expose the required physical modifier keys may choose a different mapping.

---

## Outstanding design risks

The first prototype must test rather than theorize away three major risks:

1. whether LEAP plus temporal scope and Works remains effective over a large archive;
2. whether users naturally choose useful Document boundaries without excessive management;
3. whether a continuous editable Work View can remain simple while safely projecting edits into multiple underlying Markdown Documents.

No additional organizational system should be introduced until one of these risks produces a concrete problem.

---

## DD-021 — Rust is the reference implementation language

**Decision:** The first reference implementation will be written in Rust.

**Why:** Carta Space is intended to be a durable standalone system rather than an editor-specific package. Rust provides strong types for the format model, explicit error handling, good testing support, efficient Unicode/text processing, mature serialization libraries, straightforward CLI distribution, and the ability to share one core across multiple frontends.

This is an implementation decision, not a format requirement. Independent implementations may use any language.

---

## DD-022 — Separate format, core, CLI, and frontend

**Decision:** The reference implementation will be split into at least four logical components: `carta-format`, `carta-core`, `carta-cli`, and `carta-tui`.

**Why:** Archive semantics must not become entangled with one user interface.

The core should be testable without a terminal or GUI. The CLI should make administrative and scripted operations available in a conventional Unix form. Interactive frontends should use the same core operations rather than maintaining a second implementation of Documents, Works, history, or Trash/Wipe.

This separation is also the foundation for possible future GTK, Emacs, web, or other frontends.

---

## DD-023 — The first interactive frontend is a TUI

> **Historical status:** fulfilled by v0.1.2. The terminal-first line is frozen; see DD-043.

**Decision:** The initial interactive frontend will be a full-screen terminal application built with Ratatui and Crossterm rather than GTK.

**Why:** Carta Space is primarily a keyboard-driven text environment and deliberately separates writing from final page presentation. A terminal UI therefore matches the project's minimal interaction model, works naturally over SSH, has low runtime overhead, and discourages premature addition of toolbars, panels, and presentation-oriented controls.

**Trade-off:** Traditional terminal protocols do not reliably expose physical left/right modifiers or key-release events. The reference TUI will therefore rely on modern enhanced keyboard reporting when available and must detect when the required event fidelity is unavailable.

A graphical frontend remains a possible future frontend, not a competing core implementation.

---

## DD-024 — Left Control and Left Alt are the reference TUI LEAP bindings

> **Historical status:** retained for the frozen v0.1.2 TUI. The physical gesture remains the target for the v0.2 desktop frontend; see DD-043.

**Decision:** In the reference TUI, physical Left Control is the binding for LEAP backward and physical Left Alt is the binding for LEAP forward. Physical Right Alt/AltGr and Right Control are not LEAP keys.

**Why:** Real use of the initial Left/Right Control experiment motivated moving forward LEAP to Left Alt. Enhanced Crossterm keyboard reporting can distinguish physical Left Alt from Right Alt, allowing the reference TUI to keep AltGr available for international text entry.

**Constraint:** The binding is a frontend decision, not archive or core semantics. If a terminal path cannot report the required physical modifier identity and press/release events, palette LEAP remains the compatibility path.

---

## DD-025 — Views are derived projections

**Decision:** A View is a runtime projection over canonical Archive state. Draft 0.1 does not introduce a persistent View object or a `views/` directory.

**Why:** Creation Date View, Modification Date View, and Work View already demonstrate that the same Documents can be presented through different intellectual or temporal organizations without copying them. Generalizing this principle permits future search-result, backlink, history, and semantic-search Views without adding another canonical storage hierarchy.

**Constraint:** A View must not become the only owner of authored Document content. Persistence of user-defined Views, if later required, needs an explicit format decision rather than an accidental frontend convention.

---

## DD-026 — Internal links reuse CommonMark and stable Carta identities

**Decision:** Internal references use ordinary CommonMark link syntax with reserved destinations of the form `carta:doc:<DocumentId>` and `carta:work:<WorkId>`.

**Why:** This requires no custom Markdown dialect, keeps the authored body readable in generic tools, and separates human-visible labels from stable object identity. A title or heading may change without breaking the reference.

**Constraint:** An unresolved internal prose link does not make the Archive structurally invalid. This is deliberately different from a Work's ordered Document references, which are structural and must resolve.

---

## DD-027 — Backlinks are derived, not authored metadata

**Decision:** Backlinks are computed as the inverse of canonical internal links. Draft 0.1 does not store a canonical backlink list in Document or Work metadata.

**Why:** Storing both outgoing links and backlinks would create two sources of truth that could diverge after external Markdown edits. A direct scan is sufficient for small Archives; a disposable index may accelerate large Archives.

---

## DD-028 — Semantic retrieval is optional derived infrastructure

**Decision:** Carta Space may later maintain semantic or vector indexes, but they are optional, model-dependent derived data and are not part of the initial core.

**Why:** Semantic retrieval can solve cases where remembered meaning does not share literal vocabulary with the original text, especially in large Archives. It should complement rather than replace exact content navigation, and the Archive must remain usable if the model, embeddings, or index disappear.

**Implementation rule:** Do not implement semantic indexing merely to anticipate scale. Introduce it only when real Archive use demonstrates a retrieval problem that exact search, temporal context, Works, and simpler derived indexes do not solve.

---

## DD-029 — Writing precedes organization

**Decision:** The normal Carta Space workflow must allow useful writing to begin without prior classification, naming, tagging, templating, or presentation decisions.

**Why:** Carta Space is intended to reduce the administrative work that conventional filesystems and knowledge-management applications place before content production. Requiring users to design where a thought belongs before they can record it would contradict the project's central content-first model.

**Consequence:** Organizational mechanisms may be introduced when they solve demonstrated retrieval or composition problems, but they should normally operate on content that already exists rather than becoming mandatory preconditions for capture.

---

## DD-030 — Power is available on demand, not presented by default

**Decision:** Interactive frontends should prefer a quiet default writing surface and reveal secondary capabilities transiently or when explicitly requested.

**Why:** A feature can impose cognitive cost even when the user is not actively using it if it occupies persistent interface space or demands continual maintenance. Backlinks, Views, history, publishing, and future retrieval tools remain compatible with distraction-free writing when they disappear after use.

**Consequence:** Persistent panels, dashboards, properties, toolbars, status elements, and similar interface chrome require justification. Discoverability alone is not sufficient reason to keep a feature permanently visible if a transient command, palette, View, or quasimode can serve the same need.

---

## DD-031 — Distraction-free is architectural, not cosmetic

**Decision:** Carta Space treats distraction-free use as a system-level objective rather than a visual theme or a deliberate restriction to obsolete capabilities.

**Why:** The project should use modern storage, indexing, history, rendering, and retrieval mechanisms when they reduce user administration. Internal sophistication is desirable when it produces a simpler writing experience.

A dedicated writing device with a keyboard and simple display is therefore a natural Carta Space frontend, but the project does not depend on any specific hardware, display technology, or retro-computing aesthetic.

**Consequence:** Different frontends may expose different amounts of functionality while sharing the same Archive and core semantics. A focused writer, richer desktop environment, or lightweight capture client should be considered specialized entrances to the same information space, not separate product models.

---

## DD-032 — v0.1 has stable Document boundaries and no split/merge

**Decision:** The first usable version does not expose Document split or merge operations. Document boundaries are stable once a Document becomes persistent.

**Why:** Split/merge immediately creates difficult questions about chronology, Volume assignment, shared Work membership, history, backlinks, and undo semantics. No demonstrated v0.1 use case justifies that complexity.

**Alternative provided:** `Duplicate as New` creates an independent current Document from an existing one. `New Linked Document` creates a current Document and inserts an ordinary link from the source.

---

## DD-033 — Trash and Wipe are distinct user concepts

**Decision:** Recoverable removal is called `Trash`. Permanent removal from Carta-managed history on the current device is called `Wipe`.

**Why:** Calling both operations "delete" obscures a material distinction. Trash preserves historical recoverability; Wipe intentionally rewrites/cleans history and derived state.

**Constraint:** Wipe guarantees removal only from copies under Carta Space's control on the current device. It does not claim forensic erasure from arbitrary storage remnants, backups, clones, exports, or other devices.

---

## DD-034 — Interaction semantics are specified independently of storage format

**Decision:** `INTERACTION-CONTRACT.md` is the accepted behavioral contract for the v0.1 reference interaction model. `SPECIFICATION.md` remains authoritative for on-disk format and conformance.

**Why:** The project needs precise behavior so implementation agents do not invent UX semantics, but keyboard/UI choices must not unnecessarily contaminate the durable archive format.

---

## DD-035 — Session/UI state is local and disposable

**Decision:** Cursor/scroll positions, last View, per-Work resume positions, MRU Work ordering, and similar UI state are device-local, non-canonical, and excluded from Git history. The reference implementation keeps these files under the Carta XDG data root, `$XDG_DATA_HOME/carta` (fallback `~/.local/share/carta`), outside the `archive/` Git working tree.

**Why:** Such state improves continuity but is not authored content and should not create merge/synchronization pressure or affect archive recoverability. Keeping the default Archive and its device-local sidecar state below one deterministic XDG data root avoids hidden last-used paths while preserving the boundary between canonical Archive data and disposable UI state.

---

## DD-036 — Multi-object structural operations are atomic

**Decision:** Any user operation that changes multiple canonical objects must either complete fully or leave the previous valid Archive state intact.

**Why:** Partial Trash, restore, Work edits, or linked-document creation could otherwise leave dangling references or incoherent history after crashes or I/O failures.

**Implementation note:** The mechanism (temporary files, journaling, atomic rename, transactional layer, or equivalent) is intentionally left to the implementation.

---

## DD-037 — Work title equivalence is Unicode-defined

**Decision:** Active Work title uniqueness uses a comparison key produced by trimming leading and trailing Unicode whitespace, normalizing to NFC, and applying Unicode full case folding. NFKC is not used. The authored title is stored unchanged.

**Why:** Work title uniqueness affects archive validity and must be deterministic across implementations without replacing the user's spelling with a normalized display value.

---

## DD-038 — LEAP has a palette compatibility mode

**Decision:** Physical Left Control and physical Left Alt are the normal momentary LEAP controls when terminal event fidelity is sufficient. On terminals that cannot distinguish the required physical keys and report press/release events, the TUI exposes directional incremental LEAP and Leap Again through the command palette.

**Why:** Carta must remain usable on conventional terminal paths without compromising Right Alt/AltGr or international text entry. The fallback is deliberately degraded and does not redefine normal LEAP semantics.

---

## DD-039 — Divergent variants require explicit choice

**Decision:** Carta does not automatically merge externally divergent or ambiguously recovered Document content or Work structure. It preserves all recoverable variants until the user chooses the current variant. A non-selected Document variant may be preserved as a new Document; a second Work is not created automatically.

**Why:** Silent overwrite or speculative merge would risk authored content and structural intent. Document duplication has defined identity semantics, while automatic Work duplication would invent a new structural object without user intent.

---

## DD-040 — Work version restore is integral

**Decision:** Restoring a historical Work version is atomic and all-or-nothing. Required trashed Documents may be restored only with explicit consent in the same operation. If any required Document is unrecoverable, including after Wipe, restore fails without changing current state. v0.1 has no partial Work restore.

**Why:** A partial restore would silently change the historical Work being requested and introduce new semantics for omissions, ordering, and user intent.


---

## DD-041 — Development separates implementation from local verification

**Decision:** During the current v0.1 cycle, repository design, source changes, documentation changes, code review, and Git commits are normally performed in the design/implementation layer (ChatGPT). OpenCode is normally used as a local verification agent for builds, formatters, linters, automated tests, runtime reproduction, TUI behavior, keyboard/terminal behavior, filesystem behavior, and Distrobox-dependent checks.

**Why:** Most Carta Space changes can be specified and implemented directly from repository state without consuming a second coding agent's context. Local execution remains valuable because the reference TUI depends on a real terminal, keyboard protocol, filesystem, Git installation, and NixOS/Distrobox environment that are not available to the design/implementation layer.

**Consequence:** The default loop is user/dogfooding -> ChatGPT implementation and commit -> local fast-forward pull -> OpenCode verification without edits -> ChatGPT review and follow-up commit if needed. OpenCode may edit files only when a task explicitly grants that responsibility. This workflow does not change Carta archive semantics.


---

## DD-042 — Work color is a persistent optional presentation hint

**Decision:** A Work may store an optional `color` value in `work.json` as a stable presentation hint. The reference TUI assigns a muted `#RRGGBB` color when a Work has none and reuses that stored value across sessions.

**Why:** Dogfooding showed that a subtle stable color makes it easier to distinguish Works at a glance without adding persistent panels, icons, or other organizational chrome. Storing the choice avoids visually reassigning Works when archive order changes.

**Constraint:** Color is not Work identity or structural semantics. Frontends may ignore it, colors may repeat, and title/order/membership behavior must not depend on it. Older implementations preserve it through the existing unknown-member rule.


---

## DD-043 — v0.1.2 closes the terminal-first line; v0.2 becomes graphical

**Decision:** v0.1.2 is the final feature release of the terminal frontend. From v0.2 the canonical interactive frontend will be graphical and designed for Linux, Windows, and macOS.

**Why:** The momentary LEAP interaction depends on physical modifier identity and press/release events. Enhanced terminal protocols can expose those events in some emulators, but real TTYs and many terminal chains cannot do so portably without privileged or system-specific raw input access. Portable Keyboard Mode is useful as an emergency compatibility path but is not the intended Carta interaction.

A graphical application receives a richer native input event stream and also provides a better foundation for Unicode/IME handling, HiDPI text rendering, platform clipboard integration, themes, and later desktop portability without making those concerns part of the Archive model.

**Constraint:** "Graphical" does not mean adopting conventional editor chrome. The writing surface remains quiet; toolbars, tabs, sidebars, file browsers and persistent panels require a demonstrated need.

**Consequence:** `carta-tui` is frozen except for critical maintenance. It remains useful as a regression oracle and compatibility frontend while v0.2 is built.

---

## DD-044 — Shared interactive semantics live in carta-app

**Decision:** Introduce `carta-app` between `carta-core` and interactive frontends.

`carta-core` continues to own canonical Archive/domain behavior. `carta-app` owns frontend-independent interactive state and behavior: editing, Cat highlight/undo, session/View models, and other interaction semantics as they are cleanly extracted. Frontends own native input translation, rendering and platform integration.

**Why:** In v0.1 much of the real Carta interaction engine grew inside `carta-tui`. Copying that code into a GUI would create two divergent applications. A shared application layer lets desktop, terminal, and future web/mobile surfaces reuse the same behavior.

**Constraint:** Extraction is incremental. Code must not be moved into `carta-app` merely because it is currently in the TUI; TUI-specific commands and platform assumptions stay outside until a genuinely shared contract exists.

**Consequence:** The frozen TUI should increasingly consume `carta-app` through compatibility re-exports while preserving v0.1.2 behavior.

---

## DD-045 — Frontends translate native events into semantic actions

**Decision:** The long-term frontend boundary is semantic rather than toolkit-specific:

```text
native input -> Action -> carta-app -> state / Effect -> frontend/platform adapter
```

Physical keys such as Left Control and Left Alt are interpreted by the frontend adapter. Shared application logic receives semantic LEAP/edit/navigation actions rather than Winit, Crossterm, browser, Android, or other native event types.

Operations that require the host platform should eventually leave `carta-app` as explicit effects when a concrete need appears.

**Why:** This keeps the interaction engine independently testable and makes additional desktop, web, or mobile surfaces possible without duplicating behavior.

**Constraint:** Do not build a speculative generic platform framework. Introduce Action/Effect boundaries incrementally around real v0.2 requirements.


---

## DD-046 — Stop speculative extraction at the pre-GUI boundary

**Decision:** The preparatory extraction from the terminal frontend is complete once the canonical `App`, editor, View/AppMode state, LEAP semantics, semantic `Action` values, shared prompt/confirmation/selector `ModeAction` handling, scheduler, session model, and help content live in `carta-app`, while Crossterm state, Portable Keyboard Mode, terminal clipboard integration, terminal-only palette extensions, and rendering-dependent navigation remain in `carta-tui`.

Further abstraction must be driven by a concrete requirement from the second frontend rather than by a desire to make `carta-app` mechanically pure.

**Why:** Without a second concrete frontend, moving filesystem effects, visual navigation geometry, clipboard behavior, or other platform operations behind generic interfaces would force Carta to guess at abstractions before their requirements are known. That risks complexity, leaky interfaces, and accidental encoding of terminal assumptions into supposedly portable APIs.

The desktop GUI will provide the second implementation needed to determine which remaining concerns genuinely require shared contracts.

**Consequence:** The next architectural milestone is the desktop input/rendering prototype. Before implementing it, compare candidate input/windowing/toolkit approaches with the user, beginning with physical modifier press/release fidelity, AltGr/IME behavior, Unicode text input, portability, rendering quality, dependency weight, and integration with the existing Rust layers.


---

## DD-047 — Iced is the canonical v0.2 desktop GUI toolkit

**Decision:** Carta Space v0.2 uses Iced 0.14 for the canonical desktop GUI, with Winit-backed native window/input events and `tiny-skia` as the initial renderer.

The decision follows a dedicated isolated probe rather than API assumptions. The probe builds on Linux, macOS and Windows, and the target Linux/Wayland keyboard test confirmed all required behaviors:

- distinct physical Left Ctrl, Right Ctrl, Left Alt and Right Alt events;
- reliable DOWN and UP events for all four modifiers;
- Left Ctrl and Left Alt can therefore retain momentary LEAP semantics;
- Right Alt remains independent for AltGr;
- ordinary text, Unicode/non-ASCII input and AltGr-produced text work together;
- no stuck modifier state was observed.

IME event support remains part of the adapter contract even though no configured IME was present during the initial manual validation.

**Architecture:** Iced is a frontend only. The canonical editor, LEAP semantics, Cat selection, View/AppMode state and application behavior remain in `carta-app`. The GUI translates native Iced/Winit events into shared `Action` and `ModeAction` values and renders shared state. It must not introduce a second editor model.

**MSRV:** Iced 0.14 requires Rust 1.88 or newer. During the initial shell/prototype stage, `carta-gui` remains isolated from the main Rust 1.85 workspace so the existing v0.1 compatibility targets are not raised prematurely. Integrating `carta-gui` into the main workspace and raising the v0.2 MSRV will be a separate deliberate step once the graphical shell is stable.

**Renderer:** The first prototype used WGPU, but a real Linux desktop run failed during Iced shader creation because the active GPU/driver did not expose the shader capability required by the WGPU quad shader. Carta's surface is predominantly text and simple geometry, so GPU-specific shader requirements are not justified for the baseline frontend. The canonical renderer is therefore Iced's supported `tiny-skia` software renderer. WGPU may be reconsidered later as an optional acceleration path, but Carta must not require it to start.

**Why Iced:** It provides the best balance for Carta between implementation simplicity, runtime performance, modern desktop presentation, cross-platform support and the physical keyboard fidelity required by LEAP.


---

## DD-048 — GUI cursor states and bundled monospace font

**Decision:** The v0.2 GUI renders the Cat insertion point without injecting two character-wide Unicode squares into document text. It uses the shared application's narrow, wide and extended Cat highlight semantics, and distinguishes the saved (~3 Hz) from the dirty (~1 Hz) blink rate described by the Canon Cat Reference Manual (1987, cursor/highlight section). An existing character at point acts as the caret's painted cell; only newline/end-of-document positions need a stable blank cell. This is an intermediate rich-text projection, not a claim of pixel-perfect replication of the original raster cursor.

**Font:** Iosevka Regular is included with the graphical application under the SIL Open Font License 1.1; the GUI loads its bytes explicitly on startup rather than merely requesting a system font by family name. The font asset is text/base64 because this repository receives programmatic GitHub Contents API updates and the application decodes it in memory. The CLI and frozen TUI remain unaffected; a terminal emulator must independently select Iosevka to use identical typography.

**Rationale:** The text must never shift because of a blinking insertion cursor. The desktop graphical font must not silently fall back to an arbitrary installed font. Do not couple cursor appearance to the archive format or the shared editor's semantic state beyond the already defined Cat cursor/highlight rules.

**Reference:** Canon Cat Reference Manual, "The Cursor and the Highlight", pages 8–9, https://www.manualslib.com/manual/986126/Canon-Cat.html?page=8 .

---

## DD-049 — GUI editing viewport and nonblinking cursor

**Decision (supersedes the GUI blink cadence in DD-048):** The v0.2 desktop caret is always visible. The graphical shell no longer animates cursor blinking or redraws the full text surface at ~6 Hz while idle. It runs the application maintenance tick at a low frequency for autosave, status expiry and synchronization; keyboard, clipboard and window events drive interactive updates.

**Input:** Backward LEAP takes characters from the keyboard event's layout-aware modified logical key when native text is suppressed by Left Control. This preserves non-US layouts and Shift rather than reconstructing characters from physical key positions. Up/Down navigation uses the shared editor's visual movement method with a GUI-local column estimate. Right Control+C and Cat highlight clipboard export are executed through the Iced platform clipboard tasks, not by adding host API calls to `carta-app`.

**Viewport:** The scrolling writing surface includes synthetic vertical padding so the caret can remain around two-thirds of the visible height even at the beginning/end of a Document. The GUI translates editor byte offsets to approximate visual rows, accounting for generated separators and soft wrapping; it updates scroll position on keyboard input and resize. These presentation-only values do not change Markdown content or archive format. Exact visual metrics for tabs, multi-cell glyphs, variable shaping and mixed scripts remain an explicit limitation of the provisional rich-text renderer.

**Work View:** Generated Work separators now extend to the editor's current monospace column width rather than a fixed 40-character line. They remain UI only and must never be included in Clipboard operations or saved content.

**Validation:** Static/CI checks can verify keyboard fallbacks, cursor row projection and clipboard-text normalization, but precise position, modifier fidelity, resource use and host clipboard integration must be manually checked in the target graphical session. Preserve the frozen v0.1.2 terminal interaction semantics.

---

## DD-050 — Sustained responsiveness and GUI maintenance isolation (2026-10-08)

**Product invariant:** Carta is a continuously responsive writing environment. Low and stable input latency, fluid LEAP/navigation and negligible idle work remain release-blocking acceptance criteria even after long sessions on modest hardware. A successful compile/test is not proof of responsiveness; compare release-mode behavior at startup and after at least ten minutes on a realistic Archive.

**Observed concern:** The provisional GUI becomes sluggish after minutes. The code contained two plausible contributors, not yet demonstrated as the sole cause: an unconditional two-second GUI event tick and an automatic Git/SSH sync attempt at 180 seconds executed synchronously inside `App::tick` on the GUI thread. Each tick could reconstruct the entire rich-text widget. Software rasterization in an unoptimized debug build can compound the cost. A measurement on the affected machine is still required.

**Interim mitigation:** The GUI now uses a local-only maintenance tick, which continues automatic document autosaves and local Git checkpoints but skips automatic remote Git/SSH synchronization. Manual sync remains available and is still synchronous; the reference TUI retains its unchanged scheduler. The GUI maintenance subscription runs approximately every 15 seconds when idle, every two seconds while a transient status is shown and every second while unsaved edits are pending. Keyboard events without changes to the cursor, region count or view no longer issue a redundant scroll operation.

**Trade-off:** Automatic remote sync is temporarily unavailable in the GUI. This must be disclosed to users and must be reinstated only with safe asynchronous execution, archive/editor consistency and reliable error reporting. The remaining local checkpoint path can still block the GUI and needs measurement. Do not solve responsiveness problems by making unsaved authored text less durable or running overlapping archive mutations on background threads.

**Diagnostic strategy:** Optional `CARTA_GUI_PROFILE=1` prints content-free average/worst time for view construction, row-based scroll calculations and maintenance. It does not measure downstream glyph shaping or software rasterization. Profile the release build and verify CPU, RSS and event latency over time before introducing heavier caching or renderer changes.

---

## DD-051 — GUI Markdown colors, zoom, mouse editing and shared session state (2026-10-08)

**Scope:** The graphical v0.2 frontend can style Markdown source code, adjust the writing font, position/select through the mouse and restore the previous cursor location. The reference v0.1.2 terminal behavior and Archive format remain untouched.

**Markdown:** Reuse the same `pulldown-cmark` source-range strategy already used by the TUI. Only presentation ink changes; Markdown delimiters are never hidden, replaced or rewritten. The foreground palette recognizes headings, emphasis, strong text, links/images, quotes and inline/fenced code. A per-Document hash/cache prevents reparsing unchanged documents; the cache is disposable and is never persisted. Caret and conventional/Cat selection colors take precedence.

**Zoom:** Physical RightCtrl+Minus and RightCtrl+Equal apply 2 px decrements/increments over a constrained 12–32 px writing font; navigation columns, line heights and scroll offsets use the same size. Modal/status font sizes are separate. The zoom value is currently transient, not part of the Archive or shared session format.

**Mouse:** Iced's mouse area reports local pointer coordinates over the centered writing sheet, including scrolling translation. Those are mapped to UTF-8 byte offsets using the existing GUI single-width font/wrap approximation, then passed to `CompositeEditor::set_cursor` with conventional selection anchored by a held left button. Generated separator rows are protected from direct editing. The mapping is necessarily approximate for tabs, East Asian wide characters, grapheme clusters and renderer-specific wrapping; proper glyph-position hit testing should replace it when justified. Do not create a second independent selection/text buffer.

**Session:** On startup the GUI reads `session-<archive-id>.json` from the same `$XDG_DATA_HOME/carta` root and in the same `carta-app::Session` schema already used by the TUI. It saves on maintenance and shutdown, independently of the Archive and Git. The emergency kill-switch deliberately suppresses persistence. No new canonical metadata or synchronization responsibilities are introduced.

**Performance invariant:** Keep styling presentation-only and cache parsed source ranges, avoid additional polling loops or font-reflow animations, and verify launch-to-long-session latency in a release build. More exact hit testing or renderer changes must not regress sustained input responsiveness.

---

## DD-052 — Virtualized GUI text surface for large Views (2026-10-08)

**Problem (release blocker):** Iced's original GUI frontend constructed a single rich-text widget from the source of *every* Document in the current View on every interface update. It then performed shaping/layout/rasterization work over the full composite text. The Markdown renderer additionally hashed each Document and applied source style ranges using a linear scan. Cursor-follow calculations walked text from the beginning of the View. This work scaled with the total Archive projection instead of the small visible fraction, fundamentally violating Carta's latency invariant.

**Accepted solution:** Keep canonical source and editing semantics entirely in `carta-app`. Give the GUI a disposable, indexed *visual row projection* that records byte offsets for hard/soft wrapped rows and generated non-editable separators. The text widget receives only an overscanned, bounded window around the native scroll position. Off-screen regions occupy spacer height but are not shaped or rasterized. The row index is created for a View once and reused across navigation. Edits, undo and redo publish content revisions and invalidate only changed Documents; their row count deltas are applied to a Fenwick prefix-sum index in O(log N), so neither cursor location nor edits require scanning the other N Documents. Markdown source styles are resolved to non-overlapping, indexed runs and cached per Document generation with bounded memory. No new canonical fields, file formats or persistence are introduced.

**Interface compatibility:** Keep the native Iced scrollable/scrollbar and its absolute scroll offset. The pointer maps visible local row/column positions back to existing `CompositeEditor::Cursor` byte offsets. Preserve Ctrl/Alt LEAP semantics, conventional and Cat highlight, screen-anchored writing point, session restore, generated separators and source Markdown visibility. Font changes rebuild the disposable row index because wrapping changes; navigation alone does not. No network sync or new background polling is introduced.

**Limitations:** The mapping remains based on the current Iosevka single-cell approximation for tabs, wide/combining Unicode and complex shaping; a future renderer-measured hit tester is a separate issue. Initial View index construction scales with the size of its source text, and edits to one very large Document still need its row reindexing and syntax update. That cost is not hidden or treated as equivalent to the former large-View per-keystroke slowdown. Measure these specific cases separately before considering more complex indexing.

**Validation:** Unit tests protect row boundaries, Fenwick prefix/search/update, and window-size independence for a synthetic View of 20,000 Documents. Build/test/lint checks and long-running real GUI dogfooding are both required before claiming the regression fixed. A passing build is not evidence of sustained fluidity.

---

## DD-053 — Automatic remote synchronization is an Archive service (2026-10-08)

**Requirement:** Remote push and pull must be automatic and must never perform SSH or Git network operations on the editor/GUI event loop. Temporary suspension of automatic GUI synchronization was a performance workaround, not an acceptable feature state. The canonical Archive and its history remain the only persistence authority; frontends may schedule work but do not implement separate merge semantics.

**Implementation:** The frontend-independent service in `carta-core::background_sync` operates in two phases. The worker receives an Archive filesystem path after the editor has saved and checkpointed its in-memory changes. In a blocking runtime worker it checks the configured `carta-sync` remote and the pinned Git HEAD: when already synchronized it returns immediately; otherwise it attempts a non-force push of the *pinned commit*. When a pull/merge is required, the worker creates an owner-private temporary clone of the locally committed Archive, invokes the existing `Archive::sync` there and returns a retained staging object. It does **not** run a network operation on the GUI event loop or mutate the live worktree.

**Validation / integration:** The GUI retains the staging object until the editor is idle. The shared `App` verifies no dirty editor/provisional Document, checks the live Archive is clean, that the remote configuration matches and that HEAD still equals the snapshot base. Only then may the core import objects locally and update the working tree, followed by `Archive::refresh` and restoration of the current Document/cursor. If local writing or checkpointing advanced since staging, the old snapshot is discarded and a new sync is scheduled. Non-force pushes and the pre-apply HEAD comparison prevent loss of either machine's committed changes. If a remote merge conflicts, neither side is reset and explicit conflict resolution remains necessary.

**Cadence:** Initial sync, immediate queued sync following a checkpoint/manual `Sync Now`, and periodic checks (180 s); worker failures back off, protecting offline operation. Standard TUI timing remains unchanged; the TUI can adopt the same shared service later.

**Remaining performance limit:** The *network* portion is fully asynchronous and common cycles avoid copying the Archive. Checkpoint preparation and short, network-free final integration still run on the active application thread after a two-second quiet period, and can take noticeable time on extremely large Archives. This is not a claim of absolute zero-latency I/O. Track these costs separately and do not regress editing responsiveness; if necessary, move final import/integration to a coordinated storage actor rather than allowing concurrent writes to the live Archive.

**Safety tests:** Exercise initial publish, no-op, remote-only update, stale local HEAD, uncheckpointed local edits, delayed network failure and divergent changes using synthetic Archives. Long-lived physical GUI testing remains required.

---

## DD-054 — Existing upstream and nonblocking desktop synchronization

**Decision:** The v0.2 GUI explicitly adopts an existing `origin/carta` upstream
when no dedicated `carta-sync` remote exists. Adoption copies the ordered origin
URLs and configured push URLs into device-local synchronization configuration,
without network access, altering authored data or enabling unrelated remotes. An
explicit disable flag prevents adoption on later opens. The dedicated remote
retains precedence. This behavior was approved with the warning that Carta
does not encrypt remote text, metadata or retained history.

**Safety:** Device-local remote changes use a locked atomic configuration
transaction, so failed setup cannot silently replace the publication destination.
Each worker resolves and pins effective fetch/push endpoints, including Git URL
rewrites and relative local paths. A private transport snapshots authentication
settings without retaining routing rewrites. Validate every nonempty destination
against the committed Archive identity before any push. Known-object cycles
avoid cloning the Archive; unknown objects and incoming/divergent histories use
the private staging path. No worker writes the live worktree or publishes a
moving HEAD. Partial multi-destination publication is safely retryable, not an
atomic distributed transaction.

**Warnings:** Synchronization setup, transport and conflict errors are non-modal
status warnings. They never prevent opening, writing or local checkpoints.
Unpublished checkpoints remain queued with bounded retry backoff. Outbound-only
success is acknowledged while writing; only incoming integration waits for a
quiet, clean editor and the existing configuration/HEAD guards.

**Quit:** Disable the toolkit's automatic window-close exit and route window
close through shared Quit autosave/checkpoint. After local durability succeeds,
exit without waiting for the network. Startup automatically retries unpublished
history, including the final checkpoint. A closed window is not a promise of
remote publication. Network jobs use detached threads with async result channels
so toolkit runtime shutdown does not join a blocked Git/SSH worker.

**Scope:** The requested nonblocking warning behavior replaces the earlier
local proposal to keep Quit open until final remote publication. It does not
change the Archive format, frozen TUI interaction or emergency termination.
Git network work never runs in Drop or the event loop. This change introduces
no network deadline or child-process cancellation policy: a stalled transfer
can delay further sync attempts, but not writing or Quit. Runtime responsiveness
and physical window/keyboard testing remain separate from synthetic tests.


## DD-055 — Optional, on-demand spelling without a second editor model (v0.2)

**Decision:** spelling is an optional frontend-independent operation exposed
through `carta-app` command palette, backed by the installed Hunspell binary.
Each Document has an optional language in `meta.json`; a Work uses each
member Document's language and never copies member content. Personal additions
are portable canonical text lists in the Archive and travel through Git. The
existing editor performs all replacements, preserving Undo/Redo and locking.
No network model, grammar engine, live underlining, background process or
new language-specific metadata format is introduced.

**Tradeoff:** Hunspell and base language dictionaries must be installed on
each computer. Missing backends must never block writing. Concurrent additions
use Git union merge for personal lists; this is not a general conflict strategy
for arbitrary files. Avoid using union semantics to infer deletion intent.

## DD-056 — On-demand, device-local dictionary distribution (v0.2)

**Decision:** Keep the canonical Document language metadata and Archive personal
word lists unchanged. Provide a frontend-independent asynchronous dictionary
manager in `carta-app`, using an immutable public dictionary snapshot and
verification against pinned Git blob identifiers. Install the original license
next to each system dictionary in a disposable device-local cache. Prefer an
already installed Hunspell dictionary to an unnecessary download. Never fetch
at startup. A missing language is retrieved after explicit language selection
or a spelling command; a pending check resumes when download completes.

**Constraints:** Do not access the Archive from downloader workers, do not put
the downloaded corpus in Git, do not run network transfers on the UI thread,
and do not interpret a missing runtime executable as a missing dictionary.
External Hunspell remains a package/runtime requirement for this version.
