Carta Space v0.2 - portable Windows x64 preview
================================================
Works on Windows 10/11 x64 when application execution is allowed.

1. Unzip the entire CartaSpace folder into a folder you can write to.
2. Double-click Start-Carta.cmd (do not run carta-gui.exe directly).
3. The default Archive is stored in data\carta\archive, next to the program.
4. On first launch, create an Archive or import one from a Git URL.

No administrator rights, MSI, registry writes or system PATH edits are
required. The launcher changes PATH and XDG_DATA_HOME for this invocation
only. All documents and local session data are under the extracted folder.
For portability, move the whole folder together. Do not delete data.

Included:
- carta-gui.exe: graphical editor; bundled Iosevka font
- carta-cli.exe: optional administrative tool
- git\: MinGit (Git for Windows); history and synchronization
- tools\typst.exe: Typst, for PDF export

Spell checking is optional and currently requires Hunspell installed
separately or a compatible portable hunspell.exe added to tools\.
The spelling dictionaries themselves download at first use.

Important limitations:
- This is a development preview, not a stable Windows release.
- Corporate execution restrictions (AppLocker, Defender policy, etc.) may
  block unsigned software. Do not bypass your employer's security policy.
- Internet access or a Git remote may be blocked by workplace networking.
- Windows directory durability is not identical to Unix directory fsync;
  file contents are explicitly flushed before atomic replacements.
- Keep backups. Verify a successful push before editing the same archive
  on another computer.

Third-party software:
- MinGit: https://github.com/git-for-windows/git
  License: GPL-2.0; license information is included with MinGit.
- Typst: https://github.com/typst/typst
  License: Apache-2.0 OR MIT.
- Iosevka: https://github.com/be5invis/Iosevka
  License: SIL Open Font License (see repository LICENSES/OFL-iosevka.txt).
