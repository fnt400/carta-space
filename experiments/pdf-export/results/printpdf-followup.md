# printpdf Follow-up

Measured against the shared `printpdf-latin.pdf` (48,582 bytes) and
`printpdf-unicode.pdf` (35,005 bytes). Baseline renderer, default CSS, and
shared PDFs were not modified. All commands ran inside `carta-dev`.

## Real Font Subsetting

`qpdf --filtered-stream-data` decoded each `/FontFile2`; fontTools read its
`maxp.numGlyphs`. Original TTFs were matched by PostScript name. Glyph counts
include `.notdef`. Decoded lengths equal the PDF stream's `/Length1`.

| PDF | Face | Original bytes / glyphs | Embedded decoded bytes / glyphs |
| --- | --- | ---: | ---: |
| Latin | Source Code Pro Regular | 210,312 / 1,568 | 14,204 / 37 |
| Latin | Source Serif 4 Regular | 261,868 / 1,464 | 12,660 / 98 |
| Latin | Source Serif 4 Italic | 187,808 / 1,093 | 4,224 / 11 |
| Unicode | Noto Sans Devanagari | 219,212 / 954 | 8,336 / 14 |
| Unicode | Noto Sans JP | 5,767,260 / 17,936 | 4,676 / 11 |
| Unicode | Source Serif 4 Regular | 261,868 / 1,464 | 13,776 / 114 |
| Unicode | Noto Sans Hebrew | 26,900 / 149 | 6,480 / 6 |
| Unicode | Noto Sans Arabic | 240,456 / 1,661 | 8,492 / 18 |

These are genuinely reduced font programs, not full fonts made tiny solely by
compression. Latin has 31,088 decoded font bytes (16,907 compressed); Unicode
has 41,760 decoded font bytes (23,495 compressed). Embedded programs omit
`OS/2`, GSUB and GPOS; the JSON log records their remaining tables. This table
does not certify shaping or PDF standards conformance.

### Why `pdffonts` Says `sub=no`

Verified in matching Poppler 25.03.0 source, `poppler/GfxFont.cc:249-260`:
`GfxFont::isSubset()` recognizes exactly six uppercase A-Z letters, then `+`.
`utils/pdffonts.cc:156` prints that boolean; it does not count embedded glyphs.

printpdf 0.12.8 `src/html/mod.rs:1090` creates `FontId(format!("F{}", hash))`.
`src/serialize.rs:1920-1922` takes the first six characters of that font ID
as the subset prefix when `was_subset` is true. Measured names include
`F18280+NotoSansJP-Regular` and `F18282+SourceSerif4-Regular`. Their six-character
tags contain digits, violating the six-uppercase-letter subset-tag rule
(ISO 32000-1, 9.6.4), so Poppler reports `sub=no`. This cause is source-verified,
not an inference from output size. Both `/BaseFont` levels and descriptor
`/FontName` match, but all inspected subset prefixes fail `[A-Z]{6}+`.

## Explicit Semibold Diagnostic

Separate input: `../printpdf/probes/weight-explicit.html`. Inline CSS on H1 and
strong specifies `font-family: 'Source Serif 4 Semibold'; font-weight: 600`.
It is not a change to the baseline CSS or executable. Same printpdf engine.

- Output: `printpdf-weight-explicit.pdf`, 13,705 bytes, one page.
- Font inspection: `printpdf-weight-explicit.pdffonts.txt` includes
  `F13117+SourceSerif4-Semibold`, plus Regular and Italic.
- Semibold stream: 5,548 decoded bytes, 22 glyphs; original: 272,088 bytes,
  1,464 glyphs, original `OS/2.usWeightClass=600`.
- Preview: `printpdf-weight-explicit.png`, visually inspected. H1 and strong
  are visibly heavier than regular body text. Extraction:
  `printpdf-weight-explicit.layout.txt`, retaining all diagnostic text.
- `qpdf --check` reports no syntax/stream-encoding errors; this does not
  validate the malformed subset naming or prove full standards conformance.

Explicit family selection works. Native weight-based selection remains a
baseline defect; this diagnostic does not repair or overwrite that result.

## Native Footer Source Mismatch

The pinned azul-layout 0.0.16 `solver3/display_list.rs:9528-9551` generates
footer items through `generate_text_display_items`. At lines 9958-10050 this
function selects a registered font and returns a standalone `Text` item with
real glyph IDs and that font's hash, without a `TextLayout` item.

printpdf `src/html/bridge.rs:523-541` still assumes standalone footer text has
Unicode-codepoint glyph IDs and placeholder hash zero. It returns immediately
for nonzero-hash `Text` items, assuming a `TextLayout` already renders them.
These source paths conflict: real registered-font footer items have no such
layout to render them. This is a concrete producer/consumer mismatch consistent
with the observed missing footer. No display-list instrumentation was added,
so the actual generated footer item was not captured in a runtime trace.
No footer overlay, alternate engine, or main-PDF workaround was introduced.

## Runtime System-Font Scan

`build_font_pool(&fonts, Some(&[]))` excludes discovered system faces from the
resolution cache, but does NOT avoid scanning or opening font files.
rust-fontconfig 5.0.1 `src/lib.rs:1706-1714` calls `FcScanDirectories()` before
applying its family filter. `FcScanDirectories` reads system configuration.

An actual `strace -f -e trace=%file` run of the separate weight diagnostic
successfully opened **122 unique system TTF/OTF files**, plus `/etc/fonts/fonts.conf`,
configuration includes and shared-home fontconfig paths. Only embedded faces
are selected, but startup I/O and configuration access are not hermetic.
Missing-system-config behavior was not tested; system-font scanning must not
be described as disabled. No runtime configuration workaround was applied.

## Evidence And Tools

Paths below are relative to `experiments/pdf-export/printpdf/`:

- `inspect-font-streams.py`: reproducible qpdf/fontTools measurement script.
- `logs/font-stream-inspection.jsonl`: object IDs, decoded/compressed lengths,
  original/embedded glyph counts, names, tables, and invalid-tag checks.
- `logs/followup-source-evidence.log`: subset detector, naming, footer and scan
  source excerpts. Matching Poppler source downloaded only under container
  `/opt/carta-pdf-bench/poppler-25.03.0`.
- `logs/weight-explicit-runtime.log`, `logs/weight-explicit-qpdf.log`:
  diagnostic renderer and qpdf results.
- `logs/system-font-scan.strace`, `logs/system-font-scan-summary.log`,
  `logs/system-font-open-count.log`, `logs/system-font-scan-runtime.log`:
  actual access evidence and successful-open count.
- `logs/followup-tools.log`, `logs/baseline-pdf-sha256.log`: tools and baseline hashes.

Used preinstalled qpdf 12.2.0, Poppler 25.03.0, Python and fontTools 4.57.0.
No pypdf or other package was installed. The inspection script was executed
successfully on all three PDFs; the diagnostic PDF was rendered, inspected,
extracted, rasterized and qpdf-checked. No Rust source changed, so no Rust
rebuild or lint run was necessary for this follow-up.
