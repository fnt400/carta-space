# Automatic Dictionary Download Verification: FAIL

## Scope

- Repository: `fnt400/carta-space`.
- Branch: `opencode/v0.2`.
- Tested HEAD: `36cfdad5a55bf5b65c55d3f113da7e3c709f3a85`.
- Date: 2026-10-10.
- Rust checks and functional harness: existing Ubuntu 26.04 `carta-gui-dev`
  Distrobox, Rust 1.93.1 / Cargo 1.93.1.
- Nix builds and wrapper startup: NixOS host.
- Working tree inspected before `git pull --ff-only origin opencode/v0.2`.
  The checkout was already at the exact requested HEAD. There were no tracked
  changes; a pre-existing untracked `result` symlink was preserved.
- All Archive contents, remotes, homes, dictionary caches and fixtures used for
  functional testing were synthetic and temporary. No personal Archive was opened.
- No source, algorithm, dependency, persistence, synchronization or contract
  change is published. This report is the only repository change.

## Confirmed Functional Failure

### Late Completion Interrupts Writing If the Cursor Returns to Its Original Position

Priority: high. This violates the requested guarantee that a late dictionary
completion must not interrupt writing.

References:

- `crates/carta-app/src/spelling_ui.rs:130-137`: completion checks only
  `AppMode::Editing` and equality of the current cursor with the saved cursor.
- `crates/carta-app/src/spelling_ui.rs:171-174`: pending state stores only
  `(work, Cursor)`.
- `crates/carta-app/src/spelling_ui.rs:200-206,247-282`: resumed review creates
  a Cat highlight and switches to the spelling selector.
- `crates/carta-gui/src/main.rs:894-906`: the GUI maintenance tick processes
  this shared completion path.

Executed reproduction at the public shared application layer:

1. Use an empty isolated dictionary cache and a synthetic Document containing
   `éléphnt`, with language `fr_FR`.
2. Ensure no system dictionary is available to the manager's probe.
3. Start `Command::CheckDocumentSpelling`.
4. Delay each curl invocation by one second, then delegate it to real curl.
   The files are actually downloaded over HTTPS from the pinned upstream.
5. While download is pending, insert `écriture ` through
   `Action::InsertText`.
6. Move the cursor back to its original `{region, byte}`.
7. Process the real completion with `tick_without_remote_sync`.

Expected: keep `AppMode::Editing`, preserve the text, and announce that spelling
can be requested when convenient.

Observed: text remains intact, but the application enters the spelling
`Selector`. Subsequent typing is handled by the selector instead of the editor.
The assertion that late completion preserves Editing fails.

Control case: identical download and typing without returning the cursor passes,
and reports `Dictionary ready · Check Spelling when convenient`.

Sanitized executed observations:

```text
cursor returned=false: Editing, Dictionary ready · Check Spelling when convenient
cursor returned=true: Selector, Spelling dictionary fr_FR ready
FAIL: Late download interrupted writing
```

The production guard does not distinguish an unchanged editing session from
typing/navigation followed by a return to the same coordinates. This is a
reproduced shared-controller defect, not a claim based solely on source review.
It was not corrected autonomously.

## Build and Test Matrix

Executed the entire script inside the Distrobox:

```bash
bash scripts/verify-carta.sh
```

All script commands passed:

| Command | Result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo fmt --manifest-path crates/carta-gui/Cargo.toml --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS: 319 passed, 0 failed, 1 ignored |
| `cargo build --workspace` | PASS |
| `cargo clippy --manifest-path crates/carta-gui/Cargo.toml --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --manifest-path crates/carta-gui/Cargo.toml` | PASS: 39 passed |
| `cargo build --manifest-path crates/carta-gui/Cargo.toml` | PASS |
| `cargo build --release --manifest-path crates/carta-gui/Cargo.toml` | PASS |
| `git diff --check` | PASS |

Executed both requested Nix package builds on the host:

```bash
nix build .#carta-gui --no-link --print-out-paths
nix build .#carta-space --no-link --print-out-paths
```

Both passed. `--no-link` preserves the user's existing `result` symlink;
it does not bypass package construction or install anything globally.

No mechanical correction was needed. No tracked lockfile changed.

## Functional Harness and Results

A temporary external Cargo harness exercised the published public APIs without
adding repository tests. HOME, XDG_DATA_HOME and global Git configuration were
isolated. The final correctly configured run performed nine checks: eight passed,
one failed, exit status 1.

The Distrobox already has system dictionaries from the earlier spelling check.
To exercise actual downloads, a temporary Hunspell launcher rejects ONLY the
manager's exact `hunspell -a -d <locale>` system probe. All actual spelling
commands delegate unmodified to the real Hunspell 1.7.2 executable supplied by
Nix. No spelling responses, suggestions or dictionary recognition are simulated.

A temporary curl launcher logs invocations and delegates to real curl for online
tests. Fault tests either return a controlled network error or write a truncated
output file; delayed tests sleep one second before running real curl. These
changes affect only the test process and temporary files, not the application.

| Required Area | Executed Result |
| --- | --- |
| Select `fr_FR` with empty cache | PASS: shared language selector initiates three real HTTPS downloads |
| Affix, dictionary and license integrity | PASS: independently compared all French file Git blob IDs with the pinned catalog; other managed resolutions also verify all three files |
| Cache location | PASS: files under isolated `$XDG_DATA_HOME/carta/dictionaries/<locale>-8cfea406b505e4d7df52d5a19bce525df98c54ab/` |
| Git isolation | PASS: Archive HEAD and porcelain status remain identical across download after the intentional language-metadata checkpoint |
| Real French correction | PASS: `bonjour éléphnt` becomes `bonjour éléphant` using an actual suggestion |
| Offline reuse | PASS: new managers resolve all three cached languages as Managed and run real checks; zero additional curl invocations with curl configured to reject all network use |
| Italian and British English | PASS: real downloads and scans of `po' po’ città buogiorno` and `hello café helo`, with correct source spans and typo suggestions |
| Multilingual Work | PASS: actual review visits French, Italian and British English members in Work order and applies real suggestions |
| Network unavailable | PASS: nonmodal `Spelling unavailable (fr_FR)` with download failure; editor remains writable and no final cache is published |
| Truncated download | PASS: integrity error, no final cache, staging cleaned up, typed text retained |
| Altered existing cache | PASS: new manager/App rejects a modified `.dic` with `incomplete or corrupted` and retry guidance; Document and Archive HEAD unchanged |
| Download operation responsiveness | PASS at shared layer: starting review and inserting text while the delayed worker runs each return within the harness's 250 ms bound |
| Late completion after typing | PASS if cursor moved; FAIL if cursor returns to its starting coordinates, as detailed above |
| Personal words | PASS: additions remain in Archive `spelling/fr_FR.dic`, appear in Git checkpoints and are recognized by real Hunspell with the managed dictionary |
| Quit publication | PASS at shared layer: saves Work corrections, verifies staged final publication, local HEAD equals the disposable remote's `carta` HEAD, scheduler has no pending push |
| Concurrent personal additions | PASS: two synthetic Archive copies preserve both additions through real core sync; real Hunspell recognizes the resulting union |

The sample `café` is reported by the downloaded British English dictionary;
that vocabulary choice is not classified as an application failure.

The offline test measures downloader invocations, not a packet capture. No
attempted download occurs when the verified cache is reused. Git hashing and
local Hunspell execution do not require network access.

The existing automatic suites also cover generic LEAP, Cat selection, Undo/Redo,
locked Documents, pending push retries, and explicit failure acknowledgment on
GUI Quit. Those passing tests are not manual GUI evidence.

## Wrapper and GUI Limitations

The built Nix wrapper was inspected: it supplies Git, curl, Hunspell and Typst,
without preinstalled language corpora. On the host, that Hunspell's real
`-a -d fr_FR` probe fails with missing affix/dictionary files.

The real packaged GUI was launched with an explicit synthetic Archive and
isolated HOME/XDG data. Its X11 window rendered the synthetic Document. Temporary
Nix-shell tools were used for an attempted input smoke test; nothing was added
to a global profile.

The automated X11 input attempts did not produce verifiable text insertion or
dictionary downloads, and palette Quit did not complete within the harness
deadline. The test process was then terminated. This is recorded as an
INCONCLUSIVE automation result, not another confirmed application regression:
the attempted keystrokes were not established as reaching the application.

Consequently these required acceptance areas are NOT established:

- Full download/correction initiated from the actual Nix GUI window.
- Physical-keyboard/manual GUI behavior during download.
- Sustained release-mode latency, frame smoothness, idle CPU and memory profile.
- Manual graphical Quit with a real remote failure and pending-push warning.

The shared-controller late-completion failure is confirmed independently of
these graphical testing limitations. No claim of GUI responsiveness PASS is made.
No local screenshot, raw log, personal path or real Archive data is published.

## Unrelated and Environmental Observations

- The pre-existing Typst smoke test remains ignored because it requires a
  Typst 0.15.1+ executable in `CARTA_TYPST`. It was not modified.
- The Nix wrapper currently supplies Typst 0.14.2. Publication compatibility
  was outside this dictionary-download task and was not repaired or retested.
- Parallel Nix evaluation emitted an ignored SQLite busy warning. Both requested
  builds nevertheless completed successfully; it is not a source failure.
- No CI workflow was modified and no unrelated CI issue was repaired.

## Disposition

Overall: **FAIL** due to the reproduced late-completion interruption, despite
passing build, integrity, offline, spelling, storage and synthetic Git tests.
Only this sanitized report is published. The completion guard requires
implementation review; the remaining graphical acceptance tests still need
reliable runtime input and profiling.
