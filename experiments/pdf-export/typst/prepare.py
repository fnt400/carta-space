#!/usr/bin/env python3
"""Adapt only the existing corpus constructs; never evaluate authored code."""
import json
import os
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent


def literal(value):
    return json.dumps(value, ensure_ascii=False)


def inline(value):
    pattern = r"\*\*(.+?)\*\*|\*(.+?)\*|`(.+?)`|\[([^]]+)\]\(([^)]+)\)"
    parts, offset = [], 0
    for match in re.finditer(pattern, value):
        if match.start() > offset:
            parts.append("#text(" + literal(value[offset:match.start()]) + ")")
        bold, italic, code, label, url = match.groups()
        if bold or italic:
            parts.append("#" + ("strong" if bold else "emph") + "[" + inline(bold or italic) + "]")
        elif code:
            parts.append("#raw(" + literal(code) + ")")
        else:
            parts.append("#link(" + literal(url) + ")[" + inline(label) + "]")
        offset = match.end()
    if offset < len(value):
        parts.append("#text(" + literal(value[offset:]) + ")")
    return "".join(parts)


def convert(markdown):
    output = []
    for block in markdown.strip().split("\n\n"):
        if block.startswith("```"):
            output.append("#raw(" + literal("\n".join(block.splitlines()[1:-1])) + ", block: true)")
        elif block.startswith("#"):
            level = len(block) - len(block.lstrip("#"))
            output.append(f"#heading(level: {level})[" + inline(block[level:].strip()) + "]")
        elif block.startswith("> "):
            output.append("#quote[" + inline(block[2:]) + "]")
        elif block.startswith("- ") or re.match(r"1\. ", block):
            items = [re.sub(r"^(?:- |\d+\. )", "", line) for line in block.splitlines()]
            output.append("#" + ("list" if block.startswith("- ") else "enum") + "(" + ", ".join("[" + inline(item) + "]" for item in items) + ")")
        else:
            output.append(inline(" ".join(block.splitlines())))
    return "\n\n".join(output)


def main():
    if os.environ.get("CONTAINER_ID") != "carta-dev":
        raise SystemExit("Run via distrobox enter carta-dev")
    preamble = '#import "carta-classic.typ": carta-classic\n#show: carta-classic\n\n'
    for name in ("latin", "unicode"):
        source = (ROOT / "corpus" / f"{name}.md").read_text()
        (HERE / f"{name}.typ").write_text(preamble + convert(source) + "\n")
    # Same Latin paragraph as the previous scaling test; continuous flow.
    paragraph = (ROOT / "corpus" / "latin.md").read_text().split("\n\n")[10]
    for nominal in (100, 500):
        (HERE / f"long-{nominal}.typ").write_text(preamble + "\n\n".join([inline(paragraph)] * (5 * nominal)) + "\n")


if __name__ == "__main__":
    main()
