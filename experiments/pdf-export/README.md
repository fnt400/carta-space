# PDF Export: Esperimento Isolato

Confronto empirico GNU groff/gropdf **1.24.1** e printpdf **0.12.8**, eseguito
il 2026-10-06. Nessuna integrazione o dipendenza aggiunta a Carta.

Il risultato e la raccomandazione sono in [results/REPORT.md](results/REPORT.md).
L'inventario completo e' [results/FILES.txt](results/FILES.txt).

## Ambiente

Tutte le installazioni, conversioni, compilazioni e verifiche sono state
eseguite nella Distrobox Debian 13 `carta-dev`. Nessuna installazione sull'host,
nessun uso di Nix, nessuna modifica a `main` o ai manifest principali.
Toolchain Rust 1.94.0 isolata, Rust homes e target sotto `/opt/carta-pdf-bench`.
L'eseguibile groff di sistema non e' stato sostituito.

Pacchetti richiesti:

```bash
distrobox enter carta-dev -- sudo apt-get install -y build-essential curl ca-certificates pkg-config fontforge poppler-utils qpdf time wget bison flex texinfo libuchardet-dev python3-fonttools unzip strace
```

I pacchetti effettivamente nuovi, incluse le dipendenze transitive, sono
registrati in `results/apt-installed.txt`. `python3-pil` e `libraqm0` arrivano
come dipendenze transitive e servono solo al riferimento raster indipendente.
Non sono dipendenze runtime del compositore printpdf.

## Font

Provenienza, release/commit, licenze OFL, file, versioni interne e SHA-256:
[fonts/README.md](fonts/README.md) e [fonts/manifest.json](fonts/manifest.json).

- Source Serif 4 4.005: Regular, Italic, Semibold statici ufficiali Adobe.
- Source Code Pro 2.042: Regular statico ufficiale Adobe.
- Noto Sans 2.008: greco e cirillico per la prova esplicita.
- Noto Sans Hebrew 3.000, Arabic 2.009, Devanagari 2.002.
- Noto Sans JP 2.004: TTF ufficiale istanziato a `wght=400` con FontTools.

Source Serif non e' un font universale. Il fallback e lo shaping sono parte
del test. I nove TTF occupano 7.755.112 byte; il set occidentale quattro TTF
occupa 932.076 byte. Le quattro licenze upstream sono conservate in `fonts/`.

## Riproduzione

Preparare i font e groff tramite gli script isolati:

```bash
distrobox enter carta-dev -- python3 experiments/pdf-export/fonts/prepare.py
distrobox enter carta-dev -- bash experiments/pdf-export/groff/build.sh
distrobox enter carta-dev -- bash experiments/pdf-export/groff/install-fonts.sh
distrobox enter carta-dev -- bash experiments/pdf-export/printpdf/build.sh
```

`printpdf/build.sh` presuppone la toolchain isolata gia' disponibile. La sua
installazione iniziale non modifica il profilo shell o la toolchain di Carta:

```bash
distrobox enter carta-dev -- bash -lc 'curl -fL https://sh.rustup.rs -o /opt/carta-pdf-bench/rustup-init.sh && RUSTUP_HOME=/opt/carta-pdf-bench/rustup CARGO_HOME=/opt/carta-pdf-bench/cargo bash /opt/carta-pdf-bench/rustup-init.sh -y --no-modify-path --profile minimal --default-toolchain 1.94.0 --component rustfmt --component clippy'
```

Generare input equivalenti, PDF, verifiche, preview e tre ripetizioni per scala:

```bash
distrobox enter carta-dev -- python3 experiments/pdf-export/benchmark/prepare.py
distrobox enter carta-dev -- python3 experiments/pdf-export/benchmark/run.py
distrobox enter carta-dev -- python3 experiments/pdf-export/benchmark/verify.py
distrobox enter carta-dev -- python3 experiments/pdf-export/benchmark/collect.py
```

Il converter e' intenzionalmente piccolo: riconosce soltanto i costrutti del
corpus. Non e' CommonMark completo. Il link ha la stessa etichetta nei due
output, ma groff non crea un'annotazione PDF cliccabile. Il corpus Latin e'
su tre pagine per entrambi, senza page break imposti.

Groff: `contenuto -> roff semantico -> carta.tmac -> groff/gropdf`.
Printpdf: `contenuto -> HTML semantico -> classic.css -> motore HTML/CSS`.
Il profilo richiesto e' lo stesso, ma i difetti del motore non vengono
mascherati. `unicode.pdf` e' il caso baseline; `unicode-explicit.pdf` diagnostica
la selezione per script. `groff-unicode-fallback.pdf` usa il fallback nativo
`.fspecial CR NS NH NA ND NJ`. `printpdf-rtl.pdf` imposta `dir`/`direction`.

## Evidenze

- `results/*.pdf`: quattro documenti principali, diagnostiche e prove di scala.
- `results/*.pdfinfo.txt`, `*.pdffonts.txt`, `*.qpdf.txt`: controlli PDF.
- `results/*.layout.txt`, `*.raw.txt`: estrazione Poppler nei due modi.
- `results/preview/`: raster a 120 dpi di tutte le pagine del corpus comune.
- `results/performance.csv`: tre campioni per motore e scala, CPU/RSS/byte.
- `results/performance-summary.json`: mediane e conteggi reali delle pagine.
- `results/extraction-checks.json`: controlli esatti, anche quelli falliti.
- `groff/logs/`, `printpdf/logs/`: build, font, dipendenze e tracciamento runtime.
- `results/groff-packaging.txt`: stima PDF-only e limite di relocazione.
- `results/printpdf-followup.md`: subset reali, Semibold esplicito e font scan.

Le note dei singoli motori includono prove preliminari, precedenti ai corpus
comuni. `REPORT.md` descrive i risultati finali. Nessun PDF rotto e' sostituito
con l'output di un altro motore. Il riferimento RAQM e' solo un'immagine per
confrontare shaping e ordine, non un terzo renderer PDF.

## Limiti

Misure locali a cache calda, tre ripetizioni, non uno studio statistico.
Stesso contenuto, non stesso numero esatto di pagine: groff produce 1/9/82,
printpdf 1/10/100. GNU time riporta il massimo RSS dei processi figli, non la
somma simultanea della pipeline groff. Nessuna conformance Unicode completa,
PDF/A, PDF/X, accessibilita', packaging macOS o glibc precedente verificata.

L'esperimento si ferma qui. La decisione sul motore precede qualsiasi
integrazione nell'applicazione.
