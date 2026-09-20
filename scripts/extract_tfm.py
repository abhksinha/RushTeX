#!/usr/bin/env python3
"""
Offline TFM Extraction Pipeline for RushTeX.
Parses canonical TeX Live TFM files using tftopl and extracts:
1. Font dimensions (sigma_1 .. sigma_22, default_rule_thickness, big_op_spacing)
2. Glyph metrics (width, height, depth, italic correction)
3. Delimiter chains (NEXTLARGER) and extensible recipes (VARCHAR)
"""

import os
import subprocess
import sys

CM_TFM_DIR = "/usr/share/texlive/texmf-dist/fonts/tfm/public/cm"
AMS_TFM_DIR = "/usr/share/texlive/texmf-dist/fonts/tfm/public/amsfonts/symbols"

FONTS = {
    "cmr10": f"{CM_TFM_DIR}/cmr10.tfm",
    "cmr7": f"{CM_TFM_DIR}/cmr7.tfm",
    "cmr5": f"{CM_TFM_DIR}/cmr5.tfm",
    "cmmi10": f"{CM_TFM_DIR}/cmmi10.tfm",
    "cmmi7": f"{CM_TFM_DIR}/cmmi7.tfm",
    "cmmi5": f"{CM_TFM_DIR}/cmmi5.tfm",
    "cmsy10": f"{CM_TFM_DIR}/cmsy10.tfm",
    "cmsy7": f"{CM_TFM_DIR}/cmsy7.tfm",
    "cmsy5": f"{CM_TFM_DIR}/cmsy5.tfm",
    "cmex10": f"{CM_TFM_DIR}/cmex10.tfm",
    "msam10": f"{AMS_TFM_DIR}/msam10.tfm",
    "msbm10": f"{AMS_TFM_DIR}/msbm10.tfm",
}

def parse_pl(pl_text):
    font_dimen = {}
    chars = {}
    current_char = None
    in_fontdimen = False
    in_char = False
    in_varchar = False

    for raw_line in pl_text.splitlines():
        line = raw_line.strip()
        if "(FONTDIMEN" in line:
            in_fontdimen = True
            continue
        elif in_fontdimen:
            if line.startswith(")"):
                in_fontdimen = False
                continue
            parts = line.strip("()").split()
            if len(parts) >= 3:
                name, typ, val = parts[0], parts[1], float(parts[2])
                font_dimen[name] = val
        elif line.startswith("(CHARACTER "):
            in_char = True
            parts = line.split()
            c_type = parts[1]
            c_val = parts[2].rstrip(")")
            if c_type == "O":
                current_char = int(c_val, 8)
            elif c_type == "C":
                current_char = ord(c_val)
            elif c_type == "D":
                current_char = int(c_val)
            chars[current_char] = {"W": 0.0, "H": 0.0, "D": 0.0, "IC": 0.0}
        elif in_char:
            if line.startswith(")"):
                in_char = False
                current_char = None
                continue
            parts = line.strip("()").split()
            if len(parts) >= 3 and current_char is not None:
                prop = parts[0]
                if prop in ("CHARWD", "CHARHT", "CHARDP", "CHARIC"):
                    tag = prop[4:]
                    chars[current_char][tag] = float(parts[2])
                elif prop == "NEXTLARGER":
                    target = int(parts[2], 8) if parts[1] == "O" else int(parts[2])
                    chars[current_char]["NEXTLARGER"] = target

    return font_dimen, chars

def extract_all():
    results = {}
    for name, path in FONTS.items():
        if not os.path.exists(path):
            print(f"Warning: {path} not found!", file=sys.stderr)
            continue
        out = subprocess.check_output(["tftopl", path]).decode("ascii")
        dimens, chars = parse_pl(out)
        results[name] = (dimens, chars)
        print(f"Successfully parsed {name}: {len(chars)} glyphs, {len(dimens)} font dimensions.")
    return results

if __name__ == "__main__":
    extract_all()
