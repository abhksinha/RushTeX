"""gen_one.py <repo> <font> <flags>: writes assets/cm_fonts/<font>.flate and prints the Rust
statics (CmFontProgram, PackedGlyphMetric table, FixMetrics, LigKernTable) to stdout."""
import sys, struct, os
sys.path.insert(0, os.path.dirname(__file__))
import gen_cm_assets as g
from ligkern_lib import ligkern
repo, name, flags = sys.argv[1], sys.argv[2], int(sys.argv[3])
r = g.build_cff(name)
open(f"{repo}/assets/cm_fonts/{name}.flate", "wb").write(r["flate"])
m = g.tfm_metrics(name)
d = open(g.kpse(name + ".tfm"), "rb").read()
lf, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np_ = struct.unpack(">12H", d[:24])
pb = 4 * (lf - np_)
params = [struct.unpack(">i", d[pb + 4 * i: pb + 4 * i + 4])[0] for i in range(min(np_, 7))]
ci = 24 + lh * 4; wb = ci + (ec - bc + 1) * 4; ib = wb + (nw + nh + nd) * 4
raw = lambda off: struct.unpack(">i", d[off:off + 4])[0]
fw, fi = [0] * 128, [0] * 128
for c in range(bc, min(ec, 127) + 1):
    b0, b1, b2, _ = d[ci + (c - bc) * 4: ci + (c - bc) * 4 + 4]
    if b0: fw[c] = raw(wb + b0 * 4); fi[c] = raw(ib + (b2 >> 2) * 4)
U = name.upper()
q = lambda v: max(0, round(v * 1000))
table = [(q(m[c][0]), q(m[c][1]), q(m[c][2]), q(m[c][3])) if c in m else (0, 0, 0, 0) for c in range(128)]
ascent = max(t[1] for t in table); cap = table[ord("H")][1] or ascent; descent = -max(t[2] for t in table)
print(f"""pub static {U}_PROGRAM: CmFontProgram = CmFontProgram {{
    name: "{r['ps']}",
    flags: {flags},
    bbox: [{', '.join(map(str, r['bbox']))}],
    ascent: {min(ascent, r['bbox'][3])},
    descent: {max(descent, r['bbox'][1])},
    cap_height: {cap},
    italic_angle: {r['italic_angle']},
    stem_v: {r['stem_v']},
    diff_encoding: "{r['diff']}",
    compressed_stream: include_bytes!("../../assets/cm_fonts/{name}.flate"),
}};
""")
print(f"/// {U} metric table.\npub static {U}_METRICS: [PackedGlyphMetric; 128] = [")
for c in range(128):
    w, h, dd, ic = m.get(c, (0, 0, 0, 0)); v = [max(0, round(x * 16384)) for x in (w, h, dd, ic)]
    print(f"    PackedGlyphMetric {{ w: {v[0]}, h: {v[1]}, d: {v[2]}, ic: {v[3]} }}, // 0x{c:02X}")
print("];\n")
print(f"pub static {U}_FIX: FixMetrics = FixMetrics {{\n    widths: [{', '.join(map(str, fw))}],\n    ics: [{', '.join(map(str, fi))}],\n    params: [{', '.join(map(str, params))}],\n}};\n")
kerns, ligs = ligkern(name)
print(f"static {U}_KERNS: [(u16, i16); {len(kerns)}] = [")
for k, v in sorted(kerns.items()): print(f"    (0x{k:04X}, {v}),")
print(f"];\nstatic {U}_LIGS: [(u16, u8, u8); {len(ligs)}] = [")
for k, (op, rem) in sorted(ligs.items()): print(f"    (0x{k:04X}, {op}, 0x{rem:02X}),")
print(f"];\npub static {U}: LigKernTable = LigKernTable::new(&{U}_KERNS, &{U}_LIGS);")
