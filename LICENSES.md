# Carta Space — Licensing

Carta Space deliberately separates the license of the reference software from the freedom to implement and reuse the format documentation.

## Reference implementation — GPL-3.0-or-later

Unless a file states otherwise, the Carta Space reference implementation is licensed under the **GNU General Public License v3.0 or later** (`GPL-3.0-or-later`).

This includes the Rust crates, executable frontends, development experiments, shell scripts, and build metadata that make up the reference implementation.

The complete GPLv3 license text is in the repository root as `LICENSE`. The `GPL-3.0-or-later` designation means that the covered software may be redistributed and/or modified under GPL version 3 or, at the recipient's option, any later version published by the Free Software Foundation.

## Original project documentation — CC0-1.0

Unless a file states otherwise, original Carta Space project documentation is dedicated under **Creative Commons CC0 1.0 Universal** (`CC0-1.0`).

This includes the format specification, white paper, interaction contract, design-decision records, README, and project/development documentation.

The complete CC0 legal code is in `LICENSES/CC0-1.0.txt`.

The intent is that the Carta Space format can be described, quoted, adapted, and independently implemented without imposing the GPL on independent readers, writers, tools, or applications.

## Third-party fonts — SIL Open Font License 1.1

Carta's PDF publication backend embeds Source Serif 4, Source Code Pro, and selected Noto Sans families so publication is deterministic and does not depend on system fonts. These font files are distributed under the SIL Open Font License 1.1.

The corresponding license texts are:

- `LICENSES/OFL-source-serif.txt`;
- `LICENSES/OFL-source-code-pro.txt`;
- `LICENSES/OFL-noto-fonts.txt`;
- `LICENSES/OFL-noto-cjk.txt`.

The fonts remain third-party works; embedding them in Carta's binary does not relicense them under GPL or CC0.

## Third-party material

Third-party standards, names, code, excerpts, and other material remain subject to their own copyright, license, and trademark terms. A reference to an external standard does not relicense that standard under CC0.

## Names and trademarks

These copyright licenses do not grant trademark rights. Carta Space v0.1 does not define a separate trademark policy.
