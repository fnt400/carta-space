# Crossterm keyboard event probe

This standalone experiment tests what the current terminal path actually reports through
Crossterm. It is not a Carta Space crate and implements no LEAP behavior.

It requests these kitty keyboard protocol enhancements when the terminal reports support:

- disambiguated escape codes;
- press, repeat, and release event types;
- escape-code reporting for every key, required for releases of ordinary text keys.

`REPORT_ALTERNATE_KEYS` is deliberately not requested: Crossterm may use the alternate keycode
instead of the resulting character, which would make the AltGr and international text test less
representative.

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
KEY kind=Press   code=Modifier(LeftControl)  modifiers=KeyModifiers(CONTROL)
KEY kind=Release code=Modifier(LeftControl)  modifiers=KeyModifiers(0x0)
KEY kind=Press   code=Modifier(RightControl) modifiers=KeyModifiers(CONTROL)
KEY kind=Release code=Modifier(RightControl) modifiers=KeyModifiers(0x0)
```

The exact modifier value on a modifier's own release can vary. The important observations are the
`LeftControl`/`RightControl` code and separate `Press`/`Release` kinds. If startup says that enhanced
reporting is unsupported, or every event is only `Press`, that terminal path does not provide the
required fidelity through Crossterm.
