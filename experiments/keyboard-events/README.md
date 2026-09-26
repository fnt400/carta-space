# Crossterm keyboard event probe

This standalone experiment tests what the current terminal path actually reports through
Crossterm. It is not a Carta Space crate and implements no LEAP behavior.

It requests these kitty keyboard protocol enhancements when the terminal reports support:

- disambiguated escape codes;
- press, repeat, and release event types;
- alternate keycodes, so Crossterm reports the layout-produced shifted character;
- escape-code reporting for every key, required for releases of ordinary text keys.

## Run

From the repository root:

```sh
distrobox enter carta-dev
cd ~/software/git/carta-space/experiments/keyboard-events
cargo run
```

Press `Esc` or `F12` to exit. The program restores the enhancement stack and raw mode through an
RAII guard on normal exit, I/O errors, and unwinding panics. Uncatchable termination such as
`SIGKILL` cannot run cleanup; if that happens, run `reset` or open a fresh terminal.

## What to look for

A fully useful result should contain separate lines resembling:

```text
KEY kind=Press   code=Modifier(LeftControl) modifiers=KeyModifiers(CONTROL)
KEY kind=Release code=Modifier(LeftControl) modifiers=KeyModifiers(0x0)
KEY kind=Press   code=Modifier(LeftAlt)     modifiers=KeyModifiers(ALT)
KEY kind=Release code=Modifier(LeftAlt)     modifiers=KeyModifiers(0x0)
KEY kind=Press   code=Modifier(RightAlt)    modifiers=KeyModifiers(ALT)
KEY kind=Press   code=Modifier(RightControl) modifiers=KeyModifiers(CONTROL)
```

The exact modifier value on a modifier's own release can vary. The important observations are distinct
`LeftControl`, `LeftAlt`, `RightAlt`, and `RightControl` codes plus separate `Press`/`Release` kinds.
Carta uses `LeftControl` for LEAP backward and `LeftAlt` for LEAP forward; `RightAlt` must remain
available for AltGr/international input. Also test Shift, an ordinary character, Shift+character, and
RightAlt/AltGr+character. If startup says enhanced reporting is unsupported, or every event is only
`Press`, that terminal path does not provide the required fidelity through Crossterm.
