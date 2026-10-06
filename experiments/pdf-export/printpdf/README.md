# Isolated printpdf Experiment

Run every command in `carta-dev`. No dependency on the main Cargo workspace.

```bash
distrobox enter carta-dev -- bash experiments/pdf-export/printpdf/build.sh
distrobox enter carta-dev -- /opt/carta-pdf-bench/printpdf-target/release/carta-printpdf-bench INPUT.html OUTPUT.pdf
distrobox enter carta-dev -- bash experiments/pdf-export/printpdf/benchmark.sh INPUT_DIRECTORY
```

`INPUT_DIRECTORY` contains `latin.html`, `unicode.html`, `perf-1.html`,
`perf-10.html`, and `perf-100.html` supplied by the benchmark coordinator.
Inputs and shared fonts are never rewritten. Output PDFs and inspection text
remain in `outputs/`; raw evidence remains in `logs/`.

The exact Rust toolchain is 1.94.0. Both Rust homes and the target directory
are under `/opt/carta-pdf-bench`; the scripts never install tools. The initial
rustup installation used `--no-modify-path` and the minimal profile plus
rustfmt and Clippy. All nine shared TTF fonts are embedded with `include_bytes!`.

`classic.css` is inserted at the end of the HTML head. A4 dimensions and
25 mm vertical / 32 mm horizontal margins use the engine's programmatic API,
not unsupported CSS `@page` rules. Native page numbering is requested, with
no custom PDF drawing or replacement renderer. Font pools explicitly disable
system fonts from resolution so fallback is tested against the packaged font set.
The fontconfig implementation still scans system configuration/directories;
its empty family filter rejects all discovered system faces.

The engine's `XmlRenderOptions::fonts` map accepts raw TTF bytes and registers
both the map keys and internal font family names. This API is documented in
`printpdf-0.12.8/src/html/mod.rs`; the crate's
`tests/html_font_resolution.rs` verifies the corresponding `from_html` map.

See `experiments/pdf-export/results/printpdf-notes.md` for results and limitations. An exit status of
zero means the engine produced bytes, not that the publication profile was met.
