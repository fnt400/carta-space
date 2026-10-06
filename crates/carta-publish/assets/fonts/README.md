# Embedded PDF fonts

Carta Space embeds this controlled font set for deterministic PDF publication.
At export time the fonts are materialized into a temporary private directory
and Typst is invoked with system and embedded Typst fonts disabled.

The files are the same pinned font binaries validated by
experiments/pdf-export/fonts/manifest.json:

- Source Serif 4 4.005: Regular, Italic, Semibold;
- Source Code Pro 2.042: Regular;
- Noto Sans 2.008: Regular;
- Noto Sans Hebrew 3.000: Regular;
- Noto Sans Arabic 2.009: Regular;
- Noto Sans Devanagari 2.002: Regular;
- Noto Sans JP 2.004: Regular.

All are distributed under the SIL Open Font License 1.1. License texts are in
the repository-level LICENSES/ directory.
