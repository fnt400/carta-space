# Iced input probe

This standalone experiment validates Iced 0.14 as the candidate desktop input/windowing layer for Carta Space v0.2.

It is deliberately **not** part of the Carta workspace and does not implement Carta behavior. The experiment exists to answer the blocking question before a GUI toolkit is adopted:

> Can the desktop frontend reliably distinguish physical Left/Right Ctrl and Left/Right Alt press/release events while preserving normal Unicode, AltGr and IME text input?

Iced 0.14 requires Rust 1.88 or newer. Carta's main workspace remains on its current Rust 1.85 MSRV while this experiment is evaluated.

## Run

Use a current stable Rust toolchain:

```sh
cd experiments/iced-input-probe
cargo +stable run
```

On Linux, the probe enables both Wayland and X11. Run it in the desktop session normally used for Carta.

## Test sequence

Click the text field first, then verify:

1. Press and release Left Ctrl.
2. Press and release Right Ctrl.
3. Press and release Left Alt.
4. Press and release Right Alt / AltGr.
5. Type ordinary lowercase and uppercase text.
6. Type accented/non-ASCII characters.
7. Use a real AltGr combination from the active keyboard layout.
8. If an IME is configured, enter text through preedit and commit.

For the four physical modifiers the log must contain independent DOWN and UP lines such as:

```text
LeftCtrl DOWN ...
LeftCtrl UP ...
RightCtrl DOWN ...
RightCtrl UP ...
LeftAlt DOWN ...
LeftAlt UP ...
RightAlt DOWN ...
RightAlt UP ...
```

## Acceptance criteria

The probe passes only if all of the following are true:

- Left Ctrl, Right Ctrl, Left Alt and Right Alt are distinguishable by physical key code.
- Press and release are both reported reliably.
- Holding Left Ctrl or Left Alt can be treated as a momentary Carta gesture.
- Right Alt/AltGr remains independent from Left Alt.
- AltGr produces the expected text in the text field.
- Normal Unicode text input works.
- IME preedit/commit events are observable when an IME is available.
- No stuck modifier state appears after release.

The window includes an **Automatic observations** section. After the test, the four modifier checks, normal text, Unicode and AltGr-produced-text checks should all read `PASS`. IME checks are informational when no IME is configured.

If any required criterion fails on the target desktop environment, Iced must not be adopted for Carta's canonical GUI without first resolving the failure.
