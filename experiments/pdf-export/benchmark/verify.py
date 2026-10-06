#!/usr/bin/env python3
"""Record exact extraction failures; reference PNG is not a replacement PDF."""
import json
import re
import unicodedata
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, features

ROOT = Path(__file__).resolve().parents[1]
RESULTS = ROOT / "results"


def clean(text):
    return re.sub(r"\s+", " ", "".join(c for c in text if unicodedata.category(c) != "Cf")).strip()


def main():
    samples = ["Perché l'estate è già finita — «Davvero?» Costa 10 €.", "Příliš žluťoučký kůň", "Zażółć gęślą jaźń", "Știință și țară", "Καλημέρα κόσμε", "Добрый день", "é", "e\u0301", "שלום עולם", "مرحبا بالعالم", "नमस्ते दुनिया", "こんにちは世界", "Italiano 日本語 Ελληνικά العربية हिन्दी"]
    records = []
    for engine in ("groff", "printpdf"):
        for variant in ("unicode", "unicode-explicit", "unicode-fallback"):
            path = RESULTS / f"{engine}-{variant}.layout.txt"
            if not path.exists():
                continue
            extracted = clean(path.read_text())
            records.append({"file": path.name, "checks": [{"input": s, "exact_substring_present": clean(s) in extracted} for s in samples], "cleaned_extraction": extracted})
        for pages in (1, 10, 100):
            path = RESULTS / f"{engine}-perf-{pages}.layout.txt"
            records.append({"file": path.name, "expected_paragraphs": 5 * pages, "extracted_paragraph_starts": clean(path.read_text()).count("Un paragrafo lungo")})
    (RESULTS / "extraction-checks.json").write_text(json.dumps(records, ensure_ascii=False, indent=2) + "\n")
    assert features.check_feature("raqm"), "Pillow RAQM required for reference"
    image = Image.new("RGB", (1100, 650), "white")
    draw = ImageDraw.Draw(image)
    label = ImageFont.truetype(str(ROOT / "fonts" / "SourceCodePro-Regular.ttf"), 22)
    rows = [("Hebrew", "NotoSansHebrew-Regular.ttf", "שלום עולם", "rtl", "he"),
            ("Arabic", "NotoSansArabic-Regular.ttf", "مرحبا بالعالم", "rtl", "ar"),
            ("Arabic + numbers", "NotoSansArabic-Regular.ttf", "العربية لغة جميلة 123", "rtl", "ar"),
            ("Devanagari", "NotoSansDevanagari-Regular.ttf", "नमस्ते दुनिया", "ltr", "hi"),
            ("Hindi", "NotoSansDevanagari-Regular.ttf", "हिन्दी", "ltr", "hi"),
            ("Composed / decomposed", "SourceSerif4-Regular.ttf", "é e\u0301 é e\u0301", "ltr", "it")]
    for i, (name, file, text, direction, language) in enumerate(rows):
        font = ImageFont.truetype(str(ROOT / "fonts" / file), 40, layout_engine=ImageFont.Layout.RAQM)
        y = 20 + 100 * i
        draw.text((20, y), name, font=label, fill="black")
        draw.text((480, y + 25), text, font=font, fill="black", direction=direction, language=language)
    image.save(RESULTS / "preview" / "shaping-reference-raqm.png")
    for name, box in [("groff-unicode-explicit-1", (140, 790, 650, 1015)), ("printpdf-unicode-1", (140, 730, 650, 950))]:
        original = Image.open(RESULTS / "preview" / f"{name}.png")
        crop = original.crop(box)
        crop.resize((crop.width * 3, crop.height * 3)).save(RESULTS / "preview" / f"{name}-shaping-detail.png")
    (RESULTS / "shaping-reference.txt").write_text("Independent raster reference only; not a third PDF renderer.\n" + "Pillow RAQM " + str(features.version_feature("raqm")) + "; HarfBuzz " + str(features.version_feature("harfbuzz")) + "; FriBidi " + str(features.version_feature("fribidi")) + "\nSame bundled fonts; RTL/language explicit. Visual comparison, not full-script conformance.\n")


if __name__ == "__main__":
    main()
