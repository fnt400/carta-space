# GNU groff 1.24.1 PDF Experiment

These are worker notes, including preliminary smoke checks. Shared-corpus
results were subsequently completed; see REPORT.md for the final comparison.

Isolated experiment only. No main application, archive, corpus, shared fonts, or
printpdf changes were made by this worker. All downloads, compilation, font
conversion, and runtime inspection ran via `distrobox enter carta-dev`.
No host packages were installed. Measurements were taken on 2026-10-06.

## Renderer Interface

From the repository root:

```bash
distrobox enter carta-dev -- bash experiments/pdf-export/groff/render.sh input.roff output.pdf
```

The script requires exactly two arguments and the `carta-dev` container. It
invokes `/opt/carta-pdf-bench/groff-1.24.1/bin/groff` with
`-Kutf8 -Tpdf -P-pa4 -M <groff-directory> -mcarta -ww`, puts the PDF at the
requested path, and preserves diagnostics in `output.pdf.stderr.log`.
The output parent must already exist. Missing glyph warnings do **not** make
groff exit unsuccessfully: PDF creation is not proof of correct Unicode output.
Input is semantic UTF-8 roff, not Markdown; no converter is included here.

| Macro | Interface |
| --- | --- |
| `H1`, `H2`, `H3` | `.H1 "Heading text"`, similarly H2/H3; no numbering |
| `P` | Begin a paragraph; preserves selected font |
| `BQ`, `EQ` | Begin/end a block quotation, indented 8 mm |
| `UL`, `OL` | Begin a simple unordered/ordered list |
| `LI` | Begin item; optional quoted label overrides bullet/automatic number |
| `LE` | End the list |
| `CODE`, `ENDCODE` | Begin/end non-filling monospaced code |
| `FONT` | `.FONT NS`, `.FONT NH`, etc.; selects a font without shaping |

Use ordinary roff escaping for generated text: `\e` for literal backslash,
`\&` before a literal leading dot/apostrophe. Inline fonts can use
`\f[CI]italic\f[CR]`, `\f[CB]semibold\f[CR]`, and
`\f[CM]code\f[CR]`; `\fP` restores the previous font.
Lists are simple, not a nested list-stack implementation. Code lines are not
wrapped. The macros reserve no syntax for links, tables, or images.

`carta.tmac` mounts these fonts explicitly:

| Name | Supplied font |
| --- | --- |
| CR | SourceSerif4-Regular.ttf |
| CI | SourceSerif4-Italic.ttf |
| CB | SourceSerif4-Semibold.ttf |
| CM | SourceCodePro-Regular.ttf |
| NS | NotoSans-Regular.ttf |
| NH | NotoSansHebrew-Regular.ttf |
| NA | NotoSansArabic-Regular.ttf |
| ND | NotoSansDevanagari-Regular.ttf |
| NJ | NotoSansJP-Regular.ttf |

Fonts are not marked special and no automatic Noto fallback is configured.
Physical bindings, archive semantics, and application architecture are irrelevant
to this experiment. Style: A4 portrait, top/bottom body margin 25 mm,
left/right 32 mm (146 mm text width), body 11 pt/14.85 pt, left aligned without justification or
hyphenation; semibold H1/H2/H3 at 20/15/12 pt. Footer: centered 8 pt page number,
no header. The body ink limit is 272 mm; the trap reserves body leading and
glyph descent before that limit. The footer baseline is independently placed
near 282 mm and centered across the 146 mm text width.
groff's `m` unit is **em**, not millimetres; physical dimensions use
centimetres in the macro package.

## Build Procedure

Official GNU release tarball, exactly 1.24.1. `ftp.gnu.org` timed out after
136 seconds; `ftpmirror.gnu.org` timed out after 15 seconds; a bounded IPv4 retry
of `ftp.gnu.org` also timed out. The GNU mirror at
`https://mirrors.kernel.org/gnu/groff/groff-1.24.1.tar.gz` succeeded.

Observed SHA-256:

```text
74e2819795b6aff431aeac983d63a9c8968eeaba2a2eba7df8ba4c7b41e7cfd8
```

This is an observed digest, not an independently verified release signature.
The successful initial procedure inside `carta-dev` was:

```bash
curl -fL --connect-timeout 10 --max-time 90 https://mirrors.kernel.org/gnu/groff/groff-1.24.1.tar.gz -o /opt/carta-pdf-bench/groff-1.24.1.tar.gz
sha256sum /opt/carta-pdf-bench/groff-1.24.1.tar.gz
tar -xzf /opt/carta-pdf-bench/groff-1.24.1.tar.gz -C /opt/carta-pdf-bench
mkdir -p /opt/carta-pdf-bench/groff-build-1.24.1
cd /opt/carta-pdf-bench/groff-build-1.24.1
/opt/carta-pdf-bench/groff-1.24.1/configure --prefix=/opt/carta-pdf-bench/groff-1.24.1 > configure.log 2>&1
make -j2 > build.log 2>&1
make install > install.log 2>&1
```

The tool call hit its 200-second timeout, but the container build/install
completed; installation logs and both installed version checks were inspected.
`groff --version` and `gropdf --version` both report **1.24.1**.
`groff/logs/configure.log`, `build.log`, and `install.log` preserve the complete
build logs, including upstream documentation-generation warnings.
`build.sh` reproduces this procedure with bounded download time and a digest
check. Its wrapper was syntax-checked; the successful initial build used the
commands above, not a second execution of the wrapper.

Sources initially share the installation prefix; do not mistake total prefix
size for installed renderer size. Compilation outputs remain in the container's
`/opt/carta-pdf-bench/groff-build-1.24.1`, not the repository.

## Official FontForge Route

Run:

```bash
distrobox enter carta-dev -- bash experiments/pdf-export/groff/install-fonts.sh
```

The wrapper reads the installed upstream
`share/doc/groff-1.24.1/examples/install-font.bash`. Its **only** in-memory
modification is relocating `tmp_dir=/tmp/install-font` to
`/opt/carta-pdf-bench/install-font`, keeping build scratch inside `/opt`.
No shaping, bidi, glyph remapping, metric repair, or conversion substitutions
are applied. Each supplied TTF is passed separately, from the shared fonts
directory, with `-P <prefix>/share/groff -n -d -F Carta -f <name>`.
`-n` prevents copying or modifying the shared TTFs.

For each of nine inputs, upstream FontForge generates a `.pfa`, a `.t42`,
and AFM metrics; `afmtodit -e text.enc` produces the groff metric file
(italic `-i50`, otherwise `-i0 -m`). The AFM is deleted. Type 42 and metrics
are installed in `site-font/devps`; Type 1 is installed in `site-font/devpdf`,
with a symlink to the devps metric. Each device's `download` registry is updated.
Upstream cleans temporary input/mapping symlinks and conversion outputs.

Observed final artifacts:

| Artifact | Count | File bytes |
| --- | ---: | ---: |
| Type 42 `.t42` | 9 | 15,241,345 |
| Type 1 `.pfa` | 9 | 17,565,431 |
| groff metric files | 9 | 9,430,550 |
| download registries and backups | 4 | 1,879 |
| Total regular files | 31 | 42,239,205 |
| PDF metric symlinks | 9 | 576 |

`du -sb site-font`: **42,239,781 bytes**. This includes both PS and PDF assets,
not a minimal PDF-only deployment. Original nine TTFs total **7,755,112 bytes**
and remain in the shared fonts directory. One generated FontForge script,
`generate-t42.pe` (89 bytes), remains in `/opt/carta-pdf-bench/install-font`;
no AFMs remain. Per-font conversion logs are `groff/logs/font-<name>.log`.
The unmodified upstream script appends registry entries on repeated runs, so
these counts describe one conversion pass, not arbitrarily repeated installs.

Conversion warnings include FontForge ignoring `STAT` style attributes,
`afmtodit: warning: cannot open 'DESC': No such file or directory`, and duplicate
Unicode glyph-name mappings, especially Japanese compatibility ideographs.
These were not hidden or repaired. Source Serif metric files are unusually
large (individual sizes in `facts.log`). The PDF font route uses Type 1 subsets,
not the original TrueType/OpenType shaping tables.

## Size And Dependencies

`groff/logs/facts.log` records exact versions, individual artifact sizes,
symlinks, `ldd` output, and `pdfinfo`/`pdffonts` results.

| Measured tree | Apparent bytes (`du -sb`) |
| --- | ---: |
| Installed bin | 13,331,128 |
| Installed lib | 2,428,414 |
| Installed share, including custom fonts | 70,420,805 |
| Installed payload sum | 86,180,347 |
| Installed payload excluding custom site-font tree | 43,940,566 |
| Prefix including co-located sources | 118,167,096 |
| Separate build tree | 56,933,682 |

These are apparent bytes, not allocated disk blocks or a minimal runtime closure.
The full install includes non-PDF tools, documentation, macros, and stock fonts.
Shared system libraries and Perl are excluded from the payload sum.

Rendering pipeline: groff driver, preconv UTF-8 conversion, troff formatter,
gropdf Perl postprocessor. ELF dependencies observed for groff/troff are
libstdc++, libgcc_s, libm, libc, and the ELF loader; preconv additionally links
libuchardet. gropdf is a Perl script, so `ldd gropdf` would not describe its
dependencies: its `/usr/bin/perl` interpreter links libc, libm, libcrypt and
the loader. gropdf imports standard Perl modules including Getopt::Long,
Encode, POSIX, File::Spec and File::Path; optional Compress::Zlib is available
(2.212), with its shared extension linking libz. Optional Inline::C is absent;
gropdf reports that installing it would speed up large-font processing.
No Inline::C installation was performed. Optional Image::Magick is outside
these text-only cases. FontForge, afmtodit and a compiler are font-preparation
dependencies, not required to render already installed fonts. Poppler tools
are inspection dependencies, not renderer dependencies.

## Verification And Limitations

`bash -n` passed for all four shell scripts. Latin smoke rendering succeeded:
`groff/smoke-latin.pdf` is two A4 pages; raster inspection confirmed typography,
indentation, mono code and footer, and extraction contains page numbers 1/2.
After correcting the side/top margin interpretation, all three smoke PDFs,
extracted texts, raster previews and `facts.log` were regenerated inside
`carta-dev`. `logs/smoke-latin-bbox.html` records a 90.708 pt left edge
(32 mm) and footer glyphs centered at 297.637 pt; their vertical bounds are
793.370-801.938 pt, consistent with a baseline near 282 mm.
The Unicode cases are deliberately **separate**:

- `smoke-unicode.roff` / `.pdf`: Source Serif baseline, no fallback.
- `smoke-unicode-explicit.roff` / `.pdf`: explicit NS/NH/NA/ND/NJ font selection.
- Matching `.stderr.log` files retain all renderer warnings/notices.
- `logs/smoke-*.txt` retains Poppler extraction; `.png` files retain first-page rasterizations.

Baseline warnings include missing U+FB01/U+FB03 ligatures and Hebrew, Arabic,
Devanagari and Japanese characters. Those glyphs disappear; Greek and Cyrillic
are present in Source Serif. Explicit selection produces glyphs, but **not a
correct multilingual renderer**: Hebrew/Arabic stay in logical left-to-right
visual order, Arabic letters are unjoined, Devanagari positioning/shaping is
not reliable, and Japanese `U+65E5` disappears with a missing-glyph warning
even in the explicit case (29 baseline warnings, one explicit-font warning).
Raster inspection showed `日本語。` becoming
`本語。`. Duplicate conversion mappings are a possible contributor, not an
established diagnosis. Combining-accent extraction alone does not establish
correct positioning. No custom shaping hacks or reordered input were used.

Poppler reports embedded, subsetted Type 1 fonts with custom encoding and
`uni=no` (no ToUnicode map). PDFs are untagged. Extraction may appear better
than visual order for RTL text because Poppler applies its own processing;
it is not evidence of correct groff bidi behavior. No PDF/A or accessibility
conformance is claimed.

At this report's initial completion, the main agent's `groff/latin.roff` and
`groff/unicode.roff` had not yet appeared. Only the isolated smoke inputs were
rendered; do not treat these results as completed shared-corpus benchmarking.
Once those inputs exist, invoke the same renderer interface above. No Cargo
validation applies: no Rust/application source was changed.

## PDF-Only Packaging Measurement

The shared corpus inputs and main-agent results subsequently became available.
`groff/measure-packaging.sh` traces the shared Latin input and the native fallback
Unicode diagnostic, builds an exact manifest, and tests a copied runtime tree
under `/opt/carta-pdf-bench/groff-pdf-runtime-measured`. It does **not** modify
or remove anything in the installed groff tree. `strace` (and its libunwind
dependency) was installed only inside the verified Debian `carta-dev` container.
The script and its Perl manifest helper passed syntax checks and executed.

Run:

```bash
distrobox enter carta-dev -- bash experiments/pdf-export/groff/measure-packaging.sh
```

The exact per-file manifest and successful copied-tree checks are in
`experiments/pdf-export/results/groff-packaging.txt`; syscall logs are in the
`TRACE_DIRECTORY` named by that report. Sizes are dereferenced regular-file
bytes, **unstripped**, excluding directory overhead and shared libraries.
This is an observed text-benchmark closure estimate, not an absolute minimum
or a portable, fully isolated bundle.

| PDF-only component | Files | Bytes |
| --- | ---: | ---: |
| groff | 1 | 434,496 |
| preconv | 1 | 209,480 |
| troff | 1 | 3,489,296 |
| gropdf (Perl script) | 1 | 109,835 |
| PDF startup device assets and macros | 48 | 620,477 |
| Base PDF core total | 52 | 4,863,584 |
| Custom PDF Type 1 outlines | 9 | 17,565,431 |
| Custom metrics, dereferenced into devpdf | 9 | 9,430,550 |
| Custom PDF download registry | 1 | 425 |
| Custom PDF font closure total | 19 | 26,996,406 |
| carta.tmac | 1 | 1,205 |
| Core + custom fonts + carta.tmac | 72 | 31,861,195 |

All nine fonts are counted because the macro package mounts them all. No Type
42 files, devps download registry, download backups, AFMs, FontForge tools,
shared input TTFs, documentation, or non-PDF executables are needed in this
configured PDF-only set. Flattening the original devpdf metric symlinks avoids
retaining their devps parent tree. A narrower Latin-only deployment or stripped
binaries could be smaller, but neither is claimed as measured here.

`devpdf/DESC` (167 bytes) is required by the driver, formatter and postprocessor.
It specifies `fonts 8 0 0 0 0 0 SS S ZD`, with family T; stock SS/S/ZD and TR
metrics are opened at startup before `carta.tmac` remounts font positions.
`ps.tmac` also inspects stock CBI and the standard PS-family metric files.
Their **metrics**, plus the stock PDF download registry, remain in the manifest
to preserve normal initialization without pruning warnings. No stock outline
file was opened by these text cases. This is not a guarantee for arbitrary
roff using additional stock symbols, figures, tables, or other macro packages.

The 14 startup macro/data files actually opened are `troffrc`, `troffrc-end`,
`composite.tmac`, `fallbacks.tmac`, `pdf.tmac`, `ps.tmac`, `europs.tmac`,
`en.tmac`, `latin1.tmac`, `hyphen.en`, `hyphenex.en`, `papersize.tmac`,
`pspic.tmac`, and `pdfpic.tmac`. Some are loaded even though these cases do not
use their features and `carta.tmac` disables hyphenation. They are not silently
omitted from the measured estimate.

Perl and system dependencies are **additional**, reported separately:

| External component | Files | Bytes |
| --- | ---: | ---: |
| /usr/bin/perl | 1 | 3,935,376 |
| Observed Perl modules and XS extensions | 62 | 1,193,293 |
| Interpreter + observed modules | 63 | 5,128,669 |
| Observed ABI-specific ELF libraries + loader | 8 | 6,413,536 |

The eight ELF files are libc, libcrypt, libgcc_s, libm, libstdc++, libuchardet,
libz, and ld-linux-x86-64. Perl's XS extensions are already in the module row,
not counted twice as system libraries. Locale and loader caches, timezone data,
and the entire Perl distribution are not included in those subtotal figures;
the manifest records observed external opens. Do not present 31.9 MB as the
whole runtime dependency closure on a machine without Perl or these libraries.

Copied-tree Latin and fallback PDFs pass qpdf checking and have identical raw
text extraction to their installed-tree counterparts. However, gropdf appends
its compiled search roots even when `GROFF_FONT_PATH` points to the copied tree.
The trace records successful opens of the **two original download registries**;
all other groff assets came from the copy. Therefore this test is not proof
that the original install can be removed. A deployment must preserve its prefix
or regenerate/rebuild the compiled paths. No installed paths were changed to
conceal this limitation.

## Native Fallback Diagnostic

Native configured fallback **exists**. The separate prelude
`groff/unicode-fallback.roff` contains:

```roff
.fspecial CR NS NH NA ND NJ
```

This per-font list is tried for glyphs absent from CR, in the specified order;
it does not override glyphs already present in CR. groff also provides `.special`
for global special-font configuration; that alternative was not needed here.
The baseline macro package and main-agent baseline/explicit PDFs are unchanged.

The measurement script concatenates this prelude and the shared Unicode input
into `/opt/carta-pdf-bench/unicode-fallback-combined.roff`. This ensures both
pass through preconv: including raw UTF-8 via `.so` after preconv would instead
cause mojibake, not a meaningful fallback test.

Render the generated diagnostic with:

```bash
distrobox enter carta-dev -- bash experiments/pdf-export/groff/render.sh /opt/carta-pdf-bench/unicode-fallback-combined.roff experiments/pdf-export/results/groff-unicode-fallback.pdf
```

`groff-unicode-fallback.pdf` is one A4 page and passes qpdf checking. Matching
pdfinfo, pdffonts, raw/layout text, and stderr files are in the results directory;
`results/preview/groff-unicode-fallback-1.png` preserves the raster. Most missing
Hebrew/Arabic/Devanagari/Japanese glyphs now appear, including mixed-script
paragraphs. The only missing-glyph warning in this shared fallback case is
U+65E5 (`日`). Automatic glyph fallback does **not** supply bidi or shaping:
raster inspection still shows left-to-right Hebrew/Arabic visual sequencing,
unjoined Arabic, and unreliable Devanagari positioning. No text reordering,
glyph repairs, or shaping hacks were introduced.

The CJK failure is localized to the official conversion's mapping: read-only
FontForge inspection of the original TTF reports **both U+65E5 and U+2F47 as
glyph `uni2F47`**. The generated NJ metrics have a `u2F47` entry and no `u65E5`
entry. Logs are `groff/logs/cjk-cmap-inspection-corrected.log` and
`cjk-metric-inspection.txt`. This explains why explicit NJ selection and native
fallback both lose `日` despite the source font having its outline. No repair
was applied.

The shared baseline and fallback extraction also turn Greek mu U+03BC into
micro sign U+00B5 in `Καλημέρα κόσμε`; the explicit Noto Greek case extracts
U+03BC correctly. CR's metrics contain U+03BC with glyph name `mu` (recorded in
`greek-metric-inspection.txt`), and these PDFs lack ToUnicode maps. This is
consistent with glyph-name-based extraction assigning `mu` to U+00B5, not
evidence that Greek mu was unavailable to the formatter. Native fallback leaves
this existing CR glyph alone, so it does not resolve the extraction mismatch.
The main renderer recommendation remains outside this diagnostic's scope.

## Footer Trap Correction

The final label-only Latin corpus exposed a **macro-package bug**, not an
engine limitation: body text overlapped the footer and continued below it.
The previous `.bp` after restoring the body environment could break buffered
body text on the outgoing page. A no-break `'bp` alone did not fully resolve
interrupted-paragraph behavior, so the final minimal correction is:

- The page eject `.bp` runs while the empty footer environment is still active;
  `.ev` restores the buffered body environment only afterwards.
- The top trap uses no-break `'sp`, avoiding an accidental paragraph break.
- The body trap is at `-(2.5c+14.85p+6p)`, reserving one body leading plus glyph
  descent before the **272 mm ink limit**. The 6 pt allowance covers the measured
  Latin serif and code-font descent; baseline coordinates alone are not an ink bound.
- Headings reserve `.ne 90p`, covering the largest single-line heading, leading
  space, trailing space, the following paragraph gap, and two body lines.

No engine patches or full widow/orphan system were introduced. The oversized
unbreakable word still overflows horizontally; that separate limitation remains
visible rather than being repaired as part of this pagination fix.

The main benchmark outputs were left for the main agent's final rerun. Its old
Latin PDF was copied to `groff/latin-before-trap-fix.pdf`. The bounding-box checker
rejects that PDF on page 1 (`volutamente`, yMax 785.394 pt, below 272 mm), proving
the check detects the reported regression. The corrected diagnostic is
`groff/latin-trap-fixed.pdf`; all three PNG pages were visually inspected.

| Corrected Latin page | Maximum body glyph yMax |
| --- | ---: |
| 1 | 246.576 mm |
| 2 | 256.936 mm |
| 3 | 207.168 mm |

`groff/check-page-bounds.pl` verifies **every word on every page**, excluding
exactly one centered footer number, against the 272 mm limit. It passed the
three-page Latin corpus, a two-page paragraph starting at 260 mm, and a seven-page
dense repeated paragraph. All twelve checked pages contain one centered footer
and no body text below it. The dense test's maximum yMax is **269.847 mm**, and
its body counts total **3,840 words**, matching 30 copies of the 128-word source
paragraph. All three corrected diagnostic PDFs pass qpdf checking. Bounds and
extraction logs are under `groff/logs/`, including `latin-trap-fixed.bounds.txt`,
`smoke-trap-trap-fixed.bounds.txt`, and `trap-long-paragraph.bounds.txt`.

All rendering, inspection and validation ran inside `carta-dev`.
`measure-packaging.sh` was rerun: the macro grew by **85 bytes**, from 1,120 to
**1,205 bytes**. The measured runtime remains **72 files**, now
**31,861,195 bytes**; base core, custom fonts, Perl and ELF subtotals are unchanged.
