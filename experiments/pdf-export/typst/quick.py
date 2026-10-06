#!/usr/bin/env python3
"""One run per long input. Record raw evidence, not a full benchmark suite."""
import hashlib
import json
import os
import re
import subprocess
import unicodedata
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
RESULTS = ROOT / "results"
BIN = Path("/opt/carta-pdf-bench/typst-0.15.1/typst")


def run(command):
    proc = subprocess.run(command, capture_output=True, text=True)
    return proc.stdout + proc.stderr + f"\nexit_status={proc.returncode}\n"


def clean(value):
    return re.sub(r"\s+", " ", "".join(c for c in value if unicodedata.category(c) != "Cf")).strip()


def main():
    if os.environ.get("CONTAINER_ID") != "carta-dev":
        raise SystemExit("Run inside carta-dev")
    packaging = [run([str(BIN), "--version"]), run(["ldd", str(BIN)]), run(["readelf", "-l", str(BIN)]),
                 run([str(BIN), "fonts", "--font-path", str(ROOT / "fonts"), "--ignore-system-fonts", "--ignore-embedded-fonts"])]
    packaging.append(f"binary_bytes={BIN.stat().st_size}\nfont_bytes={sum(p.stat().st_size for p in (ROOT / 'fonts').glob('*.ttf'))}\n")
    archive = BIN.parent / "typst.tar.xz"
    packaging.append("official_archive_sha256=" + hashlib.sha256(archive.read_bytes()).hexdigest() + "\n")
    (RESULTS / "typst-packaging.txt").write_text("\n".join(packaging))
    measurements = []
    for name in ("latin", "unicode", "long-100", "long-500"):
        pdf = RESULTS / f"typst-{name}.pdf"
        command = ["bash", str(HERE / "render.sh"), str(HERE / f"{name}.typ"), str(pdf)]
        timing = RESULTS / f"typst-{name}.time.txt"
        if name.startswith("long"):
            command = ["/usr/bin/time", "-v", "-o", str(timing), *command]
        proc = subprocess.run(command, capture_output=True, text=True)
        (RESULTS / f"typst-{name}.render.txt").write_text(proc.stdout + proc.stderr + f"\nexit_status={proc.returncode}\n")
        if proc.returncode:
            raise SystemExit(proc.stderr)
        info = run(["pdfinfo", str(pdf)])
        (RESULTS / f"typst-{name}.pdfinfo.txt").write_text(info)
        pages = int(re.search(r"^Pages:\s+(\d+)", info, re.M)[1])
        measurements.append(dict(input=name, pages=pages, pdf_bytes=pdf.stat().st_size))
        if not name.startswith("long"):
            for tool, args in [("qpdf", ["--check"]), ("pdffonts", [])]:
                (RESULTS / f"typst-{name}.{tool}.txt").write_text(run([tool, *args, str(pdf)]))
            for mode in ("layout", "raw"):
                subprocess.run(["pdftotext", "-" + mode, str(pdf), str(RESULTS / f"typst-{name}.{mode}.txt")], check=True)
            subprocess.run(["pdftotext", "-bbox-layout", str(pdf), str(RESULTS / f"typst-{name}.bbox.html")], check=True)
            subprocess.run(["pdftoppm", "-r", "120", "-png", str(pdf), str(RESULTS / "preview" / f"typst-{name}")], check=True)
    paragraphs = (ROOT / "corpus" / "unicode.md").read_text().strip().split("\n\n")
    samples = [s for s in paragraphs if not s.startswith("#")]
    checks = []
    for mode in ("layout", "raw"):
        text = (RESULTS / f"typst-unicode.{mode}.txt").read_text()
        checks.append(dict(mode=mode, samples=[dict(input=s, exact_substring=clean(s) in clean(text), nfc_substring=unicodedata.normalize("NFC", clean(s)) in unicodedata.normalize("NFC", clean(text))) for s in samples]))
    (RESULTS / "typst-extraction-checks.json").write_text(json.dumps(checks, ensure_ascii=False, indent=2) + "\n")
    (RESULTS / "typst-quick-artifacts.json").write_text(json.dumps(measurements, indent=2) + "\n")
    print(json.dumps(measurements, indent=2))


if __name__ == "__main__":
    main()
