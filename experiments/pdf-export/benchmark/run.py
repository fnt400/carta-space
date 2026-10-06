#!/usr/bin/env python3
"""Run only inside carta-dev; subprocess time excludes compilation."""
import csv
import json
import os
import statistics
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RESULTS = ROOT / "results"
BIN = "/opt/carta-pdf-bench/printpdf-target/release/carta-printpdf-bench"


def render(engine, name, output):
    source = ROOT / "benchmark" / "generated" / (name + (".roff" if engine == "groff" else ".html"))
    return ["bash", str(ROOT / "groff" / "render.sh"), str(source), str(output)] if engine == "groff" else [BIN, str(source), str(output)]


def inspect(pdf):
    for tool, args in [("pdfinfo", []), ("pdffonts", []), ("qpdf", ["--check"])]:
        proc = subprocess.run([tool, *args, str(pdf)], capture_output=True, text=True)
        pdf.with_suffix(f".{tool}.txt").write_text(proc.stdout + proc.stderr + f"\nexit_status={proc.returncode}\n")
    for mode in ("layout", "raw"):
        subprocess.run(["pdftotext", "-" + mode, str(pdf), str(pdf.with_suffix(f".{mode}.txt"))], check=True)
    info = subprocess.check_output(["pdfinfo", str(pdf)], text=True)
    pages = int(next(x.split(":")[1] for x in info.splitlines() if x.startswith("Pages:")))
    preview = RESULTS / "preview"
    preview.mkdir(exist_ok=True)
    # All pages for quality corpus, first page for scaling artifacts.
    if "perf" not in pdf.stem:
        subprocess.run(["pdftoppm", "-r", "120", "-png", str(pdf), str(preview / pdf.stem)], check=True)
    return pages


def main():
    if os.environ.get("CONTAINER_ID") != "carta-dev":
        raise SystemExit("Run via distrobox enter carta-dev")
    RESULTS.mkdir(exist_ok=True)
    records = []
    for engine in ("groff", "printpdf"):
        names = ["latin", "unicode", "unicode-explicit"] + (["rtl"] if engine == "printpdf" else [])
        for name in names:
            output = RESULTS / f"{engine}-{name}.pdf"
            proc = subprocess.run(render(engine, name, output), capture_output=True, text=True)
            output.with_suffix(".render.txt").write_text(proc.stdout + proc.stderr + f"\nexit_status={proc.returncode}\n")
            if proc.returncode == 0:
                inspect(output)
    # Sequential engine measurements avoid CPU competition, three repeats each.
    for engine in ("groff", "printpdf"):
        for size in (1, 10, 100):
            name = f"perf-{size}"
            output = RESULTS / f"{engine}-{name}.pdf"
            for repeat in range(1, 4):
                timing = RESULTS / f"{engine}-{name}-run{repeat}.time.txt"
                start = time.monotonic()
                proc = subprocess.run(["/usr/bin/time", "-f", "%e,%U,%S,%M", "-o", str(timing), *render(engine, name, output)], capture_output=True, text=True)
                elapsed = time.monotonic() - start
                (RESULTS / f"{engine}-{name}-run{repeat}.render.txt").write_text(proc.stdout + proc.stderr)
                if proc.returncode:
                    raise RuntimeError(proc.stderr)
                wall, user, system, rss = timing.read_text().strip().split(",")
                pages = int(next(x.split(":")[1] for x in subprocess.check_output(["pdfinfo", str(output)], text=True).splitlines() if x.startswith("Pages:")))
                records.append(dict(engine=engine, target_pages=size, actual_pages=pages, repeat=repeat, wall_s=float(wall), precise_wall_s=elapsed, user_s=float(user), system_s=float(system), peak_rss_kib=int(rss), pdf_bytes=output.stat().st_size))
            inspect(output)
    with (RESULTS / "performance.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(records[0]))
        writer.writeheader()
        writer.writerows(records)
    summary = []
    for engine in ("groff", "printpdf"):
        for size in (1, 10, 100):
            group = [r for r in records if r["engine"] == engine and r["target_pages"] == size]
            summary.append({"engine": engine, "target_pages": size, "actual_pages": group[0]["actual_pages"], **{key: statistics.median(r[key] for r in group) for key in ("wall_s", "precise_wall_s", "user_s", "system_s", "peak_rss_kib", "pdf_bytes")}})
    (RESULTS / "performance-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
