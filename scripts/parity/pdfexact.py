"""Zero-tolerance comparison of two PDFs: what a viewer draws, computed exactly.

Each page's content stream is interpreted with exact rational arithmetic (`Fraction` of the
decimal numbers as written), so a glyph's position is the one any viewer derives, with no
float rounding in between. Three checks, all without tolerance:

  marks      every glyph (base font, code, full rendering matrix) and every painted path
             (device coordinates, paint operator), and the fill/stroke colour of each
  pixels     both PDFs rendered by MuPDF at the same resolution; differing pixels per page
  structure  link annotations, outlines, named destinations and document metadata

A page passes `marks` only when the two multisets are equal; one scaled point (1/65536 pt,
about 1.5e-5 bp) of difference is a mismatch.
"""
from collections import Counter
from fractions import Fraction
import re

import pymupdf

ONE = Fraction(1)
IDENTITY = (ONE, Fraction(0), Fraction(0), ONE, Fraction(0), Fraction(0))


def mul(m, n):
    """m x n for PDF's row-vector matrices [a b c d e f]."""
    a, b, c, d, e, f = m
    A, B, C, D, E, F = n
    return (a * A + b * C, a * B + b * D, c * A + d * C, c * B + d * D, e * A + f * C + E, e * B + f * D + F)


def apply(m, x, y):
    a, b, c, d, e, f = m
    return (x * a + y * c + e, x * b + y * d + f)


# --- content stream tokens -------------------------------------------------------------

DELIM = b"()<>[]{}/%"
WS = b" \t\r\n\f\x00"


def tokens(s):
    """Operands as Python values (Fraction, bytes for strings, ('name', str), lists, dicts as
    lists) and operators as str."""
    i, n = 0, len(s)
    stack = [[]]
    while i < n:
        c = s[i]
        if c in WS:
            i += 1
        elif c == 0x25:  # % comment
            while i < n and s[i] not in b"\r\n":
                i += 1
        elif c == 0x28:  # ( literal string
            depth, i, out = 1, i + 1, bytearray()
            while i < n:
                ch = s[i]
                if ch == 0x5C:
                    i += 1
                    e = s[i]
                    if 0x30 <= e <= 0x37:
                        j = i
                        while j < n and j < i + 3 and 0x30 <= s[j] <= 0x37:
                            j += 1
                        out.append(int(s[i:j], 8) & 0xFF)
                        i = j
                        continue
                    i += 1
                    if e in b"\r\n":  # a line continuation
                        if e == 0x0D and i < n and s[i] == 0x0A:
                            i += 1
                        continue
                    out.append({0x6E: 10, 0x72: 13, 0x74: 9, 0x62: 8, 0x66: 12}.get(e, e))
                    continue
                if ch == 0x28:
                    depth += 1
                elif ch == 0x29:
                    depth -= 1
                    if depth == 0:
                        i += 1
                        break
                out.append(ch)
                i += 1
            stack[-1].append(bytes(out))
        elif c == 0x3C and i + 1 < n and s[i + 1] == 0x3C:  # << dict
            stack.append([])
            i += 2
        elif c == 0x3E and i + 1 < n and s[i + 1] == 0x3E:
            d = stack.pop()
            stack[-1].append(("dict", d))
            i += 2
        elif c == 0x3C:  # <hex>
            j = s.index(b">", i)
            h = re.sub(rb"\s", b"", s[i + 1:j])
            if len(h) % 2:
                h += b"0"
            stack[-1].append(bytes.fromhex(h.decode()))
            i = j + 1
        elif c == 0x5B:
            stack.append([])
            i += 1
        elif c == 0x5D:
            a = stack.pop()
            stack[-1].append(a)
            i += 1
        elif c == 0x2F:  # /name
            j = i + 1
            while j < n and s[j] not in WS and s[j] not in DELIM:
                j += 1
            stack[-1].append(("name", s[i + 1:j].decode("latin1")))
            i = j
        else:
            j = i
            while j < n and s[j] not in WS and s[j] not in DELIM:
                j += 1
            w = s[i:j].decode("latin1")
            i = j
            if re.fullmatch(r"[+-]?(\d+\.?\d*|\.\d+)", w):
                stack[-1].append(Fraction(w.rstrip(".") if w.endswith(".") else w))
            elif w in ("true", "false", "null"):
                stack[-1].append(w)
            elif w == "BI":  # inline image: skip to EI
                k = s.find(b"EI", j)
                stack[-1] = []
                yield ("op", "BI", [])
                i = n if k < 0 else k + 2
            else:
                ops = stack[-1]
                stack[-1] = []
                yield ("op", w, ops)


# --- resources ------------------------------------------------------------------------


def ref_xref(text):
    m = re.match(r"\s*(\d+)\s+0\s+R", text)
    return int(m.group(1)) if m else None


def dict_entries(doc, text):
    """`/Name n 0 R` pairs of a dictionary given as text (resolved if indirect)."""
    x = ref_xref(text)
    if x is not None:
        text = doc.xref_object(x)
    return {k: int(v) for k, v in re.findall(r"/([^\s/<>\[\]()]+)\s+(\d+)\s+0\s+R", text)}


def resources(doc, xref):
    """Font and XObject names of the object `xref` (a page or a form)."""
    out = {}
    for kind in ("Font", "XObject"):
        t, v = doc.xref_get_key(xref, f"Resources/{kind}")
        if t == "null":
            # Resources may be indirect as a whole.
            t2, v2 = doc.xref_get_key(xref, "Resources")
            r = ref_xref(v2) if t2 == "xref" else None
            if r is not None:
                t, v = doc.xref_get_key(r, kind)
        out[kind] = dict_entries(doc, v) if t in ("dict", "xref") else {}
    return out


class FontInfo:
    def __init__(self, doc, xref):
        t, v = doc.xref_get_key(xref, "BaseFont")
        # An indirect name (pdfTeX writes one for a font it replaced in an included PDF) is
        # the same name.
        if t == "xref":
            v = doc.xref_object(ref_xref(v)).strip()
        self.base = v.lstrip("/")
        # A subset tag (`ABCDEF+`) is not part of the font's identity.
        self.base = re.sub(r"^[A-Z]{6}\+", "", self.base)
        first = doc.xref_get_key(xref, "FirstChar")[1]
        self.first = int(first) if first.lstrip("-").isdigit() else 0
        t, v = doc.xref_get_key(xref, "Widths")
        if t == "xref":
            v = doc.xref_object(ref_xref(v))
        nums = re.findall(r"[+-]?(?:\d+\.?\d*|\.\d+)", v) if t in ("array", "xref") else []
        self.widths = [Fraction(w) for w in nums]
        t, v = doc.xref_get_key(xref, "FontMatrix")
        self.matrix = None
        if t == "array":
            m = [Fraction(w) for w in re.findall(r"[+-]?(?:\d+\.?\d*|\.\d+)", v)]
            self.matrix = tuple(m) if len(m) == 6 else None

    def width(self, code):
        k = code - self.first
        w = self.widths[k] if 0 <= k < len(self.widths) else Fraction(0)
        # Type 1 glyph space is 1/1000; a Type 3 font's own matrix scales it.
        return w * self.matrix[0] if self.matrix else w / 1000


# --- interpretation -------------------------------------------------------------------


class State:
    def __init__(self, ctm):
        self.ctm = ctm
        self.fill = ("g", (Fraction(0),))
        self.stroke = ("G", (Fraction(0),))
        self.font, self.size = None, Fraction(0)
        self.tc = self.tw = self.rise = Fraction(0)
        self.th = ONE
        self.tl = Fraction(0)

    def copy(self):
        s = State(self.ctm)
        s.__dict__.update(self.__dict__)
        return s


def colour(ops, op):
    nums = tuple(o for o in ops if isinstance(o, Fraction))
    names = tuple(o[1] for o in ops if isinstance(o, tuple) and o[0] == "name")
    return (op, nums + names)


PAINT = {"f", "F", "f*", "B", "B*", "b", "b*", "S", "s"}


def marks(doc, xref, stream, ctm, out, fonts, depth=0):
    """Appends every glyph and painted path of `stream` (in the resources of `xref`) to `out`."""
    res = resources(doc, xref)
    st = State(ctm)
    stack = []
    tm = lm = IDENTITY
    path = []
    for _, op, a in tokens(stream):
        if op == "q":
            stack.append(st.copy())
        elif op == "Q":
            if stack:
                st = stack.pop()
        elif op == "cm":
            st.ctm = mul(tuple(a[:6]), st.ctm)
        elif op in ("g", "rg", "k", "sc", "scn", "cs"):
            st.fill = colour(a, op)
        elif op in ("G", "RG", "K", "SC", "SCN", "CS"):
            st.stroke = colour(a, op)
        elif op == "BT":
            tm = lm = IDENTITY
        elif op == "Tf":
            name = a[0][1]
            x = res["Font"].get(name)
            if x not in fonts:
                fonts[x] = FontInfo(doc, x) if x else None
            st.font, st.size = fonts[x], a[1]
        elif op == "Tc":
            st.tc = a[0]
        elif op == "Tw":
            st.tw = a[0]
        elif op == "Tz":
            st.th = a[0] / 100
        elif op == "TL":
            st.tl = a[0]
        elif op == "Ts":
            st.rise = a[0]
        elif op in ("Td", "TD"):
            if op == "TD":
                st.tl = -a[1]
            lm = mul((ONE, 0, 0, ONE, a[0], a[1]), lm)
            tm = lm
        elif op == "Tm":
            lm = tm = tuple(a[:6])
        elif op == "T*":
            lm = mul((ONE, 0, 0, ONE, 0, -st.tl), lm)
            tm = lm
        elif op in ("Tj", "TJ", "'", '"'):
            if op in ("'", '"'):
                if op == '"':
                    st.tw, st.tc = a[0], a[1]
                lm = mul((ONE, 0, 0, ONE, 0, -st.tl), lm)
                tm = lm
            items = a[-1] if op == "TJ" else [a[-1]]
            for it in items:
                if isinstance(it, Fraction):
                    tm = mul((ONE, 0, 0, ONE, -it / 1000 * st.size * st.th, 0), tm)
                    continue
                if not isinstance(it, bytes):
                    continue
                for code in it:
                    trm = mul(mul((st.size * st.th, 0, 0, st.size, 0, st.rise), tm), st.ctm)
                    base = st.font.base if st.font else "?"
                    out.append(("glyph", base, code, trm, st.fill))
                    w = st.font.width(code) if st.font else Fraction(0)
                    tx = (w * st.size + st.tc + (st.tw if code == 32 else 0)) * st.th
                    tm = mul((ONE, 0, 0, ONE, tx, 0), tm)
        elif op == "re":
            x, y, w, h = a[:4]
            path.append(("re", apply(st.ctm, x, y), apply(st.ctm, x + w, y + h)))
        elif op in ("m", "l"):
            path.append((op, apply(st.ctm, a[0], a[1])))
        elif op in ("c", "v", "y"):
            pts = [apply(st.ctm, a[k], a[k + 1]) for k in range(0, len(a) - 1, 2)]
            path.append((op, *pts))
        elif op == "h":
            path.append(("h",))
        elif op in PAINT:
            col = st.stroke if op in ("S", "s") else st.fill
            out.append(("path", op, tuple(path), col))
            path = []
        elif op == "n":
            path = []
        elif op == "Do":
            x = res["XObject"].get(a[0][1])
            if x is None:
                continue
            sub = doc.xref_get_key(x, "Subtype")[1]
            if sub == "/Form" and depth < 8:
                m = doc.xref_get_key(x, "Matrix")
                fm = IDENTITY
                if m[0] == "array":
                    v = [Fraction(w) for w in re.findall(r"[+-]?(?:\d+\.?\d*|\.\d+)", m[1])]
                    fm = tuple(v) if len(v) == 6 else IDENTITY
                marks(doc, x, doc.xref_stream(x), mul(fm, st.ctm), out, fonts, depth + 1)
            else:
                # An image: its placement (the unit square through the CTM) and its size.
                out.append(("image", st.ctm, doc.xref_get_key(x, "Width")[1], doc.xref_get_key(x, "Height")[1]))


def page_marks(doc, page, fonts):
    out = []
    stream = b"".join(doc.xref_stream(x) or b"" for x in page.get_contents())
    marks(doc, page.xref, stream, IDENTITY, out, fonts)
    return out


def without_colour(m):
    return m[:-1] if m[0] in ("glyph", "path") else m


def compare_marks(A, B):
    """Per page: (ref glyphs, glyphs matched exactly, ref paths, paths matched, colour
    mismatches among position-matched marks, stream identical)."""
    rows = []
    fa, fb = {}, {}
    for i in range(len(A)):
        ma = page_marks(A, A[i], fa)
        mb = page_marks(B, B[i], fb) if i < len(B) else []
        ca, cb = Counter(ma), Counter(mb)
        pa, pb = Counter(map(without_colour, ma)), Counter(map(without_colour, mb))
        both = ca & cb
        pos = pa & pb
        g = sum(v for k, v in ca.items() if k[0] == "glyph")
        gm = sum(v for k, v in pos.items() if k[0] == "glyph")
        p = sum(v for k, v in ca.items() if k[0] != "glyph")
        pm = sum(v for k, v in pos.items() if k[0] != "glyph")
        colour_bad = sum(pos.values()) - sum(both.values())
        extra = sum((cb - ca).values())
        rows.append((g, gm, p, pm, colour_bad, extra))
    return rows


def first_difference(A, B, page):
    """The first ref mark of `page` (1-based) with no exact twin, and our nearest by position."""
    fa, fb = {}, {}
    ma, mb = page_marks(A, A[page - 1], fa), page_marks(B, B[page - 1], fb)
    left = Counter(mb)
    for m in ma:
        if left[m] > 0:
            left[m] -= 1
            continue
        near = None
        if m[0] == "glyph":
            cands = [k for k in left if left[k] > 0 and k[0] == "glyph" and k[2] == m[2]]
            if cands:
                near = min(cands, key=lambda k: abs(k[3][4] - m[3][4]) + abs(k[3][5] - m[3][5]))
        return m, near
    return None, None


# --- pixels ---------------------------------------------------------------------------


def pixel_diffs(A, B, dpi):
    """Differing pixels per page (any channel), both rendered by MuPDF at `dpi`."""
    import numpy as np

    rows = []
    for i in range(len(A)):
        pa = A[i].get_pixmap(dpi=dpi, alpha=False)
        if i >= len(B):
            rows.append(pa.width * pa.height)
            continue
        pb = B[i].get_pixmap(dpi=dpi, alpha=False)
        if (pa.width, pa.height) != (pb.width, pb.height):
            rows.append(pa.width * pa.height)
            continue
        a = np.frombuffer(pa.samples, dtype=np.uint8).reshape(pa.height, pa.width, pa.n)
        b = np.frombuffer(pb.samples, dtype=np.uint8).reshape(pb.height, pb.width, pb.n)
        rows.append(int(np.any(a != b, axis=2).sum()))
    return rows


# --- structure ------------------------------------------------------------------------

META = ("title", "author", "subject", "keywords", "creator")


def structure(doc):
    links = []
    for i, p in enumerate(doc):
        for l in p.get_links():
            r = l["from"]
            links.append((i, round(r.x0, 3), round(r.y0, 3), round(r.x1, 3), round(r.y1, 3), l.get("kind"), l.get("uri") or l.get("nameddest") or l.get("page")))
    toc = [tuple(t[:3]) for t in doc.get_toc()]
    meta = tuple((k, doc.metadata.get(k) or "") for k in META)
    # Each named destination: its page and the exact point it shows (in PDF units, as
    # written; pdfTeX prints them to \pdfdecimaldigits, so 4 places hold them exactly).
    try:
        dests = {}
        for name, d in doc.resolve_names().items():
            to = d.get("to") or (None, None)
            dests[name] = (d.get("page"), *(None if v is None else round(v, 4) for v in to))
    except Exception:
        dests = None
    return {"links": links, "outlines": toc, "meta": meta, "dests": dests}


def compare_structure(A, B):
    """Each structural part: (ref count, matched, equal)."""
    sa, sb = structure(A), structure(B)
    out = {}
    for k in ("links", "outlines"):
        ca, cb = Counter(sa[k]), Counter(sb[k])
        out[k] = (len(sa[k]), sum((ca & cb).values()), ca == cb)
    out["meta"] = (len(META), sum(x == y for x, y in zip(sa["meta"], sb["meta"])), sa["meta"] == sb["meta"])
    da, db = sa["dests"] or {}, sb["dests"] or {}
    same = sum(1 for k, v in da.items() if db.get(k) == v)
    out["dests"] = (len(da), same, sa["dests"] is not None and da == db)
    return out, sa, sb
