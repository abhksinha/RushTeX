import subprocess, re

old = subprocess.check_output(["git", "show", "4b175e6:src/math/math_fonts.rs"]).decode("utf-8")
new = open("src/math/math_symbols.rs").read()

old_entries = {}
for line in old.splitlines():
    m = re.search(r'(".*?")\s*=>\s*\(\s*MathAtomKind::(\w+),\s*Face::(\w+),\s*("(?:\\.|[^"\\])*"),\s*Some\((.*?)\)\)', line)
    if m:
        cmds_str = m.group(1)
        cmds = re.findall(r'"([^"]+)"', cmds_str)
        kind = m.group(2)
        face = m.group(3)
        glyph = m.group(4)
        code = m.group(5).strip()
        for c in cmds:
            old_entries[c] = (kind, face, glyph, code)

new_recs = re.findall(
    r'MathSymbolRecord\s*\{[^}]*?cmd:\s*"([^"]+)"[^}]*?glyph:\s*"((?:\\.|[^"\\])*)"[^}]*?kind:\s*MathAtomKind::(\w+)[^}]*?face:\s*Face::(\w+)[^}]*?tfm_code:\s*([^,]+),',
    new,
    re.DOTALL
)

print(f"Old parsed: {len(old_entries)}, New parsed: {len(new_recs)}")
diffs = []
for cmd, glyph, kind, face, tfm_code in new_recs:
    if cmd in old_entries:
        o_kind, o_face, o_glyph, o_code = old_entries[cmd]
        tfm_strip = tfm_code.strip()
        o_code_strip = o_code.strip()
        def parse_val(v):
            if v.startswith("0x") or v.startswith("0X"):
                return int(v, 16)
            elif v.startswith("b'") and v.endswith("'"):
                inner = v[2:-1]
                if inner == r"\0": return 0
                if inner == r"\\": return ord('\\')
                if inner == r"\'": return ord('\'')
                return ord(inner[0])
            elif v.isdigit():
                return int(v)
            return v
        nv = parse_val(tfm_strip)
        ov = parse_val(o_code_strip)
        if face != o_face or nv != ov or glyph != o_glyph.strip('"'):
            diffs.append((cmd, face, o_face, tfm_strip, o_code_strip, glyph, o_glyph))

print(f"Total diffs: {len(diffs)}")
for d in diffs:
    print(d)
