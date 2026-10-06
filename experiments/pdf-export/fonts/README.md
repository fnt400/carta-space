# PDF Experiment Fonts

Explicit, repository-local TrueType inputs for the isolated PDF export experiment.
No system-font discovery or substitution is intended. These are font inputs, not
a change to Carta's archive format or production rendering policy.

## Inputs

| File | Bytes | Version | Intended use |
| --- | ---: | --- | --- |
| `SourceSerif4-Regular.ttf` | 261,868 | 4.005 | Body text |
| `SourceSerif4-Italic.ttf` | 187,808 | 4.005 | Body italic (upstream filename `SourceSerif4-It.ttf`) |
| `SourceSerif4-Semibold.ttf` | 272,088 | 4.005 | Emphasis/headings |
| `SourceCodePro-Regular.ttf` | 210,312 | 2.042 | Monospaced code |
| `NotoSans-Regular.ttf` | 569,208 | 2.008 | Greek and Cyrillic fallback |
| `NotoSansHebrew-Regular.ttf` | 26,900 | 3.000 | Hebrew fallback |
| `NotoSansArabic-Regular.ttf` | 240,456 | 2.009 | Arabic fallback |
| `NotoSansDevanagari-Regular.ttf` | 219,212 | 2.002 | Devanagari fallback |
| `NotoSansJP-Regular.ttf` | 5,767,260 | 2.004 | Japanese fallback (official JP subset) |

All nine inputs are ready; total font size is 7,755,112 bytes. Full version
name-table strings and exact SHA-256 digests are in `manifest.json`.

Source Serif is not a universal-script font. Use explicit script-aware fallback
rather than assuming all characters exist in the body face. Glyph coverage alone
does not implement Arabic/Indic shaping, bidi layout, line breaking, or prove
correct PDF rendering. The JP font is the upstream Japanese subset, not a promise
of universal CJK coverage. No corpus-dependent subsetting is applied here.

## Provenance And Licenses

`manifest.json` records exact immutable upstream URLs, repository commits, release
tags where applicable, font name-table version strings, source and final SHA-256
digests and byte sizes, copied licenses, embedding flags, and conversion details.
Upstream sources are Adobe's `source-serif` and `source-code-pro` and the official
`notofonts/noto-fonts` and `notofonts/noto-cjk` repositories. Noto's non-CJK files
come from the pinned repository's hinted static TTF distribution.

Eight inputs are copied byte-for-byte from official static TTFs. Source Serif's
italic filename is normalized locally without altering the font. The JP input
is an official variable TrueType font, instantiated with FontTools at `wght=400`
for Regular with updated style names; all axes are pinned and `fvar` is removed.
Its upstream axis default is recorded in the manifest; Regular is chosen
explicitly rather than assuming the variable default is Regular. No CFF-to-glyf
conversion, FontForge rebuilding, or outline simplification is performed.

The four `OFL-*.txt` files are verbatim upstream licenses (Adobe originals use
Markdown). All inputs use SIL OFL 1.1. Preserve these notices when redistributing
the fonts. Consult the licenses for modified-font and reserved-name conditions.

## Reproduction

Only inside the existing Debian development container, from the repository root:

```sh
distrobox enter carta-dev -- python3 experiments/pdf-export/fonts/prepare.py
```

The script requires the already installed `python3-fonttools`. All downloads and
transforms stage under `/opt/carta-pdf-bench/pdf-export-fonts` inside the container;
only final fonts, license copies, and generated manifest are placed here. No
source archives, build dependencies, or system fonts are placed in the repository.
No host installation or Nix environment is used. The manifest is generated from
the actual final files, not hand-entered checksums. The script fails if script
smoke-test characters are absent, variable axes remain, TrueType outlines are
absent, or embedding restrictions are set.
