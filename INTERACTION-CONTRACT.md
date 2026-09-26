# Carta Space — v0.1 Interaction Contract

> **Status:** accepted interaction contract for the first usable Carta Space frontend.
>
> This document specifies user-visible behavior for v0.1. It complements `SPECIFICATION.md`, which remains authoritative for the archive format. Frontends may differ visually, but they MUST NOT silently invent conflicting interaction semantics.

Carta Space v0.1 is writing-first. The normal path is: open, continue writing, retrieve or compose when necessary, and publish only when necessary.

The guiding rules are:

> **During writing, only writing decisions should normally be visible.**

> **Power should be available on demand, not presented by default.**

---

## 1. Startup, archives, and session state

### 1.1 Resume exactly where the user stopped

On normal startup, Carta SHOULD restore the previous working context as closely as possible:

- the same View or Work;
- the same current Document;
- the same cursor position;
- the same scroll position when practical.

Carta MUST NOT normally show a home screen before resuming work.

If an Archive has no previous session, Carta creates a new provisional Document in the current monthly Volume and places the cursor ready for writing.

### 1.2 One Archive at a time

A Carta process opens one Archive at a time.

If Carta is launched without an explicit path, it SHOULD reopen the last Archive used on that device. If no Archive is known, the frontend may offer only the minimal choices needed to create or open one.

`Create Archive…` asks only for a name/location, initializes the archive and Git history, then opens a new provisional Document. There is no setup wizard.

### 1.3 Device-local UI state

Cursor positions, scroll positions, the last View, per-Work resume positions, most-recently-used ordering, palette state, and similar UI conveniences are device-local, disposable, non-canonical state.

They MUST NOT be required to reconstruct authored content or Work structure and SHOULD NOT be versioned in the Archive Git history.

---

## 2. Documents

### 2.1 New Document

`New Document` creates a new Document immediately:

- new UUID;
- `created` = current time;
- Volume = current local month;
- no title prompt;
- no filename prompt;
- no tags, properties, templates, or classification prompt.

Retrodated creation is not supported in v0.1.

When invoked inside a Work View, `New Document` also inserts the new Document immediately after the current Document in that Work.

When invoked outside a Work View, it creates a neutral Document with no Work membership and takes the user to the current month's Chronological View.

### 2.2 Provisional empty Documents

A newly created Document remains provisional until it contains at least one non-whitespace character.

If a provisional Document is abandoned while still empty, Carta MAY silently discard it.

If it was provisionally inserted into a Work, that provisional Work reference is discarded with it.

A Document that once contained non-whitespace content is persistent even if the user later edits it back to empty. It then disappears only through `Trash`.

### 2.3 No separate Document title

Draft v0.1 has no separate Document title field.

For human-facing labels:

1. if the first non-empty line is a Markdown heading, use the heading text;
2. otherwise use the first non-empty line;
3. if the Document is empty, use creation date/time.

Later headings are internal document structure, not the Document title.

### 2.4 No split or merge in v0.1

v0.1 does not expose Document split or merge operations.

Document boundaries are intentionally stable. If later use demonstrates a real need, split/merge or extraction semantics may be designed separately.

### 2.5 Duplicate as New

`Duplicate as New` creates a new independent Document:

- new UUID;
- current creation time;
- current Volume;
- exact current Markdown content copied from the source;
- no inherited Work memberships.

The source Document is unchanged.

### 2.6 New Linked Document

`New Linked Document`:

1. creates a new neutral Document in the current Volume;
2. inserts an ordinary `carta:doc:` Markdown link to it at the source cursor position;
3. opens the new Document.

The default visible link text follows the interface locale. In the English v0.1 interface the default is `Continue in…`.

The label remains ordinary editable Markdown. No special continuation relation is stored outside the link.

A linked Document does not automatically inherit the current Work, even when the command is invoked from a Work View.

---

## 3. Views

### 3.1 v0.1 View set

v0.1 implements these primary Views:

- Chronological View;
- Work View;
- Search Results View.

History and Trash are specialized read-only interfaces/views.

There is no Single Document View in v0.1. A Document is normally reached in either its chronological context or a Work context.

### 3.2 Editability

- Chronological View: editable.
- Work View: editable.
- Search Results View: not directly editable.
- History: read-only except explicit restore/duplication operations.
- Trash: read-only except explicit restore/wipe operations.

When a target Document is already contained by the current editable View, navigation SHOULD preserve that View and jump to the target.

Otherwise a Document target opens in the Chronological View of its Volume.

### 3.3 Chronological View

The natural scope is one monthly Volume.

By default Carta opens the current month, unless local session state records a different recent chronological position.

Documents appear from oldest to newest by `created`.

The user can move to adjacent months or use a command such as `Go to Month…`.

### 3.4 Work View

A Work View presents its component Documents in explicit Work order as a continuous editable surface.

Each Work remembers its last local cursor/scroll position when possible.

### 3.5 Document boundaries

In multi-Document editable Views, boundaries are generated UI rather than authored text. A Work View uses one generated blank line, a simple continuous separator line, and one generated blank line between Documents.

A Chronological View gives every Document a generated metadata separator. The separator includes the Document creation date and up to two Work memberships; if more memberships exist, an ellipsis indicates the remainder. The date uses a compact human-readable form such as `ven 25 set 2026`. A blank generated row separates the metadata line from authored text and from the preceding Document.

The separator is generated UI:

- not part of Markdown;
- not selectable as authored text;
- not copied to the clipboard;
- not deletable by ordinary editing.

The cursor may cross boundaries.

Selections may cross boundaries for copying, but destructive editing MUST NOT cross them.

### 3.6 Status bar

The TUI uses a minimal one-line status bar.

Examples:

- Chronological: `2026-09-25 · Document label · September 2026 · 7/18`
- Work: `2026-09-25 · Document label · Romanzo · 3/12`
- Search: `Search: bernanos · 4 results`
- History: `History · Document label · 2026-09-25 14:20`

The status bar follows the Document under the cursor.

Chronological View keeps the neutral gray status bar and shows the active Document's Work memberships at the right edge, up to two names plus an ellipsis when more exist.

Work View uses the Work's persistent muted accent color as the status-bar background. The frontend chooses light or dark foreground text according to contrast. Colors may repeat across Works; their purpose is rapid visual distinction, not identity.

Color MUST NOT be the only indicator of context.

UUIDs, filesystem paths, Git state, and similar implementation details do not belong in the normal status bar.

---

## 4. LEAP

### 4.1 Bindings

The reference TUI uses:

- physical Left Control = LEAP backward;
- physical Left Alt = LEAP forward.

Physical Right Alt/AltGr is never a LEAP key and remains available for international text entry. Physical Right Control is not a LEAP key.

These are frontend bindings, not archive semantics.

If the terminal cannot distinguish the required physical modifier keys and their press/release events, the TUI remains usable in a degraded compatibility mode. Left Alt is used only when it is reported distinctly from Right Alt/AltGr; Carta MUST NOT treat Right Alt/AltGr as LEAP. The command palette instead exposes:

- `LEAP Forward…`;
- `LEAP Backward…`;
- `Leap Again Forward`;
- `Leap Again Backward`.

`LEAP Forward…` and `LEAP Backward…` provide a transient incremental query using the normal LEAP matching semantics and end on explicit confirmation or cancellation. This compatibility mode does not change normal momentary LEAP semantics on capable terminals.

### 4.2 Scope

Normal LEAP searches the current View.

It does not expose a runtime scope selector. Whole-Archive search is a separate command.

### 4.3 Behavior

LEAP is momentary/quasimodal and incremental:

- while a LEAP key is held, typed characters extend the query;
- Backspace shortens the query;
- matching is case-insensitive;
- the result updates immediately;
- search wraps circularly within the current View;
- wrap SHOULD receive subtle feedback;
- releasing the LEAP key ends the operation; backward LEAP leaves the cursor at the start of the match, while forward LEAP leaves it immediately after the match;
- if no match exists, Carta does not leave the original position;
- the active query may be shown transiently and disappears when LEAP ends.

A single match MUST be entirely within one Document. LEAP may navigate across Document boundaries, but the query cannot match text formed by concatenating the end of one Document and the beginning of another.

### 4.4 Leap Again

Carta remembers the last LEAP query for the session.

A tap of Left Control with no new query performs `Leap Again Backward`.

A tap of Left Alt with no new query performs `Leap Again Forward`.

The remembered query is session-global: after changing Views, Leap Again searches the new current View using the same query.

### 4.5 Selection interaction

In v0.1, starting LEAP cancels the current selection and performs navigation only.

Future frontends may experiment with Cat-style selection + LEAP behavior without changing the archive model.

---

## 5. Command palette and commands

### 5.1 Palette

`Esc` opens the command palette.

While the palette or a child selector is open, `Esc` cancels and returns to the text.

The palette is transient and dmenu-like:

- typing immediately filters commands;
- matching is case-insensitive;
- matching uses simple tokens/substrings rather than aggressive fuzzy ranking;
- ordering is deterministic;
- Enter executes the selected command.

The palette is contextual. Commands that cannot apply in the current context SHOULD be hidden rather than shown disabled.

Editable Views expose `Insert Current Date and Time`, which inserts the current local date and time at the cursor as `YYYY-MM-DD HH:MM`. History and Work History expose `Return to Previous View`, which returns to the View from which history was opened.

### 5.2 Direct shortcuts

The v0.1 TUI keeps direct bindings deliberately small:

- Left Control: LEAP backward;
- Left Alt: LEAP forward;
- Left Control+Enter: beginning of the current visual line;
- Left Alt+Enter: end of the current visual line;
- Esc: command palette;
- Ctrl+Shift+Left/Right: extend the selection to the previous/next word boundary;
- Ctrl+PageUp/PageDown: move to the previous/next Document in the current View, without wrapping;
- Ctrl+Home/End: move to the beginning/end of the current Document.

Explicitly assigned Ctrl chords take precedence over the physical Control-key LEAP binding. They MUST NOT start LEAP, leave LEAP pending, or change the remembered LEAP query.

Other direct shortcuts should be added only after real use demonstrates a need.

`Undo` and `Redo` are contextual command-palette commands. They are available in editable Views when the session-local editor history can apply them.

---

## 6. Editing

### 6.1 Selection

v0.1 uses conventional contemporary text selection, including Shift plus cursor movement and optional mouse selection where the frontend supports it.

LEAP is not the primary selection mechanism in v0.1.

### 6.2 Copy, Cut, Paste

v0.1 does not reserve direct Copy, Cut, or Paste bindings. The terminal emulator remains responsible for its normal clipboard chords and inserts pasted text as ordinary literal text.

A future Carta clipboard, if introduced, will use plain Unicode/Markdown text rather than a Carta-specific serialized object. A selection may cross Document boundaries for copying, but destructive editing MUST NOT cross them.

### 6.3 Undo and Redo

Undo/Redo are session-local editing facilities.

They MAY include recent structural actions such as Work membership/order changes, but they are not an interface to Git history.

Undo/Redo state may be lost when Carta exits.

Historical recovery across sessions belongs to History.

### 6.4 Markdown editing

v0.1 edits Markdown source directly. There is no WYSIWYG editing mode.

Future frontends may render headings, emphasis, links, or other syntax more richly without changing canonical Markdown.

### 6.5 Wrapping and indentation

Soft wrap is visual only. Carta does not automatically hard-wrap authored lines. In editable Chronological and Work Views, the writing surface is centered and at most 80 terminal columns wide, or the available width when the terminal is narrower. Cursor movement and wrapping use that same visual width.

`Tab` inserts four spaces.

`Shift+Tab` removes up to four leading spaces where appropriate.

Carta does not insert literal TAB characters for indentation in v0.1.

When Enter is pressed on a Markdown list item, Carta continues the list on the new line. Unordered markers `-`, `*`, and `+` are preserved together with indentation. Ordered markers using `.` or `)` are incremented while preserving indentation. This is an editing convenience only: the canonical Document remains ordinary CommonMark text.

---

## 7. Works

### 7.1 Creation and naming

`Create Work` asks for one explicit Work name and creates an empty Work.

It does not automatically add the current Document.

Active Work names are unique using the comparison key defined by `SPECIFICATION.md`: trim leading/trailing Unicode whitespace, normalize to NFC, then apply Unicode full case folding. The authored name is preserved exactly, and NFKC is not used.

A Work in Trash does not reserve its former name.

`Rename Work` changes only the human-facing Work name; identity and membership remain unchanged.

### 7.2 Work membership

A Document may belong to multiple Works.

The same Document may appear at most once in a given Work.

Editing a shared Document changes the single canonical Document and therefore changes what every containing Work displays. Carta does not interrupt ordinary editing to warn about this.

`Show Memberships` reveals the Works that reference the current Document.

Use `Duplicate as New` when an independent variant is required.

### 7.3 Add and remove

`Add to Work…` opens a dmenu-like Work selector and appends the current Document to the selected Work. If the typed name matches no selectable Work, Enter creates a new Work with that name and immediately adds the current Document. After either operation, Carta opens that Work View with the added Document current.

Works that already contain the Document are omitted. Carta never creates a second active Work with an equivalent title merely because the current Document is already a member.

`Remove from Work` removes only the Work reference. The Document remains in the Archive.

Removal is immediate, requires no confirmation, and may be undone in the current session.

Works may be empty.

### 7.4 Reordering

v0.1 provides:

- `Move Document Earlier`;
- `Move Document Later`;
- `Move This Document After…`.

The last command opens an ordered selector containing `[Beginning of Work]` followed by the other Documents in current Work order. Choosing an entry moves the current Document to that position.

### 7.5 Open Work

`Open Work…` is dmenu-like and filters by Work name.

The initial list is ordered by most recent access/use, using disposable device-local state rather than canonical metadata.

### 7.6 Work color

Each Work may carry a persistent `color` presentation hint in `work.json`.

The reference TUI automatically assigns a muted color when a Work has none and saves that choice so the same Work keeps the same visual accent across sessions. It prefers underused colors from a small palette but does not require global uniqueness.

---

## 8. Links and navigation

### 8.1 Insert Link

Users should not need to type Carta UUIDs.

`Insert Link…` opens a dmenu-like selector over Documents and Works and inserts an ordinary CommonMark link using `carta:doc:` or `carta:work:`.

If text is selected, it becomes the visible link label. Otherwise Carta uses the Document's derived label or the Work name.

### 8.2 Open Link

When the cursor is inside a Markdown link, the contextual palette exposes `Open Link`.

For Carta links:

- a target already in the current View is opened in that View;
- another Document opens in its Volume's Chronological View;
- a Work opens in its Work View at the locally remembered position.

For external links, v0.1 may delegate `http`, `https`, and `mailto` to the operating system. Unknown URI schemes are not executed automatically.

### 8.3 Navigation history

Following links and opening search results pushes a volatile navigation history.

`Back` and `Forward` return to prior navigation positions, including prior Views and scroll/cursor positions when practical.

This is unrelated to Git History.

### 8.4 Broken links

Carta does not silently rewrite or guess replacements for broken links.

If a Carta target is in Trash and recoverable, Carta may offer `Restore from History`.

If the target has been Wiped, the link remains unresolved.

### 8.5 Backlinks

Backlinks are derived.

`Show Backlinks` is available in v0.1 and reuses Search Results View rather than introducing a separate persistent panel or View type.

No backlink list is canonical metadata.

---

## 9. Search Archive

`Search Archive` searches only active Document content.

It does not search Trash, History, or Work names.

v0.1 search is:

- literal;
- case-insensitive;
- single-line query;
- leading/trailing query whitespace ignored;
- internal query whitespace significant;
- no regex;
- no wildcard language;
- no fuzzy matching;
- no Boolean operators;
- no semantic ranking.

A match cannot cross Document boundaries.

Results are grouped by Document, with one result entry per Document, showing label, date, and useful context. Documents are ordered newest first.

Opening a result navigates to the matching occurrence in the target Document's Chronological View.

The Search Results View remains in navigation history, so `Back` returns to the same result-list position.

---

## 10. History and checkpoints

### 10.1 Autosave

Carta has no ordinary `Save` command.

Authored text is autosaved after approximately one second of inactivity.

A crash after autosave but before the next Git checkpoint MUST NOT lose the autosaved text.

### 10.2 Automatic checkpoints

If changes exist, Carta creates automatic Git checkpoints approximately every ten minutes.

It also creates immediate checkpoints after important structural operations and on normal Quit.

Simply changing View or Document does not itself create a checkpoint.

### 10.3 Create Checkpoint

`Create Checkpoint` forces an immediate checkpoint and MAY include a short optional human note.

Automatic checkpoints remain present in Git history but MAY be visually grouped in History. Manual annotated checkpoints and structural checkpoints remain prominent.

### 10.4 Document History

Document History shows historical revisions read-only.

Available actions include:

- `Restore This Version`: makes that historical content the current content of the same UUID and creates a new checkpoint without deleting later history;
- `Restore as New Document`: creates a new Document now, in the current Volume, with a new UUID and the historical content, with no Work memberships.

If restoring the same Document affects multiple Works because the Document is shared, Carta warns before applying the restore.

### 10.5 Work History

A historical Work state represents the whole Work at that checkpoint:

- Work name;
- Work order;
- membership;
- the historical content of the member Documents at that checkpoint.

`Restore Work Version` restores that historical Work state as a new current state without deleting later history.

Documents currently in the Work but absent from the historical state are removed from the Work, not deleted from the Archive.

If the restore changes Documents also used by other Works, Carta warns about those effects before confirmation.

`Restore Work Version` is integral and atomic. If required historical Documents are in Trash, Carta may restore them only with explicit user consent as part of the same operation. If a required Document was Wiped or is otherwise unrecoverable, the operation fails without changing current state and identifies the Document that prevents complete restoration. v0.1 does not provide partial Work restore.

---

## 11. Trash and Wipe

### 11.1 Trash Document

The user-facing command is `Trash`, not `Delete`.

Trashing a Document removes it from the active Archive working state while keeping it recoverable from Git History.

Before confirmation Carta:

- lists every Work that currently contains the Document;
- reports inbound Carta links that will become unresolved;
- asks the user to approve removal of the Work references.

Trash uses a single danger-styled `y/N` confirmation. `y` confirms; `n`, Enter, or Esc cancels. It does not require typing a confirmation phrase.

The operation removes Work references and the active Document atomically.

Authored Markdown links from other Documents are never rewritten automatically. They remain as broken links until the target is restored or the links are edited by the user.

### 11.2 Show Trash

`Show Trash` lists trashed Documents and Works.

The UI may use a disposable derived Trash index. There is no canonical `trash/` storage hierarchy in v0.1.

### 11.3 Restore Document

Restoring a trashed Document restores:

- the same UUID;
- the same `created`;
- the same original Volume;
- the content immediately before Trash.

Old Work memberships are not restored automatically.

### 11.4 Wipe Document

`Wipe permanently` is available only for Documents already in Trash.

It requires a strong explicit confirmation.

On the current device, Carta MUST remove every recoverable copy under Carta's control, including:

- current working-tree copies;
- retained Git history/references containing the Document;
- relevant reflog/unreachable Git objects after history rewrite and cleanup;
- derived search/link/semantic indexes;
- Carta-managed caches;
- Carta-managed autosave/temporary/session remnants containing the content.

After successful Wipe, Carta itself must not be able to recover the Document from that device.

This guarantee does not extend to external backups, clones, exports, filesystem/storage snapshots, or physical flash remnants outside Carta's control.

### 11.5 Trash Work

`Trash Work` removes the Work from the active Archive while leaving all member Documents unchanged.

It uses the same danger-styled `y/N` confirmation as Document Trash.

Links to the Work are not rewritten and become temporarily broken.

There is no `Wipe Work` command in v0.1.

### 11.6 Restore Work

Restoring a Work restores the same Work identity, name, and order where possible.

It does not silently restore missing Documents.

If required Documents are in Trash, Carta may offer their restoration explicitly.

If the restored Work name conflicts with an active Work name, the user must choose a new unique name before restoration completes.

---

## 12. Import and export

### 12.1 Import

v0.1 has one contextual `Import…` command.

Supported file types are:

- Markdown (`.md`);
- plain text (`.txt`).

An imported file becomes a new neutral Document:

- new UUID;
- current creation time;
- current Volume;
- no implicit Work membership.

The source filename is not inserted as a heading or title.

Import does not perform deduplication.

Input must be valid UTF-8; UTF-8 BOM is accepted and stripped. CRLF is normalized to LF. v0.1 does not guess legacy encodings.

### 12.2 Export

v0.1 provides:

- `Export Document as Markdown`;
- `Export Document as PDF`;
- `Export Work as Markdown`;
- `Export Work as PDF`.

Work export commands are visible only in a Work context.

Markdown export contains authored text, not Carta UUIDs or administrative metadata.

Work Markdown export concatenates component Documents in Work order without UI boundary separators.

PDF export is a derived publishing operation implemented through Pandoc and an external PDF engine; LuaLaTeX is the initial preferred default. Carta v0.1 does not implement its own layout engine.

Carta proposes a filename from the Document label or Work name and allows the user to change path/name before export.

Exports live outside the Archive.

---

## 13. Text format and assets

Canonical Document text is UTF-8 with LF (`\n`) line endings.

Carta-managed v0.1 content is text only.

Markdown references to images or files are preserved literally, but Carta v0.1 does not import, copy, index, package, or otherwise manage assets.

---

## 14. External edits

Carta's canonical Markdown and JSON files remain user-owned and externally editable.

### 14.1 Document body changed externally

If Carta has no local unsaved/divergent edit for the affected Document, it may reload the external change automatically.

If both Carta's loaded state and the external file changed, Carta MUST NOT silently overwrite either version. Both must be preserved until explicit resolution.

Resolution does not merge automatically. The user explicitly chooses which variant continues as the current content of the original Document UUID. The user may also preserve the other variant as a new Document with a new UUID. No recoverable variant may be overwritten before that decision.

### 14.2 Work structure changed externally

If `work.json` changes externally and Carta has no concurrent local structural change, Carta may reload it.

If both external and internal Work structure changed, v0.1 does not perform an automatic structural merge. Both versions must be preserved for explicit resolution.

The user explicitly chooses which preserved structure becomes current for the existing Work identity. Carta does not automatically create a second Work.

---

## 15. Validation, crash recovery, and atomicity

### 15.1 Startup validation

Carta performs a fast, non-invasive validation when opening an Archive.

Missing/corrupt disposable derived state may be rebuilt automatically.

Canonical authored content or structural data MUST NOT be rewritten as "repair" without explicit user consent.

### 15.2 Crash recovery

After an unexpected termination, Carta attempts to reopen the previous session using the latest safely autosaved content.

If the Archive is coherent, recovery should be effectively invisible apart from optional subtle notice.

If ambiguity or corruption exists, Carta preserves recoverable alternatives and asks for explicit resolution rather than silently choosing one.

When ambiguous recovery presents competing Document contents or Work structures, it follows the same explicit-resolution rules as external-edit conflicts. No automatic merge is performed and no recoverable alternative is overwritten before the decision.

### 15.3 Structural atomicity

Any operation that changes multiple canonical objects MUST behave atomically from the user's perspective.

It must either:

- complete all intended changes; or
- leave the Archive in its previous valid state.

This requirement applies in particular to Trash, Restore operations, multi-object Work changes, and New Linked Document.

The implementation mechanism is deliberately unspecified.

---

## 16. Deferred features

The following are explicitly outside v0.1 interaction scope:

- synchronization;
- collaboration;
- semantic/vector search;
- saved Views;
- managed assets;
- bibliography;
- tags/taxonomies;
- AI-first workflows;
- Cat-style selection+LEAP editing;
- WYSIWYG editing;
- Document split/merge;
- Work Wipe;
- advanced import formats.

These may be revisited only when real use demonstrates a need.
