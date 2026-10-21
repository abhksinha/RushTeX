"""gen_ts1_symbols.py: the text commands that LaTeX typesets from TS1 by default, with their
slots, as a Rust table sorted by name (for binary search).

A command is TS1 when ts1enc.def declares it and either no other default encoding (OT1, OMS,
OML) declares it, or latex.ltx's `\\DeclareTextSymbolDefault{..}{TS1}` list moves it to TS1.
Requires kpsewhich.
"""
import re, subprocess

def kpse(name):
    return subprocess.run(["kpsewhich", name], capture_output=True, text=True).stdout.strip()

ts1 = open(kpse("ts1enc.def")).read()
slots = {m.group(1): int(m.group(2)) for m in re.finditer(r"\\DeclareTextSymbol\{\\(\w+)\}\{TS1\}\{(\d+)\}", ts1)}
# \textasteriskcentered: `\iffontchar\font 42 \char42`, which tcrm has.
slots["textasteriskcentered"] = 42
others = set()
for enc in ("ot1enc.def", "omsenc.def", "omlenc.def"):
    others |= set(re.findall(r"\\DeclareText(?:Symbol|Command|Composite)\{?\\(\w+)\}?", open(kpse(enc)).read()))
ltx = open(kpse("latex.ltx")).read()
# The last `\DeclareTextSymbolDefault{\x}{ENC}` for each name wins (latex.ltx order).
default = {}
for m in re.finditer(r"^\\DeclareTextSymbolDefault\{\\(\w+)\}\{(\w+)\}", ltx, re.M):
    default[m.group(1)] = m.group(2)
table = sorted((n, s) for n, s in slots.items() if n not in others or default.get(n) == "TS1")
print(f"/// The text commands typeset from TS1 (`scripts/fontgen/gen_ts1_symbols.py`): name and slot,\n/// sorted by name.\npub static TS1_TEXT_SYMBOLS: [(&str, u8); {len(table)}] = [")
for n, s in table:
    print(f'    ("{n}", {s}),')
print("];")
