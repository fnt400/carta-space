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
