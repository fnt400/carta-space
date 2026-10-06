"""Run only inside carta-dev; stage upstream inputs under /opt/carta-pdf-bench."""

import hashlib
import json
import os
from pathlib import Path
import shutil
import urllib.request

import fontTools
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont


DEST = Path(__file__).resolve().parent
STAGE = Path("/opt/carta-pdf-bench/pdf-export-fonts")
SOURCES = {
    "source-serif": (
        "adobe-fonts/source-serif",
        "2823e993c53fca27c5c8749f529b56a5a7c77b6b",
        "4.005R",
        "LICENSE.md",
    ),
    "source-code-pro": (
        "adobe-fonts/source-code-pro",
        "d3f1a5962cde503f9409c21e58527611d4a19ef1",
        "2.042R-u/1.062R-i/1.026R-vf",
        "LICENSE.md",
    ),
    "noto-fonts": (
        "notofonts/noto-fonts",
        "ffebf8c1ee449e544955a7e813c54f9b73848eac",
        None,
        "LICENSE",
    ),
    "noto-cjk": (
        "notofonts/noto-cjk",
        "523d033d6cb47f4a80c58a35753646f5c3608a78",
        "Sans2.004",
        "LICENSE",
    ),
}
FONTS = [
    ("SourceSerif4-Regular.ttf", "source-serif", "TTF/SourceSerif4-Regular.ttf", None, "Latin body", "Aa"),
    ("SourceSerif4-Italic.ttf", "source-serif", "TTF/SourceSerif4-It.ttf", None, "Latin italic", "Aa"),
    ("SourceSerif4-Semibold.ttf", "source-serif", "TTF/SourceSerif4-Semibold.ttf", None, "Latin emphasis", "Aa"),
    ("SourceCodePro-Regular.ttf", "source-code-pro", "TTF/SourceCodePro-Regular.ttf", None, "Monospaced code", "Aa0"),
    ("NotoSans-Regular.ttf", "noto-fonts", "hinted/ttf/NotoSans/NotoSans-Regular.ttf", None, "Greek and Cyrillic fallback", "\u03b1\u03a9\u0416\u044f"),
    ("NotoSansHebrew-Regular.ttf", "noto-fonts", "hinted/ttf/NotoSansHebrew/NotoSansHebrew-Regular.ttf", None, "Hebrew fallback", "\u05d0\u05e9"),
    ("NotoSansArabic-Regular.ttf", "noto-fonts", "hinted/ttf/NotoSansArabic/NotoSansArabic-Regular.ttf", None, "Arabic fallback", "\u0627\u0644\u0645"),
    ("NotoSansDevanagari-Regular.ttf", "noto-fonts", "hinted/ttf/NotoSansDevanagari/NotoSansDevanagari-Regular.ttf", None, "Devanagari fallback", "\u0915\u093f\u0928"),
    ("NotoSansJP-Regular.ttf", "noto-cjk", "Sans/Variable/TTF/Subset/NotoSansJP-VF.ttf", {"wght": 400}, "Japanese fallback (upstream JP subset)", "\u3042\u30a2\u65e5\u672c\u8a9e"),
]


def digest(path):
    return {"bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def download(source, upstream_path):
    repo, commit, _, _ = SOURCES[source]
    url = f"https://raw.githubusercontent.com/{repo}/{commit}/{upstream_path}"
    path = STAGE / source / upstream_path
    path.parent.mkdir(parents=True, exist_ok=True)
    urllib.request.urlretrieve(url, path)
    return path, url


def main():
    if not Path("/run/.containerenv").exists() and not os.environ.get("CONTAINER_ID"):
        raise SystemExit("Run with distrobox enter carta-dev -- python3 .../fonts/prepare.py")
    if not STAGE.parent.is_dir():
        raise SystemExit("Missing approved staging parent /opt/carta-pdf-bench")
    STAGE.mkdir(exist_ok=True)
    licenses = {}
    for source, (repo, commit, release, license_path) in SOURCES.items():
        path, url = download(source, license_path)
        assert b"SIL OPEN FONT LICENSE" in path.read_bytes().upper()
        target = DEST / f"OFL-{source}.txt"
        shutil.copyfile(path, target)
        licenses[source] = {
            "repository": f"https://github.com/{repo}",
            "commit": commit,
            "release": release,
            "file": target.name,
            "url": url,
            **digest(target),
        }
    records = []
    for filename, source, upstream_path, axes, role, sample in FONTS:
        path, url = download(source, upstream_path)
        font = TTFont(path, recalcTimestamp=False)
        input_digest = digest(path)
        conversion = None
        if axes is not None:
            original_axes = {a.axisTag: {"min": a.minValue, "default": a.defaultValue, "max": a.maxValue} for a in font["fvar"].axes}
            assert set(axes) == set(original_axes)
            # Noto's variable default is Thin (100); Regular is explicitly 400.
            font = instantiateVariableFont(font, axes, inplace=True, optimize=True, updateFontNames=True)
            static = STAGE / filename
            font.save(static)
            conversion = {
                "tool": "fontTools.varLib.instancer.instantiateVariableFont",
                "fonttools_version": fontTools.__version__,
                "axes": axes,
                "source_axes": original_axes,
                "options": {"inplace": True, "optimize": True, "updateFontNames": True, "recalcTimestamp": False},
                "outline_conversion": False,
                "subsetting": False,
            }
            path = static
        assert "fvar" not in font and "glyf" in font and "CFF " not in font
        assert font["OS/2"].fsType == 0
        cmap = font.getBestCmap()
        assert all(ord(c) in cmap for c in sample), filename
        target = DEST / filename
        shutil.copyfile(path, target)
        names = {str(i): sorted({n.toUnicode() for n in font["name"].names if n.nameID == i}) for i in [0, 1, 2, 3, 4, 5, 6, 13, 14, 16, 17]}
        records.append({
            "file": filename,
            "role": role,
            "source": source,
            "url": url,
            "upstream_path": upstream_path,
            "upstream_file": input_digest,
            "conversion": conversion,
            **digest(target),
            "name_table": names,
            "weight_class": font["OS/2"].usWeightClass,
            "embedding_fsType": font["OS/2"].fsType,
            "glyphs": font["maxp"].numGlyphs,
            "unicode_cmap_entries": len(cmap),
            "coverage_smoke_test": [f"U+{ord(c):04X}" for c in sample],
            "tables": sorted(font.keys()),
        })
        font.close()
        print(filename, target.stat().st_size, records[-1]["sha256"], flush=True)
    manifest = {"schema_version": 1, "license": "SIL Open Font License 1.1", "sources": licenses, "fonts": records}
    (DEST / "manifest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
