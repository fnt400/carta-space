#!/usr/bin/env python3
"""Tiny corpus adapter, not a CommonMark implementation or Carta code."""
import html
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "benchmark" / "generated"


def inline(text, roff=False):
    if roff:
        text = text.replace("\\", r"\e").replace('"', r"\[dq]")
        text = re.sub(r"\[([^]]+)\]\(([^)]+)\)", r"\1", text)
        for pattern, font in [(r"\*\*(.+?)\*\*", "CB"), (r"\*(.+?)\*", "CI"), (r"`(.+?)`", "CM")]:
            text = re.sub(pattern, lambda m: rf"\f[{font}]" + m[1] + r"\f[CR]", text)
        return r"\&" + text
    text = html.escape(text)
    text = re.sub(r"\[([^]]+)\]\(([^)]+)\)", r'<a href="\2">\1</a>', text)
    for pattern, tag in [(r"\*\*(.+?)\*\*", "strong"), (r"\*(.+?)\*", "em"), (r"`(.+?)`", "code")]:
        text = re.sub(pattern, lambda m: f"<{tag}>{m[1]}</{tag}>", text)
    return text


def convert(markdown):
    h, r = [], []
    blocks = markdown.strip().split("\n\n")
    for block in blocks:
        if block.startswith("```"):
            code = "\n".join(block.splitlines()[1:-1])
            h.append("<pre><code>" + html.escape(code) + "</code></pre>")
            r.extend([".CODE", *[r"\&" + x.replace("\\", r"\e") for x in code.splitlines()], ".ENDCODE"])
        elif block.startswith("#"):
            level = len(block) - len(block.lstrip("#"))
            title = block[level:].strip()
            h.append(f"<h{level}>{inline(title)}</h{level}>")
            r.append(f'.H{level} "{inline(title, True)}"')
        elif block.startswith("> "):
            text = block[2:]
            h.append("<blockquote><p>" + inline(text) + "</p></blockquote>")
            r.extend([".BQ", inline(text, True), ".EQ"])
        elif block.startswith("- ") or re.match(r"1\. ", block):
            ordered = block.startswith("1.")
            tag = "ol" if ordered else "ul"
            items = [re.sub(r"^(?:- |\d+\. )", "", x) for x in block.splitlines()]
            h.append(f"<{tag}>" + "".join("<li>" + inline(x) + "</li>" for x in items) + f"</{tag}>")
            r.append(".OL" if ordered else ".UL")
            for item in items:
                r.extend([".LI", inline(item, True)])
            r.append(".LE")
        else:
            text = " ".join(block.splitlines())
            h.append("<p>" + inline(text) + "</p>")
            r.extend([".P", inline(text, True)])
    return "\n".join(h), "\n".join(r) + "\n"


def write(name, markdown):
    body, roff = convert(markdown)
    (OUT / f"{name}.html").write_text('<!DOCTYPE html><html><head><meta charset="UTF-8"/></head><body>\n' + body + "\n</body></html>\n")
    (OUT / f"{name}.roff").write_text(roff)
    return body, roff


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for name in ("latin", "unicode"):
        body, roff = write(name, (ROOT / "corpus" / f"{name}.md").read_text())
        (ROOT / "groff" / f"{name}.roff").write_text(roff)
        if name == "unicode":
            # A separate explicit-font diagnostic preserves the untouched baseline.
            fonts = [("Καλημέρα κόσμε", "NS", "Noto Sans"), ("Добрый день", "NS", "Noto Sans"),
                     ("שלום עולם", "NH", "Noto Sans Hebrew"), ("مرحبا بالعالم", "NA", "Noto Sans Arabic"),
                     ("नमस्ते दुनिया", "ND", "Noto Sans Devanagari"), ("こんにちは世界", "NJ", "Noto Sans JP")]
            rh, rr = body, roff
            for text, groff, family in fonts:
                rh = rh.replace(f"<p>{text}</p>", f'<p style="font-family: {family};">{text}</p>')
                rr = rr.replace(r"\&" + text + "\n", f".FONT {groff}\n" + r"\&" + text + "\n.FONT CR\n")
            mixed = "Italiano 日本語 Ελληνικά العربية हिन्दी"
            spans = [('Italiano', 'CR', 'Source Serif 4'), ('日本語', 'NJ', 'Noto Sans JP'), ('Ελληνικά', 'NS', 'Noto Sans'), ('العربية', 'NA', 'Noto Sans Arabic'), ('हिन्दी', 'ND', 'Noto Sans Devanagari')]
            rr = rr.replace(mixed, " ".join(rf"\f[{f}]{t}" for t, f, _ in spans) + r"\f[CR]")
            rh = rh.replace(mixed, " ".join(f'<span style="font-family: {f};">{t}</span>' for t, _, f in spans))
            (OUT / "unicode-explicit.html").write_text('<html><head></head><body>' + rh + '</body></html>')
            (OUT / "unicode-explicit.roff").write_text(rr)
            rtl = '<h1>RTL paragraphs</h1><p dir="rtl" style="direction: rtl; text-align: right; font-family: Noto Sans Hebrew;">שלום עולם</p><p dir="rtl" style="direction: rtl; text-align: right; font-family: Noto Sans Arabic;">مرحبا بالعالم</p><p dir="rtl" style="direction: rtl; text-align: right; font-family: Noto Sans Arabic;">العربية لغة جميلة 123</p>'
            (OUT / "rtl.html").write_text('<html><head></head><body>' + rtl + '</body></html>')
    # Equal content, continuous flow, no forced page breaks. Page counts are measured.
    paragraph = (ROOT / "corpus" / "latin.md").read_text().split("\n\n")[10]
    for pages in (1, 10, 100):
        write(f"perf-{pages}", "# Carta: prova di scala\n\n" + "\n\n".join([paragraph] * (5 * pages)))


if __name__ == "__main__":
    main()
