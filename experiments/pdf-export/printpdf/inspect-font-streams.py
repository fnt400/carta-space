"""Measure decoded embedded fonts using qpdf and fontTools, without pypdf."""

import io
import json
import re
import subprocess
import sys
from pathlib import Path

from fontTools.ttLib import TTFont


def font_info(data):
    font = TTFont(io.BytesIO(data))
    return {
        "bytes": len(data),
        "glyphs": font["maxp"].numGlyphs,
        "postscript_name": font["name"].getDebugName(6) if "name" in font else None,
        "weight": font["OS/2"].usWeightClass if "OS/2" in font else None,
        "tables": sorted(font.keys()),
    }


font_dir = Path(__file__).resolve().parent.parent / "fonts"
originals = {path.name: font_info(path.read_bytes()) for path in font_dir.glob("*.ttf")}
for pdf_name in sys.argv[1:]:
    objects = json.loads(
        subprocess.check_output(["qpdf", "--json", "--json-key=qpdf", pdf_name])
    )["qpdf"][1]
    for ref, obj in objects.items():
        descriptor = obj.get("value", {})
        if not isinstance(descriptor, dict) or "/FontFile2" not in descriptor:
            continue
        stream_ref = descriptor["/FontFile2"]
        obj_num, generation, _ = stream_ref.split()
        data = subprocess.check_output(
            ["qpdf", f"--show-object={obj_num},{generation}", "--filtered-stream-data", pdf_name]
        )
        embedded = font_info(data)
        pdf_font_name = descriptor["/FontName"].lstrip("/")
        face = pdf_font_name.split("+", 1)[-1]
        candidates = {
            name: info for name, info in originals.items()
            if info["postscript_name"] == face
        }
        base_names = []
        for item in objects.values():
            value = item.get("value", {})
            if not isinstance(value, dict):
                continue
            for descendant in value.get("/DescendantFonts", []):
                if isinstance(descendant, str):
                    descendant = objects["obj:" + descendant]["value"]
                if descendant.get("/FontDescriptor") == ref.removeprefix("obj:"):
                    base_names.extend([value.get("/BaseFont"), descendant.get("/BaseFont")])
        print(json.dumps({
            "pdf": pdf_name,
            "descriptor": ref,
            "font_name": pdf_font_name,
            "valid_subset_tag": bool(re.match(r"^[A-Z]{6}\+", pdf_font_name)),
            "base_font_names": base_names,
            "stream": stream_ref,
            "stream_dictionary": objects["obj:" + stream_ref]["stream"]["dict"],
            "embedded": embedded,
            "matching_originals": candidates,
        }, ensure_ascii=True))
