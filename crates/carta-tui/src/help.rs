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
Left Ctrl + Enter           LEAP to current/previous LF line start
Left Alt + Enter            LEAP to next LF line start
Left Ctrl + Home            LEAP to start of current Document
Left Alt + End              LEAP to end of current Document
Left Ctrl + PageUp          LEAP to start of current View
Left Alt + PageDown         LEAP to end of current View
Right Ctrl + Left Alt       Leap Again forward
Right Ctrl + Left Ctrl      Leap Again backward

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
Search Archive
Document History / Work History
Trash / Show Trash
Export Document or Work
Sync Now / Sync Settings
Cheatsheet / Manual

Right Alt / AltGr is never a LEAP key."#,
}];

const MANUAL: &[HelpDocument] = &[
    HelpDocument {
        title: "1. Writing model",
        body: r#"Carta Space is a writing-first environment. The normal surface is a continuous View of Documents, not a file manager and not a page-layout application.

Authored text is Markdown. Structure is expressed semantically rather than by direct typography. Document boundaries, Work membership, history, and other structural information are not encoded as decorative Markdown.

The interface deliberately keeps technology quiet. The command palette exposes power on demand; ordinary writing should remain visually sparse."#,
    },
    HelpDocument {
        title: "2. Documents and boundaries",
        body: r#"A Document is the basic authored unit. It has an immutable UUID and creation timestamp but no separate title field. Its visible label comes from the first heading or first non-empty line.

Chronological and Work Views concatenate several Documents. Their boundaries are generated UI: they are not Markdown, cannot be selected as authored text, and cannot be erased by normal editing.

Split Document at Point is the explicit structural exception. Text before the cursor remains in the original Document; text from the cursor onward becomes a new Document with a new UUID and a timestamp immediately after the original. Every Work containing the source inserts the new Document immediately after it."#,
    },
    HelpDocument {
        title: "3. Views and navigation",
        body: r#"Chronological View presents one monthly Volume in creation order. Work View presents the Documents of one Work in explicit Work order.

The editor normally keeps the cursor about two thirds of the way down the screen. Blank screen space is allowed above or below the available text; this is presentation only and never changes Document content.

Collapse View turns a Chronological or Work View into a read-only navigation overview showing only the first three visual rows of each Document. Up/PageUp and Down/PageDown move between Documents. Enter, or Expand View from the palette, returns to normal editing on the selected Document."#,
    },
    HelpDocument {
        title: "4. LEAP",
        body: r#"Physical Left Control is LEAP backward and physical Left Alt is LEAP forward. Hold the key and type an incremental search pattern.

A lowercase query character matches either case. An uppercase query character requires uppercase. Shift and AltGr may be pressed before or after the LEAP key without terminating the session.

A successful LEAP lands on the target character. A failed LEAP rebounds to its origin. Releasing a LEAP key after no query performs creep in that direction.

LeftCtrl+Enter and LeftAlt+Enter search authored LF boundaries, not wrapped screen rows. Backward LEAP goes to the current logical line start (or the previous one when already at a line start); forward LEAP goes to the next logical line start. With a Cat highlight active these same LEAPs move the highlighted text.

LeftCtrl+Home and LeftAlt+End LEAP to the beginning and end of the current Document. LeftCtrl+PageUp and LeftAlt+PageDown LEAP to the beginning and end of the current View."#,
    },
    HelpDocument {
        title: "5. Leap Again and Cat highlight",
        body: r#"RightCtrl+LeftAlt repeats the last LEAP forward; RightCtrl+LeftCtrl repeats it backward.

During an active physical LEAP, pressing Right Control performs Leap Again without changing the original LEAP origin. This applies both to text-pattern LEAPs and to structural LEAPs using Enter, Home/End, or PageUp/PageDown. Right Control can be pressed repeatedly to reach later matches, logical line starts, or Document boundaries. Pressing the opposite LEAP key while the first remains held highlights the interval from the original origin through the final reachable target.

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
        body: r#"RightCtrl+Z performs Undo and RightCtrl+R performs Redo. Undo/Redo operate on the current in-memory editing history.

Structural commands are explicit because they change more than ordinary text. Examples include New Document, Split Document at Point, Work membership changes, Work reordering, Trash, restore operations, and Wipe.

Generated boundaries cannot be changed by Backspace, Delete, typing, or Cat highlight operations."#,
    },
    HelpDocument {
        title: "8. Works",
        body: r#"A Work is an ordered list of Document references. It does not contain duplicate copies of Document bodies. Editing a Document in a Work edits that same Document everywhere it appears.

New Document inside a Work is inserted after the current Document. Add to Work switches to the selected Work after adding the Document. Reordering commands affect Work order only, not chronological creation order.

A Document may belong to several Works. Splitting such a Document inserts the new second half after the source in every Work that contains it."#,
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

Markdown export preserves authored structure. PDF export is a publication path built from the same structural content.

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

When enhanced keyboard reporting is unavailable, Carta remains usable in compatibility mode and exposes LEAP commands through the command palette."#,
    },
];

pub fn documents(kind: HelpKind) -> &'static [HelpDocument] {
    match kind {
        HelpKind::Cheatsheet => CHEATSHEET,
        HelpKind::Manual => MANUAL,
    }
}
