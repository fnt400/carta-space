#!/usr/bin/env python3
"""Collect environment, package transaction and artifact inventory evidence."""
import hashlib
import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RESULTS = ROOT / "results"


def main():
    if os.environ.get("CONTAINER_ID") != "carta-dev":
        raise SystemExit("Run inside carta-dev")
    commands = [["uname", "-a"], ["lscpu"], ["free", "-h"], ["dpkg-query", "-W", "fontforge", "poppler-utils", "qpdf", "time", "python3-fonttools", "python3-pil", "strace", "perl", "libc6", "gcc", "bison", "flex", "texinfo", "libuchardet-dev"], ["/opt/carta-pdf-bench/groff-1.24.1/bin/groff", "--version"], ["/opt/carta-pdf-bench/groff-1.24.1/bin/gropdf", "--version"]]
    environment = []
    for command in commands:
        proc = subprocess.run(command, capture_output=True, text=True)
        environment.append("$ " + " ".join(command) + "\n" + proc.stdout + proc.stderr)
    (RESULTS / "environment.txt").write_text("\n".join(environment))
    history = Path("/var/log/apt/history.log").read_text()
    transactions = [block for block in history.split("\n\n") if "Start-Date: 2026-10-06" in block and ("fontforge" in block or "strace" in block)]
    (RESULTS / "apt-installed.txt").write_text("\n\n".join(transactions) + "\n")
    artifacts = []
    for path in sorted(RESULTS.glob("*.pdf")):
        info = subprocess.check_output(["pdfinfo", str(path)], text=True)
        pages = int(next(line.split(":")[1] for line in info.splitlines() if line.startswith("Pages:")))
        artifacts.append({"file": path.name, "bytes": path.stat().st_size, "pages": pages, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    (RESULTS / "pdf-artifacts.json").write_text(json.dumps(artifacts, indent=2) + "\n")
    inventory = RESULTS / "FILES.txt"
    files = sorted(p for p in ROOT.rglob("*") if p.is_file() and "__pycache__" not in p.parts)
    inventory.write_text("Paths relative to experiments/pdf-export; inventory includes itself.\n" + "\n".join(str(p.relative_to(ROOT)) for p in files if p != inventory) + "\nresults/FILES.txt\n")
    print(json.dumps(artifacts, indent=2))
    print(f"Inventory: {len(files)} files before inventory refresh; {sum(p.stat().st_size for p in files)} bytes")


if __name__ == "__main__":
    main()
