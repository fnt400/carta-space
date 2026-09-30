#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpKind {
    Cheatsheet,
    Manual,
}

#[derive(Debug, Clone, Copy)]
pub struct HelpDocument {
    pub title: &'static str,
    pub body: &'static str,
}

const CHEATSHEET: &[HelpDocument] = &[HelpDocument {
    title: "Carta Space — Cheatsheet",
    body: r#"ESSENTIAL

Esc                         Open command palette
Left Ctrl                   LEAP backward / tap = creep backward
Left Alt                    LEAP forward / tap = creep forward
Right Ctrl                  Carta command modifier
Right Ctrl + Z              Undo
Right Ctrl + R              Redo
Right Ctrl + C              Cat COPY when highlighted; otherwise paste system clipboard
Right Ctrl + N              New Document
Right Ctrl + G              Emergency kill switch (no final save/checkpoint/sync)
Right Ctrl + B              Insert Markdown bold markers (**|**)
Right Ctrl + I              Insert Markdown italic markers (*|*)
Right Ctrl + W              Open Work
Right Ctrl + L              Insert Link; Open Link when point is on a link
Left Ctrl + Enter           Add LF to backward LEAP pattern
Left Alt + Enter            Add LF to forward LEAP pattern
Left Ctrl + Home            LEAP to start of current Document
Left Alt + End              LEAP to end of current Document
Left Ctrl + PageUp          LEAP to start of previous Document
Left Alt + PageDown         LEAP to start of next Document
Right Ctrl + Left Alt       Leap Again forward
Right Ctrl + Left Ctrl      Leap Again backward
Portable Keyboard Mode      Enable/disable from the command palette
[PORTABLE]                  Shown in the status bar while portable mode is active
Ctrl+P (portable mode)      Portable Esc: palette/cancel
Ctrl+B (portable mode)      Start sticky LEAP backward; Enter confirms
Ctrl+F (portable mode)      Start sticky LEAP forward; Enter confirms
Ctrl+R (portable mode)      Leap Again in the last portable LEAP direction

CAT HIGHLIGHT

Hold Left Ctrl + Left Alt to extend the Cat highlight.
The final target character is included.
Backspace or Delete erases the highlighted block.
LEAP outside a highlight moves the highlighted block.
LEAP inside it collapses the highlight and allows rehighlighting.

DOCUMENT NAVIGATION

Right Ctrl + PageUp         Previous Document
Right Ctrl + PageDown       Next Document
Right Ctrl + Home           Start of current Document
Right Ctrl + End            End of current Document
PageUp / PageDown           Move by one screen
Home / End                  Start/end of visual line

PALETTE COMMANDS

New Document
Split Document at Point
Duplicate as New
New Linked Document
Collapse View / Expand View
Open Work / Add to Work / Remove from Work
Lock/Unlock Document / Lock/Unlock Work
Search Archive
Go to Start of View / Go to End of View
Document History / Work History
Trash / Show Trash
Export Document or Work
Sync Now / Sync Settings
Cheatsheet / Manual

Esc leaves Cheatsheet/Manual and returns to the previous View.
Right Alt / AltGr is never a LEAP key."#,
}];

const MANUAL: &[HelpDocument] = &[
    HelpDocument {
        title: "1. Writing model",
        body: r#"Carta Space is a writing-first environment. The normal surface is a continuous View of Documents, not a file manager and not a page-layout application.

Authored text is Markdown. Structure is expressed semantically rather than by direct typography. Document boundaries, Work membership, history, and other structural information are not encoded as decorative Markdown.

The TUI keeps the Markdown source visible but highlights CommonMark structure such as headings, emphasis, strong emphasis, quotations, code, and links. Highlighting is presentation only and never changes authored text.

The interface deliberately keeps technology quiet. The command palette exposes power on demand; ordinary writing should remain visually sparse."#,
    },
    HelpDocument {
        title: "2. Documents and boundaries",
        body: r#"A Document is the basic authored unit. It has an immutable UUID and creation timestamp but no separate title field. Its visible label comes from the first heading or first non-empty line.

Creation Date, Modification Date, and Work Views concatenate several Documents. Their boundaries are generated UI: they are not Markdown, cannot be selected as authored text, and cannot be erased by normal editing.

Split Document at Point is the explicit structural exception. Text before the cursor remains in the original Document; text from the cursor onward becomes a new Document with a new UUID and a timestamp immediately after the original. Every Work containing the source inserts the new Document immediately after it."#,
    },
    HelpDocument {
        title: "3. Views and navigation",
        body: r#"Creation Date View presents one monthly Volume in creation order. Modification Date View presents all Documents in the Archive, ordered from most recently modified to least recently modified. Work View presents the Documents of one Work in explicit Work order.

The editor normally keeps the cursor about two thirds of the way down the screen. Blank screen space is allowed above or below the available text; this is presentation only and never changes Document content.

Collapse View turns a Creation Date, Modification Date, or Work View into a read-only navigation overview showing only the first three visual rows of each Document. Up/PageUp and Down/PageDown move between Documents. Enter, or Expand View from the palette, returns to normal editing on the selected Document."#,
    },
    HelpDocument {
        title: "4. LEAP",
        body: r#"Physical Left Control is LEAP backward and physical Left Alt is LEAP forward. Hold the key and type an incremental search pattern.

A lowercase query character matches either case. An uppercase query character requires uppercase. Shift and AltGr may be pressed before or after the LEAP key without terminating the session.

A successful LEAP lands on the target character. A failed LEAP rebounds to its origin. Releasing a LEAP key after no query performs creep in that direction.

Enter while a physical LEAP is held is normal pattern input for the authored LF character. Enter+Enter therefore searches two consecutive LF characters, and Enter followed by ordinary characters searches that mixed pattern. LF searches use the normal current-View scope and wrap behavior; Document boundaries do not create synthetic LF characters. The Cat cursor logically rests on the matched LF, without any extra LF marker in the TUI.

With a Cat highlight active, the text remains in place while the LEAP query and Leap Again choose a destination; the move occurs only when the physical LEAP key is released.

LeftCtrl+Home and LeftAlt+End LEAP to the beginning and end of the current Document. LeftCtrl+PageUp LEAPs to the beginning of the previous Document; LeftAlt+PageDown LEAPs to the beginning of the next Document."#,
    },
    HelpDocument {
        title: "5. Leap Again and Cat highlight",
        body: r#"RightCtrl+LeftAlt repeats the last LEAP forward; RightCtrl+LeftCtrl repeats it backward.

During an active physical LEAP, pressing Right Control performs Leap Again without changing the original LEAP origin. This applies to text patterns, including patterns containing LF entered with Enter, and to structural LEAPs using Home/End or PageUp/PageDown. For PageUp/PageDown, repeated Right Control continues through successive Document starts. Pressing the opposite LEAP key while the first remains held highlights the interval from the original origin through the final reachable target.

After releasing the LEAP key, RightCtrl+LeftAlt or RightCtrl+LeftCtrl repeats the most recently used text or structural LEAP in the requested direction. A structural LEAP replaces an older text pattern for Leap Again.

Holding both LEAP keys extends the Cat highlight. The final target character is included. Cat highlights never cross a Document boundary."#,
    },
    HelpDocument {
        title: "6. Editing, ERASE, copy and paste",
        body: r#"Backspace is intentionally modern and always erases backward. Delete represents Cat ERASE: after typing it erases backward; after LEAP, creep, or navigation it erases forward. Either key erases an active Cat highlight as one block.

RightCtrl+C performs Cat COPY when a Cat highlight exists: the highlighted text is duplicated according to Cat semantics. Without a highlight, RightCtrl+C pastes from the system clipboard. CRLF and CR input are normalized to LF.

Creating a Cat highlight also copies its text to the system clipboard. Clipboard failure must not cancel the highlight."#,
    },
    HelpDocument {
        title: "7. Undo, redo and structural commands",
        body: r#"RightCtrl+Z performs Undo and RightCtrl+R performs Redo. Undo/Redo operate on the current in-memory editing history. Consecutive character typing is grouped into a single undo step until navigation, another edit type, or autosave closes that typing run.

RightCtrl+W opens the Work selector. RightCtrl+L inserts a link at point, or opens the existing link when point is already inside one. RightCtrl+N creates a new Document.

RightCtrl+G is an emergency kill switch. It requests immediate application exit without the normal final autosave, checkpoint, session save, or sync. It is intended only for recovery when Carta is still processing keyboard events but normal shutdown is unsafe or stuck; no in-process keybinding can interrupt a thread that has stopped servicing terminal events.

Structural commands are explicit because they change more than ordinary text. Examples include New Document, Split Document at Point, Work membership changes, Work reordering, Trash, restore operations, and Wipe.

Generated boundaries cannot be changed by Backspace, Delete, typing, or Cat highlight operations."#,
    },
    HelpDocument {
        title: "8. Works",
        body: r#"A Work is an ordered list of Document references. It does not contain duplicate copies of Document bodies. Editing a Document in a Work edits that same Document everywhere it appears.

New Document inside a Work is inserted after the current Document. Add to Work switches to the selected Work after adding the Document. Reordering commands affect Work order only, not chronological creation order.

A Document may belong to several Works. Splitting such a Document inserts the new second half after the source in every Work that contains it.

Lock Document makes that Document read-only until Unlock Document. Lock Work freezes Work structure and makes every member Document read-only everywhere it is opened, including Creation Date View or another Work. Unlock Work removes only the Work lock; an individually locked Document remains locked. Locked Documents are marked on their generated separator and in the status bar. Locked Works are marked in the status bar and Work selector."#,
    },
    HelpDocument {
        title: "9. Search, history and trash",
        body: r#"Search Archive is separate from LEAP. LEAP navigates the current View; Search Archive retrieves matches across the Archive.

Document History and Work History expose Git-backed checkpoints as read-only historical views with explicit restore commands. Restores do not silently replace current authored state.

Trash is recoverable structural deletion. Wipe is the explicit permanent-removal operation within Carta Space's control and is intentionally harder to invoke."#,
    },
    HelpDocument {
        title: "10. Export and publication",
        body: r#"Export is publication, not the normal writing workflow. Document and Work exports are always written under $HOME/Downloads. The directory is created automatically if necessary; the export prompt asks only for a filename.

Markdown export preserves authored structure. Built-in PDF publication is intentionally deferred beyond v0.1; external tools can consume the Markdown export when needed.

Packaging an Archive as .cat is distinct from Document/Work export and preserves the portable Archive representation and its required history."#,
    },
    HelpDocument {
        title: "11. Synchronization",
        body: r#"Synchronization is optional and local-first. Carta always saves and checkpoints locally; a network or authentication failure does not disable writing.

Sync Settings configures one Git remote URL for the current device. The URL lives in the local Git configuration and is not part of the portable Archive. Git and SSH handle authentication; Carta does not store remote credentials.

WARNING: synchronization is not encrypted in v0.1. Anyone who can read the remote can read authored text, metadata, and Git history, including historical text no longer present in current Documents.

Carta attempts synchronization at startup when the Archive is clean, after checkpoints, periodically while an editable Archive is clean, and on normal Quit. Sync Now performs an immediate checkpoint when necessary and then synchronizes.

Carta uses fetch, Archive-identity validation, fast-forward or a normal three-way merge, then push. It does not normally rebase or force-push. If Git cannot merge diverged histories cleanly, Carta leaves the working tree untouched, preserves both histories, and reports a sync conflict that cannot complete until the histories are reconciled."#,
    },
    HelpDocument {
        title: "12. Keyboard and terminal notes",
        body: r#"Carta Space relies on terminals that can distinguish physical modifier keys and press/release events.

Left Control and Left Alt are the two LEAP keys. Right Alt/AltGr is reserved for normal international text entry and is never LEAP. Right Control is the Carta command modifier.

When enhanced keyboard reporting is unavailable, Carta remains usable in compatibility mode and exposes LEAP commands through the command palette.

For terminals that cannot deliver physical modifier press/release events, enable Portable Keyboard Mode from the palette. While it is enabled, Ctrl+P acts as a portable Esc: in normal editing it opens the command palette, while in palettes, prompts, selectors, confirmations, and LEAP it performs the same cancel/back action as Esc. Ctrl+B starts a sticky backward LEAP and Ctrl+F starts a sticky forward LEAP; type the pattern and press Enter to finish. Ctrl+R performs Leap Again in the direction of the most recent portable LEAP; during an active sticky LEAP it advances the current query immediately.

Portable Keyboard Mode defaults to off on a host that has no saved preference. Its enabled/disabled state is then remembered locally for that host and restored on later Carta launches on the same machine; it is not Archive metadata and is not carried to other hosts. The status bar shows [PORTABLE] whenever the mode is active. When Portable Keyboard Mode is disabled, Ctrl+P is not intercepted by Carta. Native physical LEAP bindings remain unchanged."#,
    },
];

pub fn documents(kind: HelpKind) -> &'static [HelpDocument] {
    match kind {
        HelpKind::Cheatsheet => CHEATSHEET,
        HelpKind::Manual => MANUAL,
    }
}
