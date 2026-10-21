"""kern_index.py <rust file>..: rewrites `LigKernTable` kern statics to TFM's own layout: each
pair holds an index into the font's table of distinct kern fix_words (2^-20 em), as a TFM
file's lig/kern program indexes its kern array. Values come from the font's .tfm (exact);
tables of fonts without one (the Base-14 AFM kerns, integer 1/1000 em) convert their
1/10000 em values, which is exact to the fix_word.

Before: static X_KERNS: [(u16, i16); N] = [(0x4142, -278), ..];
        LigKernTable::new(&X_KERNS, &X_LIGS)
After:  static X_KERNS: [(u16, u8); N] = [(0x4142, 3), ..];
        static X_KERN_FIX: [i32; M] = [..];
        LigKernTable::new(&X_KERNS, &X_KERN_FIX, &X_LIGS)
"""
import os, re, subprocess, sys
sys.path.insert(0, os.path.dirname(__file__))
from ligkern_lib import ligkern

DECL = re.compile(r"(pub )?static (\w+): \[\(u16, i16\); (\d+)\] = \[(.*?)\];", re.S)
NEW = re.compile(r"LigKernTable::new\(&(\w+), &")


def tfm_name(static):
    for suffix in ("_KERN_PAIRS", "_KERNS"):
        if static.endswith(suffix):
            return static[: -len(suffix)].lower()
    return static.lower()


def has_tfm(name):
    return subprocess.run(["kpsewhich", name + ".tfm"], capture_output=True, text=True).stdout.strip() != ""


for path in sys.argv[1:]:
    src = open(path).read()

    def rewrite(m):
        pub, name, _, body = m.groups()
        pairs = [(int(k, 16), int(v)) for k, v in re.findall(r"\(0x([0-9A-Fa-f]+), (-?\d+)\)", body)]
        font = tfm_name(name)
        exact = ligkern(font, fix_words=True)[0] if has_tfm(font) else None
        fixes = []
        for key, tenths in pairs:
            if exact is not None:
                f = exact[key]
                assert round(f * 10000 / 2**20) == tenths, (font, hex(key), f, tenths)
            else:
                f = round(tenths * 2**20 / 10000)
            fixes.append(f)
        table = sorted(set(fixes))
        assert len(table) <= 256, (font, len(table))
        idx = {f: i for i, f in enumerate(table)}
        rows = "".join(f"    (0x{k:04X}, {idx[f]}),\n" for (k, _), f in zip(pairs, fixes))
        fix_name = name + "_FIX_WORDS"
        return (f"{pub or ''}static {name}: [(u16, u8); {len(pairs)}] = [\n{rows}];\n"
                f"{pub or ''}static {fix_name}: [i32; {len(table)}] = [{', '.join(map(str, table))}];")

    src = DECL.sub(rewrite, src)
    src = NEW.sub(lambda m: f"LigKernTable::new(&{m.group(1)}, &{m.group(1)}_FIX_WORDS, &", src)
    open(path, "w").write(src)
