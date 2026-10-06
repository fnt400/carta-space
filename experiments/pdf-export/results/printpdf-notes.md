# printpdf 0.12.8 Isolated Experiment

These are worker notes from preliminary probes. Shared-corpus rendering,
multipage benchmarks and reference shaping checks were subsequently completed.
See REPORT.md for the final comparison, including confirmed failures.

Status: renderer built and local probes validated. Coordinator-provided
`benchmark/generated/{latin,unicode,perf-1,perf-10,perf-100}.html` inputs were
not present at verification time, including a final 60-second wait. Neither
root `benchmark/generated` nor `experiments/pdf-export/benchmark/generated`
existed. No substitute benchmark results are claimed.

Renderer: `/opt/carta-pdf-bench/printpdf-target/release/carta-printpdf-bench INPUT.html OUTPUT.pdf`

Rust toolchain: exact 1.94.0, container-only homes under `/opt/carta-pdf-bench`.
Independent workspace: `experiments/pdf-export/printpdf/Cargo.toml`.
No main workspace changes or shared-home toolchain changes.

## Implementation

- Exact dependency `printpdf = "=0.12.8"`, standalone `[workspace]`, local lockfile.
- Uses the actual crate's HTML/CSS engine through `html::add_xml_to_document`
  and `XmlRenderOptions`, not a handwritten text paginator or alternate engine.
- Direct raw TTF registration through `XmlRenderOptions::fonts`, documented
  in `src/html/mod.rs`. The corresponding public `from_html` registration is
  covered by the crate's `tests/html_font_resolution.rs`.
- All nine fonts use `include_bytes!`; the executable also embeds its CSS.
  Input HTML and output path are the only required application files at runtime.
- A4, 32 mm side margins, 25 mm top/bottom margins are programmatic options.
  CSS requests body 11 pt, line height 1.35, left alignment, black text, serif,
  headings 20/15/12 pt without numbering, indented quotes/lists, and mono code.
- Native `show_page_numbers: true`, no header, no manual footer overlay.
- `build_font_pool(&fonts, Some(&[]))` excludes every system font from resolution.
  Source inspection confirms the empty filter rejects all scanned system faces.
  It does still scan fontconfig configuration/directories, so it does not prove
  zero filesystem fontconfig access. No external font is selected in our probes.

## Toolchain And Dependencies

- Container system tools: `/usr/bin/rustc` and `/usr/bin/cargo` 1.85.1.
  The crate declares Rust 1.88 minimum, so these were not used for compilation.
- Exact installed compiler: rustc 1.94.0 (`4a4ef493e`, 2026-03-02), LLVM 21.1.8.
- Cargo 1.94.0 (`85eff7c80`, 2026-01-15), x86_64-unknown-linux-gnu.
- Rustup installer and downloaded crate source are under `/opt/carta-pdf-bench`.
  Installation used minimal profile, rustfmt, Clippy, and `--no-modify-path`.
  `RUSTUP_HOME=/opt/carta-pdf-bench/rustup`,
  `CARGO_HOME=/opt/carta-pdf-bench/cargo`,
  `CARGO_TARGET_DIR=/opt/carta-pdf-bench/printpdf-target`.
- Cargo metadata resolves 132 packages including this executable: 131
  dependency packages, 223 dependency edges, including target-specific packages.
- Enabled printpdf features: `default`, `html`, `text_layout`,
  `text_layout_hyphenation`. No image, SVG, or HTML-multithreading feature.
- Main resolved engine dependencies: azul-core/css/layout 0.0.16,
  rust-fontconfig 5.0.1, allsorts-azul 0.17.2, lopdf 0.44.0.
- Full tree and tool versions: `experiments/pdf-export/printpdf/logs/toolchain-tree.log`.
  Feature tree: `logs/features.log`. Metadata: `logs/metadata.json`.
  Concise stats: `logs/dependency-stats.log` (all log paths relative to printpdf).

## Packaging

| Measurement | Bytes |
| --- | ---: |
| Original release executable | 21,108,648 |
| Stripped release executable | 19,473,272 |
| Embedded source TTF total, nine files | 7,755,112 |
| Noto Sans JP alone | 5,767,260 |

Stripped executable: `/opt/carta-pdf-bench/printpdf-target/release/carta-printpdf-bench-stripped`.
Its smoke run succeeded and extracted text matches the original executable.
The sizes are measurements of the initial release; a source-comment-only edit
does not change executable behavior. Packaging details: `logs/packaging.log`.

`ldd` lists only `libgcc_s.so.1`, `libm.so.6`, `libc.so.6`, Linux vdso and
`/lib64/ld-linux-x86-64.so.2`. No dynamically linked fontconfig, HarfBuzz,
FreeType, Cairo, browser, or external renderer. Built on Debian 13 glibc 2.41;
compatibility with older glibc or execution on the NixOS host was not tested.
The renderer does not need Rust/Cargo or the source fonts at runtime.
Inspection scripts additionally need Poppler tools and `/usr/bin/time`.

## Fallback Results

Both probes put `Italiano 日本語 Ελληνικά العربية हिन्दी` in a single paragraph.
The automatic probe only specifies the body serif; the explicit probe assigns
script-specific font families on spans. Both render one A4 page and extract
all five words, with Poppler's directional markers around Arabic. A subsequent
exact comparison FAILED for both: after removing Unicode format controls and
collapsing whitespace, extraction is `Italiano 日本語 Ελληνικά العربيةहिन्दी`,
missing the input's space between Arabic and Hindi. Therefore extraction is
not an exact preservation pass. The origin of this separator loss (PDF text
positioning/mapping versus Poppler's bidi extraction) has not been isolated.
Raster inspection of the automatic probe shows all five scripts without
visible missing-glyph boxes. Only the automatic probe raster was visually
inspected in the initial run; the explicit probe was checked by extraction
and embedded-font inspection. Neither check verifies correct Arabic joining,
ligatures, mixed-direction ordering, or Devanagari shaping against a trusted
reference. Hebrew and a full RTL paragraph have not been tested. RTL shaping
and bidi correctness remain unverified, not passed.

Exact initial evidence (relative to `experiments/pdf-export/printpdf/`):

- `logs/fallback-auto.log` and `logs/fallback-explicit.log`: renderer output,
  exit status, and `/usr/bin/time -v` measurements.
- `logs/fallback-auto-fonts.log` and `logs/fallback-explicit-fonts.log`:
  `pdffonts` output showing only the packaged faces used in each PDF.
- `logs/fallback-auto-info.log` and `logs/fallback-explicit-info.log`:
  `pdfinfo` output.
- `outputs/fallback-auto.txt` and `outputs/fallback-explicit.txt`:
  `pdftotext -layout` output, not a shaping-correctness assertion.
- `logs/fallback-extraction-check.log`: exact normalized string comparison,
  failing for both probes because of the Arabic/Hindi separator loss.
- `outputs/fallback-auto.png`: initially inspected raster, generated using
  `pdftoppm -scale-to 1200 -singlefile -png`.
- `probes/fallback-auto.html`, `probes/fallback-explicit.html`, `classic.css`,
  and `src/main.rs`: exact input, styles, and font pool configuration.

The automatic CSS family is only `Source Serif 4`, with no explicit Noto
fallback chain. The engine chooses per-character fallback from its registered
faces. The explicit probe uses Source Serif 4 for Italian, Noto Sans JP for
Japanese, Noto Sans for Greek, Noto Sans Arabic for Arabic, and Noto Sans
Devanagari for Hindi. The body remains left aligned; no `dir="rtl"` or CSS
`direction` is set. Italic uses `font-style: italic`; headings and strong text
request weight 600. Regular/Italic/Semibold bytes are registered under separate
map keys as well as the internal family names registered by the engine.

| Probe | PDF bytes | Wall time | Max RSS KiB |
| --- | ---: | ---: | ---: |
| Automatic fallback | 18,581 | 0.36 s | 49,552 |
| Explicit script families | 23,563 | 0.25 s | 51,156 |

These are single runs, not statistical performance comparisons. The first
run incurred cold filesystem reads. Automatic fallback embedded Source Serif
4, Noto Sans JP, Noto Sans Arabic, and Noto Sans Devanagari; Greek remained in
Source Serif 4. Explicit selection additionally embedded Noto Sans for Greek.
`pdffonts` reports embedded CID TrueType faces with Unicode maps. Its `sub=no`
flag should not be interpreted as embedding all 7.7 MB: output sizes are small.

Raw runtime/font/page logs: `logs/fallback-{auto,explicit}*.log`.
PDFs, extracted text, and automatic-probe raster: `outputs/fallback-*`.

## API And CSS Problems

1. Full CSS `@page` parsing is explicitly unimplemented in crate source
   (`src/html/mod.rs`, around line 585). Geometry uses the exposed API instead.
2. Requested native page numbering produces no visible or extractable footer
   in either fallback probe or the semantic probe. Even the documented format
   is `Page X of Y`, not the requested small centered bare page number; there
   are no footer font/size/alignment controls in `XmlRenderOptions`. No workaround
   was added. A plausible source-level cause is the bridge discarding standalone
   external-font `DisplayListItem::Text` items (bridge.rs around lines 535-541),
   but this has not been proven with display-list instrumentation.
3. `font-weight: 600` does not select the supplied Source Serif 4 Semibold in
   `outputs/semantics.pdf`. `pdffonts` lists Regular, Italic and Source Code Pro,
   but not Semibold; raster inspection confirms regular headings/strong text.
   Italic selection works. No explicit-family workaround silently replaces
   this failed native weight-selection result.
4. Lists, blockquote indentation, H1/H2/H3 sizes, black links, and mono inline/
   fenced-style code render in the local semantic probe. Code indentation and
   newlines survive extraction. List baseline spacing is about 14.849 pt,
   consistent with 11 pt x 1.35. Left edge is 90.7088 pt, matching 32 mm.
5. `xml_to_pdf_pages` accumulates warnings internally but returns only pages/
   resources on success. `add_xml_to_document` exposes error warnings, not
   successful-layout warnings; this renderer logs save warnings and errors,
   but a successful PDF is not proof of complete CSS support.
6. PDF metadata is the engine default (epoch timestamps, untagged PDF with
   PDF/X-identifying metadata). No PDF/X conformance claim is made or tested.
7. Exact mixed-script text extraction loses the Arabic/Hindi separator in both
   automatic and explicit probes; see `logs/fallback-extraction-check.log`.

All failed/incomplete probe PDFs remain untouched in `outputs/`. No alternate
engine or replacement content is used.

## Validation And Pending Work

- Release build passed with the pinned dependency and `--locked`.
- Standalone `cargo fmt --check` passed.
- Standalone `cargo clippy --all-targets --all-features -- -D warnings` passed.
- Standalone `cargo test` passed with zero tests; runtime probes supply actual
  rendering checks, but there is no automated visual regression suite.
- Shell syntax checks passed for `build.sh` and `benchmark.sh`.
- Incorrect argument count exits nonzero with the executable interface.
- Original and stripped binaries both produced readable smoke PDFs.
- Subsequent exact normalized extraction checks FAILED for both mixed-script
  probes. Word coverage alone is not exact text preservation or RTL validation.
- Supplied Latin, full Unicode, and 1/10/100-page benchmark runs remain pending
  because their input files were absent. Multipage layout and throughput are
  not established by these one-page probes.

When the inputs arrive, run inside the container:

```bash
bash experiments/pdf-export/printpdf/benchmark.sh INPUT_DIRECTORY
```

The script renders the five exact supplied inputs, preserves outputs and raw
logs, and uses a 180-second timeout per run. See `logs/validation.log` and
`logs/build-release.log` for the original build/validation output.
