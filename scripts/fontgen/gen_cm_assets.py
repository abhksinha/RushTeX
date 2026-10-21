#!/usr/bin/env python3
"""Regenerates RushTeX's embedded Computer Modern assets from TeX Live.

For every font it produces:
  * assets/cm_fonts/<name>.flate : zlib-compressed bare CFF (Type 1C) program converted
    from the TeX Live Type 1 .pfb (outlines only, hints dropped, empty Private dict), the
    same format as the original assets (FontFile3 /Subtype /Type1C /Filter /FlateDecode).
  * a Rust GlyphMetric[128] table (w, h, d, ic in 1/1000 of the design size, from the .tfm;
    negative depths clamp to 0, as in math_tfm.rs);
  * the PDF /Differences encoding string (codes 0..127, from the .pfb built-in Encoding);
  * FontDescriptor numbers (FontBBox, ItalicAngle, StemV from the .pfb).

Usage: gen_cm_assets.py <repo_root>   (requires fontTools; needs kpsewhich on PATH)
"""
import io, json, struct, subprocess, sys, zlib

from fontTools import t1Lib
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.t2CharStringPen import T2CharStringPen
from fontTools.pens.recordingPen import RecordingPen

# (tfm/pfb name, flags)
FONTS = [
    ("cmr7", 32), ("cmmi7", 4), ("cmmi5", 4), ("cmsy7", 4), ("cmsy5", 4),
    ("cmtt10", 32), ("cmtt8", 32), ("cmss10", 32), ("cmss8", 32),
    ("eufm10", 4), ("eufm7", 4), ("eufm5", 4),
    ("cmmib10", 4), ("cmmib7", 4), ("cmmib5", 4),
    ("cmbsy10", 4), ("cmbsy7", 4), ("cmbsy5", 4),
    ("cmex7", 4),
]


def kpse(name):
    p = subprocess.run(["kpsewhich", name], capture_output=True, text=True).stdout.strip()
    if not p:
        raise SystemExit("kpsewhich: %s not found" % name)
    return p


def tfm_metrics(name):
    d = open(kpse(name + ".tfm"), "rb").read()
    lf, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np_ = struct.unpack(">12H", d[:24])
    ci = 24 + lh * 4
    wb = ci + (ec - bc + 1) * 4
    hb = wb + nw * 4
    db = hb + nh * 4
    ib = db + nd * 4
    fix = lambda off: struct.unpack(">i", d[off:off + 4])[0] / 2 ** 20
    out = {}
    for c in range(bc, ec + 1):
        b0, b1, b2, _ = d[ci + (c - bc) * 4: ci + (c - bc) * 4 + 4]
        if b0 == 0:
            continue
        out[c] = (fix(wb + b0 * 4), fix(hb + (b1 >> 4) * 4), fix(db + (b1 & 15) * 4), fix(ib + (b2 >> 2) * 4))
    return out


def q(v):
    return max(0, int(round(v * 1000)))


def build_cff(name):
    f = t1Lib.T1Font(kpse(name + ".pfb"))
    f.parse()
    font = f.font
    gs = f.getGlyphSet()
    enc = font["Encoding"]
    order = [".notdef"]
    for i in range(256):
        g = enc[i]
        if g != ".notdef" and g not in order and g in font["CharStrings"]:
            order.append(g)
    for g in font["CharStrings"].keys():
        if g not in order:
            order.append(g)
    charstrings = {}
    for g in order:
        glyph = gs[g]
        rec = RecordingPen()
        glyph.draw(rec)
        pen = T2CharStringPen(round(glyph.width), None)
        rec.replay(pen)
        charstrings[g] = pen.getCharString()
    ps = font["FontName"]
    fb = FontBuilder(1000, isTTF=False)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap({})
    fb.setupCFF(ps, {"FullName": ps, "FamilyName": ps, "FontBBox": list(font["FontBBox"])}, charstrings, {})
    buf = io.BytesIO()
    fb.font["CFF "].cff.compile(buf, fb.font)
    raw = buf.getvalue()
    diffs = []
    prev = None
    for i in range(128):
        g = enc[i]
        if g == ".notdef" or g not in font["CharStrings"]:
            prev = None
            continue
        if prev != i - 1:
            diffs.append(str(i))
        diffs.append("/" + g)
        prev = i
    info = font.get("FontInfo", {})
    stdvw = font["Private"].get("StdVW", [80])
    return {
        "ps": ps,
        "flate": zlib.compress(raw, 9),
        "bbox": [int(round(v)) for v in font["FontBBox"]],
        "italic_angle": int(round(float(info.get("ItalicAngle", 0)))),
        "stem_v": int(round(stdvw[0] if isinstance(stdvw, (list, tuple)) else stdvw)),
        "diff": " ".join(diffs),
    }


def main():
    root = sys.argv[1]
    meta = {}
    for name, flags in FONTS:
        r = build_cff(name)
        open("%s/assets/cm_fonts/%s.flate" % (root, name), "wb").write(r["flate"])
        m = tfm_metrics(name)
        table = [(q(m[c][0]), q(m[c][1]), q(m[c][2]), q(m[c][3])) if c in m else (0, 0, 0, 0) for c in range(128)]
        # Ascent/CapHeight/Descent from TFM: tallest ascender, H (or bbox), deepest descender.
        ascent = max(t[1] for t in table) or r["bbox"][3]
        cap = table[ord("H")][1] if table[ord("H")][1] else ascent
        descent = -max(t[2] for t in table)
        meta[name] = {k: v for k, v in r.items() if k != "flate"}
        meta[name].update({"flags": flags, "ascent": min(ascent, r["bbox"][3]), "cap": cap,
                           "descent": max(descent, r["bbox"][1]), "metrics": table,
                           "flate_len": len(r["flate"])})
        print(name, r["ps"], len(r["flate"]), file=sys.stderr)
    json.dump(meta, open(sys.argv[2] if len(sys.argv) > 2 else "cm_meta.json", "w"))


if __name__ == "__main__":
    main()
