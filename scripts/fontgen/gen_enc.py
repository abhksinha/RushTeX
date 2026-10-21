"""gen_enc.py <repo> <tfm> <pfb> <enc> <first_slot> <flags>: an 8-bit TeX font (EC/TS1 from
cm-super) as a 128-code RushTeX face. Codes 0..127 are slots first_slot..first_slot+127 of the
TFM and of the dvips encoding file, or with first_slot -1 the encoding's used slots in order
(TS1 uses exactly 128; the slot-to-code table is printed too). The embedded CFF holds only
those glyphs.

Writes assets/cm_fonts/<tfm>.flate and prints the Rust statics: CmFontProgram, the
PackedGlyphMetric table, FixMetrics and the ToUnicode table. Requires fontTools and kpsewhich.
Example: gen_enc.py . tcrm1200 sfrm1200 cm-super-ts1.enc 128 32
"""
import io, os, re, struct, sys, zlib
sys.path.insert(0, os.path.dirname(__file__))
import gen_cm_assets as g
from fontTools import t1Lib
from fontTools.agl import toUnicode
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.recordingPen import RecordingPen
from fontTools.pens.t2CharStringPen import T2CharStringPen

repo, tfm, pfb, enc, first, flags = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], int(sys.argv[5]), int(sys.argv[6])

# The encoding vector: the names between [ and ], comments dropped.
text = re.sub(r"%[^\n]*", "", open(g.kpse(enc)).read())
names = re.findall(r"/([^\s\[\]/]+)", text[text.index("[") + 1:text.index("]")])
assert len(names) == 256, len(names)
if first < 0:
    slots = [i for i, n in enumerate(names) if n != ".notdef"]
    assert len(slots) <= 128, len(slots)
    slots += [None] * (128 - len(slots))
else:
    slots = list(range(first, first + 128))
codes = [names[i] if i is not None else ".notdef" for i in slots]

f = t1Lib.T1Font(g.kpse(pfb + ".pfb"))
f.parse()
font = f.font
gs = f.getGlyphSet()
order = [".notdef"] + [n for n in dict.fromkeys(codes) if n != ".notdef" and n in font["CharStrings"]]
charstrings = {}
for n in order:
    rec = RecordingPen()
    gs[n].draw(rec)
    pen = T2CharStringPen(round(gs[n].width), None)
    rec.replay(pen)
    charstrings[n] = pen.getCharString()
ps = font["FontName"]
fb = FontBuilder(1000, isTTF=False)
fb.setupGlyphOrder(order)
fb.setupCharacterMap({})
fb.setupCFF(ps, {"FullName": ps, "FamilyName": ps, "FontBBox": list(font["FontBBox"])}, charstrings, {})
buf = io.BytesIO()
fb.font["CFF "].cff.compile(buf, fb.font)
open(f"{repo}/assets/cm_fonts/{tfm}.flate", "wb").write(zlib.compress(buf.getvalue(), 9))

diffs, prev = [], None
for c, n in enumerate(codes):
    if n not in charstrings or n == ".notdef":
        prev = None
        continue
    if prev != c - 1:
        diffs.append(str(c))
    diffs.append("/" + n)
    prev = c

m = g.tfm_metrics(tfm)
d = open(g.kpse(tfm + ".tfm"), "rb").read()
lf, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np_ = struct.unpack(">12H", d[:24])
params = [struct.unpack(">i", d[4 * (lf - np_) + 4 * i: 4 * (lf - np_) + 4 * i + 4])[0] for i in range(min(np_, 7))]
ci = 24 + lh * 4; wb = ci + (ec - bc + 1) * 4; ib = wb + (nw + nh + nd) * 4
raw = lambda off: struct.unpack(">i", d[off:off + 4])[0]
fw, fi = [0] * 128, [0] * 128
for c in range(128):
    s = slots[c]
    if s is not None and bc <= s <= ec:
        b0, b1, b2, _ = d[ci + (s - bc) * 4: ci + (s - bc) * 4 + 4]
        if b0:
            fw[c] = raw(wb + b0 * 4); fi[c] = raw(ib + (b2 >> 2) * 4)

U = tfm.upper()
bbox = [int(round(v)) for v in font["FontBBox"]]
table = [m.get(slots[c], (0, 0, 0, 0)) if slots[c] is not None else (0, 0, 0, 0) for c in range(128)]
q = lambda v: max(0, round(v * 1000))
ascent = max(q(t[1]) for t in table); descent = -max(q(t[2]) for t in table)
info = font.get("FontInfo", {})
stdvw = font["Private"].get("StdVW", [80])
print(f"""pub static {U}_PROGRAM: CmFontProgram = CmFontProgram {{
    name: "{ps}",
    flags: {flags},
    bbox: [{', '.join(map(str, bbox))}],
    ascent: {min(ascent, bbox[3])},
    descent: {max(descent, bbox[1])},
    cap_height: {q(m.get(ord('H'), (0, 0))[1]) or ascent},
    italic_angle: {int(round(float(info.get('ItalicAngle', 0))))},
    stem_v: {int(round(stdvw[0] if isinstance(stdvw, (list, tuple)) else stdvw))},
    diff_encoding: "{' '.join(diffs)}",
    compressed_stream: include_bytes!("../../assets/cm_fonts/{tfm}.flate"),
}};
""")
print(f"/// {U} metric table.\npub static {U}_METRICS: [PackedGlyphMetric; 128] = [")
for c, (w, h, dd, ic) in enumerate(table):
    v = [max(0, round(x * 16384)) for x in (w, h, dd, ic)]
    slot = f"0x{slots[c]:02X}" if slots[c] is not None else "--"
    print(f"    PackedGlyphMetric {{ w: {v[0]}, h: {v[1]}, d: {v[2]}, ic: {v[3]} }}, // {slot} {codes[c]}")
print("];\n")
print(f"pub static {U}_FIX: FixMetrics = FixMetrics {{\n    widths: [{', '.join(map(str, fw))}],\n    ics: [{', '.join(map(str, fi))}],\n    params: [{', '.join(map(str, params))}],\n}};\n")
uni = [ord(toUnicode(n)[0]) if n != ".notdef" and toUnicode(n) else 0 for n in codes]
print(f"/// {U} ToUnicode.\npub static {U}_TOUNICODE: [u16; 128] = [{', '.join(f'0x{u:04X}' for u in uni)}];")
code_of = [0xFF] * 256
for c, sl in enumerate(slots):
    if sl is not None:
        code_of[sl] = c
print(f"/// Slot to code (0xFF: not in the font).\npub static {U}_SLOT_CODE: [u8; 256] = [{', '.join(map(str, code_of))}];")
