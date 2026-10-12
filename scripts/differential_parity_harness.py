#!/usr/bin/env python3
"""
RushTeX Automated Differential Regression & Perceptual Parity Test Harness.

Compiles mathematical expressions, table environments, and multi-page documents
using both RushTeX and TeX Live (pdfLaTeX), rasterizes outputs at 300 DPI, and performs
rigorous differential regression testing:
  - Multi-page document page-count parity assertions
  - Per-page exact bounding box dimensional ratios (Width, Height)
  - Multi-scale local window phase alignment
  - Bidirectional Chamfer contour distance (average pixel distance between ink boundaries)
  - Perceptual ink coverage with 1-pixel dilation tolerance
  - 3-channel false-color differential heatmaps:
      * Green: Perfect perceptual match (both in ink)
      * Red:   RushTeX only ink
      * Blue:  pdfLaTeX only ink

Suites Supported:
  - math: 26 canonical mathematical formulas covering TeX Appendix G & mlist.c
  - table: Tabular, booktabs, multicolumn, paragraph column, cline, and floating tables
  - page: Multi-page explicit page breaks, heading orphan protection, and streaming longtable
  - all: Runs all suites (math, table, and page)

Usage:
  python3 scripts/differential_parity_harness.py [--suite {math,table,page,all}] [--dpi 300] [--ci] [--diff-dir comparison/diff] [--save-all]
"""

import argparse
import glob
import os
import shutil
import subprocess
import sys
import tempfile
from typing import Dict, List, Optional, Tuple

try:
    import numpy as np
    from PIL import Image, ImageChops, ImageDraw
    from scipy.ndimage import distance_transform_edt, binary_dilation
except ImportError:
    print("Error: numpy, Pillow, and scipy are required. Install with: pip install numpy Pillow scipy", file=sys.stderr)
    sys.exit(1)

MATH_SPECIMENS: List[Tuple[str, str, str]] = [
    # 1. Rule 5 Atom Normalization & Basic Spacing
    ("Rule5", "rule5_basic_spacing", r"x + y \cdot z = \sin(\theta) \pm \sqrt{2}"),
    ("Rule5", "rule5_unary_and_parens", r"(+x) + [-y] \cdot (+z)"),
    ("Rule5", "rule5_consecutive_ops", r"x + = y \quad \text{and} \quad a = = b"),

    # 2. Appendix G Rule 11 Radicals
    ("Rule11", "rule11_simple_radical", r"\sqrt{x^2 + y^2}"),
    ("Rule11", "rule11_nested_radical", r"\sqrt{1 + \sqrt{2 + \sqrt{3}}}"),
    ("Rule11", "rule11_fraction_radical", r"\sqrt{\frac{a^2 + b^2}{c^2 + d^2}}"),
    ("Rule11", "rule11_degree_radical", r"\sqrt[3]{x} + \sqrt[n]{1 + x^n}"),

    # 3. Appendix G Rule 13 Large Operators & Limits
    ("Rule13", "rule13_summation_limits", r"\sum_{i=1}^n i^3 = \left( \sum_{i=1}^n i \right)^2"),
    ("Rule13", "rule13_infinite_product", r"\prod_{k=1}^\infty \left( 1 - \frac{1}{p_k^2} \right) = \frac{6}{\pi^2}"),
    ("Rule13", "rule13_definite_integral", r"\int_0^\infty e^{-x^2} \, \mathrm{d}x = \frac{\sqrt{\pi}}{2}"),
    ("Rule13", "rule13_surface_integrals", r"\oint_C \vec{F} \cdot \mathrm{d}\vec{r} = \iint_S (\nabla \times \vec{F}) \cdot \mathrm{d}\vec{S}"),
    ("Rule13", "rule13_explicit_limits", r"\sum\nolimits_{i=1}^n x_i + \int\limits_a^b f(t)\,\mathrm{d}t"),

    # 4. Appendix G Rule 15 Fractions & Slashed Fractions
    ("Rule15", "rule15_compound_fraction", r"\frac{a + b}{c + d} = 1 + \frac{x^2}{1 + x^3}"),
    ("Rule15", "rule15_binomial_coefficient", r"\binom{n}{k} = \frac{n!}{k!(n-k)!}"),
    ("Rule15", "rule15_slashed_fraction", r"\sfrac{3}{4} + \sfrac{1}{2} = \sfrac{5}{4}"),

    # 5. Appendix G Rule 18 Scripts & Accents
    ("Rule18", "rule18_superscripts_subscripts", r"x^2 + y_1 = z, \quad A_{i,j,k}^{2\prime}"),
    ("Rule18", "rule18_script_towers", r"2^{2^{2^x}} + e^{-x^2}"),
    ("Rule18", "rule18_rule12_accent_scripts", r"\hat{x}^2 + \bar{X}_{n+1}^k + \dot{r}^2"),
    ("Rule18", "rule18_wide_accents", r"\widehat{x + y} + \widetilde{A + B + C}"),
    ("Rule18", "rule18_bottom_and_overlay", r"\utilde{x} \quad \text{and} \quad x \not= y + \cancel{k}"),

    # 6. Appendix G Rule 19 Delimiters & Extensibles
    ("Rule19", "rule19_tall_enclosing_fences", r"\left( \frac{x^2 + 1}{x^2 - 1} \right) + \left[ 1 + \left( \frac{a}{b} \right)^3 \right]"),
    ("Rule19", "rule19_dynamic_middle_delim", r"\left\{ x \in \mathbb{R} \;\middle|\; \sqrt{x} > 1 \right\}"),
    ("Rule19", "rule19_extensible_arrows", r"A \xrightarrow{f} B \xleftarrow{g \circ h} C"),
    ("Rule19", "rule19_extensible_braces", r"\overbrace{x_1 + \dots + x_n}^{n \text{ items}} = \underbrace{y_1 + \dots + y_m}_{m \text{ items}}"),

    # 7. Matrices and Grids
    ("Grids", "grid_pmatrix", r"\begin{pmatrix} 1 & 0 \\ 0 & 1 \end{pmatrix}"),
    ("Grids", "grid_bmatrix", r"\begin{bmatrix} a & b \\ c & d \end{bmatrix}"),
]

TABLE_SPECIMENS: List[Tuple[str, str, str]] = [
    ("Table", "table_booktabs", "examples/differential/table_booktabs.tex"),
    ("Table", "table_multicolumn", "examples/differential/table_multicolumn.tex"),
    ("Table", "table_paragraph_col", "examples/differential/table_paragraph_col.tex"),
    ("Table", "table_cline", "examples/differential/table_cline.tex"),
    ("Table", "table_float_caption", "examples/differential/table_float_caption.tex"),
]

PAGE_SPECIMENS: List[Tuple[str, str, str]] = [
    ("Pagination", "multipage_breaks", "examples/differential/multipage_breaks.tex"),
    ("Pagination", "multipage_orphan", "examples/differential/multipage_orphan.tex"),
    ("Pagination", "multipage_longtable", "examples/differential/multipage_longtable.tex"),
]


def find_rushtex_binary(explicit_path: Optional[str] = None) -> str:
    if explicit_path and os.path.isfile(explicit_path):
        return os.path.abspath(explicit_path)
    for cand in ["target/release/rushtex", "target/debug/rushtex"]:
        if os.path.isfile(cand):
            return os.path.abspath(cand)
    raise FileNotFoundError("Could not find rushtex binary. Build it first with 'cargo build' or 'cargo build --release'.")


def check_tool_dependencies():
    for tool in ["pdflatex", "pdftoppm"]:
        if shutil.which(tool) is None:
            raise RuntimeError(f"Required external dependency '{tool}' is not installed in PATH.")


def compile_and_rasterize_formula(
    rushtex_bin: str,
    name: str,
    expr: str,
    dpi: int,
    tmpdir: str,
) -> Tuple[Optional[List[Image.Image]], Optional[List[Image.Image]]]:
    tex_source = (
        r"\documentclass{article}" "\n"
        r"\usepackage{lmodern}" "\n"
        r"\usepackage{amsmath}" "\n"
        r"\usepackage{amssymb}" "\n"
        r"\usepackage{xfrac}" "\n"
        r"\usepackage{cancel}" "\n"
        r"\usepackage{accents}" "\n"
        r"\providecommand{\utilde}[1]{\underaccent{\tilde}{#1}}" "\n"
        r"\pagestyle{empty}" "\n"
        r"\begin{document}" "\n"
        r"\[" "\n"
        + expr + "\n"
        r"\]" "\n"
        r"\end{document}" "\n"
    )

    tex_file = os.path.join(tmpdir, f"{name}.tex")
    with open(tex_file, "w", encoding="utf-8") as f:
        f.write(tex_source)

    # 1. RushTeX
    rush_pdf = os.path.join(tmpdir, f"{name}_rush.pdf")
    res_r = subprocess.run([rushtex_bin, tex_file, "-o", rush_pdf], capture_output=True, text=True)
    if res_r.returncode != 0 or not os.path.isfile(rush_pdf):
        print(f"[{name}] RushTeX compilation error:\n{res_r.stderr}", file=sys.stderr)
        return None, None

    # 2. pdfLaTeX
    res_p = subprocess.run(
        ["pdflatex", "-interaction=nonstopmode", f"{name}.tex"],
        cwd=tmpdir,
        capture_output=True,
        text=True,
    )
    pdf_pdf = os.path.join(tmpdir, f"{name}.pdf")
    if not os.path.isfile(pdf_pdf):
        print(f"[{name}] pdfLaTeX compilation error:\n{res_p.stdout}", file=sys.stderr)
        return None, None

    # 3. Rasterize
    rush_prefix = os.path.join(tmpdir, f"{name}_rush_p")
    subprocess.run(["pdftoppm", "-png", "-r", str(dpi), rush_pdf, rush_prefix], check=True, stdout=subprocess.DEVNULL)
    r_files = sorted(glob.glob(f"{rush_prefix}-*.png"))

    pdf_prefix = os.path.join(tmpdir, f"{name}_pdf_p")
    subprocess.run(["pdftoppm", "-png", "-r", str(dpi), pdf_pdf, pdf_prefix], check=True, stdout=subprocess.DEVNULL)
    p_files = sorted(glob.glob(f"{pdf_prefix}-*.png"))

    if not r_files or not p_files:
        return None, None

    return [Image.open(f).convert("L") for f in r_files], [Image.open(f).convert("L") for f in p_files]


def compile_and_rasterize_doc(
    rushtex_bin: str,
    name: str,
    doc_path: str,
    dpi: int,
    tmpdir: str,
) -> Tuple[Optional[List[Image.Image]], Optional[List[Image.Image]]]:
    if not os.path.isfile(doc_path):
        print(f"[{name}] Document path not found: {doc_path}", file=sys.stderr)
        return None, None

    doc_abs = os.path.abspath(doc_path)
    rush_pdf = os.path.join(tmpdir, f"{name}_rush.pdf")
    res_r = subprocess.run([rushtex_bin, doc_abs, "-o", rush_pdf], capture_output=True, text=True)
    if res_r.returncode != 0 or not os.path.isfile(rush_pdf):
        print(f"[{name}] RushTeX compilation error:\n{res_r.stderr}", file=sys.stderr)
        return None, None

    pdf_pdf = os.path.join(tmpdir, f"{name}_pdf.pdf")
    res_p = subprocess.run(
        ["pdflatex", "-interaction=nonstopmode", f"-jobname={name}_pdf", doc_abs],
        cwd=tmpdir,
        capture_output=True,
        text=True,
    )
    if not os.path.isfile(pdf_pdf):
        print(f"[{name}] pdfLaTeX compilation error:\n{res_p.stdout}", file=sys.stderr)
        return None, None

    rush_prefix = os.path.join(tmpdir, f"{name}_rush_p")
    subprocess.run(["pdftoppm", "-png", "-r", str(dpi), rush_pdf, rush_prefix], check=True, stdout=subprocess.DEVNULL)
    r_files = sorted(glob.glob(f"{rush_prefix}-*.png"))

    pdf_prefix = os.path.join(tmpdir, f"{name}_pdf_p")
    subprocess.run(["pdftoppm", "-png", "-r", str(dpi), pdf_pdf, pdf_prefix], check=True, stdout=subprocess.DEVNULL)
    p_files = sorted(glob.glob(f"{pdf_prefix}-*.png"))

    if not r_files or not p_files:
        return None, None

    return [Image.open(f).convert("L") for f in r_files], [Image.open(f).convert("L") for f in p_files]


def compute_perceptual_metrics(
    im_rush: Image.Image,
    im_pdf: Image.Image,
    is_standalone_crop: bool = True,
    suite_kind: str = "math",
) -> Dict[str, float]:
    """
    Computes rigorous geometric and perceptual differential metrics:
      - Dimensional ratio of ink bounding boxes (Width, Height)
      - Multi-scale local window phase alignment
      - Bidirectional Chamfer distance (mean pixel distance between nearest ink pixels)
      - Perceptual coverage with 1-pixel dilation tolerance
    """
    inv_r = ImageChops.invert(im_rush)
    inv_p = ImageChops.invert(im_pdf)
    bbox_r = inv_r.getbbox()
    bbox_p = inv_p.getbbox()

    if not bbox_r or not bbox_p:
        return {
            "rush_w": 0, "rush_h": 0, "pdf_w": 0, "pdf_h": 0,
            "w_ratio": 0.0, "h_ratio": 0.0, "chamfer": 99.0, "coverage_1px": 0.0,
            "grade": "FAIL", "passed": False,
        }

    crop_r = im_rush.crop(bbox_r) if is_standalone_crop else im_rush
    crop_p = im_pdf.crop(bbox_p) if is_standalone_crop else im_pdf

    w_r, h_r = crop_r.size
    w_p, h_p = crop_p.size

    w_ratio = min(w_r, w_p) / max(w_r, w_p) if max(w_r, w_p) > 0 else 1.0
    h_ratio = min(h_r, h_p) / max(h_r, h_p) if max(h_r, h_p) > 0 else 1.0

    # Common canvas with 32-pixel padding for shift search
    cw = max(w_r, w_p) + 32
    ch = max(h_r, h_p) + 32

    c_r = Image.new("L", (cw, ch), 255)
    c_p = Image.new("L", (cw, ch), 255)

    base_r_x = (cw - w_r) // 2
    base_r_y = (ch - h_r) // 2
    c_r.paste(crop_r, (base_r_x, base_r_y))

    base_p_x = (cw - w_p) // 2
    base_p_y = (ch - h_p) // 2
    c_p.paste(crop_p, (base_p_x, base_p_y))

    arr_r = np.array(c_r, dtype=np.float32) / 255.0
    arr_p = np.array(c_p, dtype=np.float32) / 255.0

    ink_r = arr_r < 0.85
    ink_p = arr_p < 0.85

    # Multi-scale local window search to find true minimum Chamfer alignment
    best_chamfer = 999.0
    best_dx = 0
    best_dy = 0
    dt_r = distance_transform_edt(~ink_r)

    step = 2 if (cw * ch) > 200000 else 1
    for dy in range(-6, 7, step):
        for dx in range(-10, 11, step):
            shifted_ink_p = np.roll(np.roll(ink_p, dy, axis=0), dx, axis=1)
            dt_p = distance_transform_edt(~shifted_ink_p)
            d_r = dt_p[ink_r].mean() if ink_r.sum() > 0 else 0.0
            d_p = dt_r[shifted_ink_p].mean() if shifted_ink_p.sum() > 0 else 0.0
            c = (d_r + d_p) / 2.0
            if c < best_chamfer:
                best_chamfer = c
                best_dx = dx
                best_dy = dy

    # Fine search around best point:
    for dy in [best_dy - 1, best_dy, best_dy + 1]:
        for dx in [best_dx - 1, best_dx, best_dx + 1]:
            shifted_ink_p = np.roll(np.roll(ink_p, dy, axis=0), dx, axis=1)
            dt_p = distance_transform_edt(~shifted_ink_p)
            d_r = dt_p[ink_r].mean() if ink_r.sum() > 0 else 0.0
            d_p = dt_r[shifted_ink_p].mean() if shifted_ink_p.sum() > 0 else 0.0
            c = (d_r + d_p) / 2.0
            if c < best_chamfer:
                best_chamfer = c
                best_dx = dx
                best_dy = dy

    aligned_ink_p = np.roll(np.roll(ink_p, best_dy, axis=0), best_dx, axis=1)

    # Perceptual 1-pixel dilation coverage
    dilated_p = binary_dilation(aligned_ink_p, iterations=1)
    dilated_r = binary_dilation(ink_r, iterations=1)
    cov_r = float(np.logical_and(ink_r, dilated_p).sum()) / ink_r.sum() if ink_r.sum() > 0 else 1.0
    cov_p = float(np.logical_and(aligned_ink_p, dilated_r).sum()) / aligned_ink_p.sum() if aligned_ink_p.sum() > 0 else 1.0
    coverage_1px = (cov_r + cov_p) / 2.0

    # Grading classification by suite kind
    if suite_kind == "math":
        if best_chamfer <= 3.5 and w_ratio >= 0.85 and h_ratio >= 0.70:
            grade = "A"
            passed = True
        elif best_chamfer <= 9.0 and w_ratio >= 0.75 and h_ratio >= 0.60:
            grade = "B"
            passed = True
        else:
            grade = "FAIL"
            passed = False
    elif suite_kind == "table":
        if best_chamfer <= 15.0 and w_ratio >= 0.80 and h_ratio >= 0.60:
            grade = "A"
            passed = True
        elif best_chamfer <= 50.0 and w_ratio >= 0.65 and h_ratio >= 0.45:
            grade = "B"
            passed = True
        else:
            grade = "FAIL"
            passed = False
    else:  # page / pagination
        if best_chamfer <= 15.0 and w_ratio >= 0.80 and h_ratio >= 0.60:
            grade = "A"
            passed = True
        elif best_chamfer <= 50.0 and w_ratio >= 0.70 and h_ratio >= 0.35:
            grade = "B"
            passed = True
        else:
            grade = "FAIL"
            passed = False

    return {
        "rush_w": w_r,
        "rush_h": h_r,
        "pdf_w": w_p,
        "pdf_h": h_p,
        "w_ratio": w_ratio,
        "h_ratio": h_ratio,
        "chamfer": float(best_chamfer),
        "coverage_1px": coverage_1px,
        "best_shift_x": best_dx,
        "best_shift_y": best_dy,
        "grade": grade,
        "passed": passed,
    }


def generate_diff_heatmap(
    im_rush: Image.Image,
    im_pdf: Image.Image,
    metrics: Dict[str, float],
    out_path: str,
):
    inv_r = ImageChops.invert(im_rush)
    inv_p = ImageChops.invert(im_pdf)
    bbox_r = inv_r.getbbox() or (0, 0, im_rush.width, im_rush.height)
    bbox_p = inv_p.getbbox() or (0, 0, im_pdf.width, im_pdf.height)

    crop_r = im_rush.crop(bbox_r)
    crop_p = im_pdf.crop(bbox_p)

    w_r, h_r = crop_r.size
    w_p, h_p = crop_p.size
    cw = max(w_r, w_p) + 16
    ch = max(h_r, h_p) + 16

    c_r = Image.new("L", (cw, ch), 255)
    c_p = Image.new("L", (cw, ch), 255)
    c_r.paste(crop_r, ((cw - w_r) // 2, (ch - h_r) // 2))
    dx = int(metrics.get("best_shift_x", 0))
    dy = int(metrics.get("best_shift_y", 0))
    c_p.paste(crop_p, ((cw - w_p) // 2 + dx, (ch - h_p) // 2 + dy))

    arr_r = np.array(c_r, dtype=np.float32) / 255.0
    arr_p = np.array(c_p, dtype=np.float32) / 255.0

    ink_r = arr_r < 0.85
    ink_p = arr_p < 0.85

    diff_rgb = np.ones((ch, cw, 3), dtype=np.uint8) * 255
    both = np.logical_and(ink_r, ink_p)
    only_r = np.logical_and(ink_r, ~ink_p)
    only_p = np.logical_and(~ink_r, ink_p)

    diff_rgb[both] = [34, 139, 34]       # Forest green
    diff_rgb[only_r] = [220, 20, 60]     # Crimson red
    diff_rgb[only_p] = [30, 144, 255]    # Dodger blue

    diff_img = Image.fromarray(diff_rgb)

    comp_w = cw * 3 + 40
    comp_h = ch + 60
    composite = Image.new("RGB", (comp_w, comp_h), (245, 245, 247))
    draw = ImageDraw.Draw(composite)

    draw.text((10, 10), "RushTeX Output", fill=(20, 20, 20))
    draw.text((cw + 20, 10), "pdfLaTeX Reference", fill=(20, 20, 20))
    draw.text(
        (cw * 2 + 30, 10),
        f"Diff Heatmap (Chamfer: {metrics['chamfer']:.2f}px, Cov: {metrics['coverage_1px']*100:.1f}%)",
        fill=(20, 20, 20),
    )

    composite.paste(c_r.convert("RGB"), (10, 35))
    composite.paste(c_p.convert("RGB"), (cw + 20, 35))
    composite.paste(diff_img, (cw * 2 + 30, 35))

    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    composite.save(out_path)


def run_suite(
    rushtex_bin: str,
    suite_name: str,
    dpi: int,
    diff_dir: Optional[str],
    ci_mode: bool,
    save_all: bool,
) -> bool:
    print(f"================================================================================")
    print(f"RushTeX Automated Differential Regression Harness (DPI: {dpi})")
    print(f"Compiler:  {rushtex_bin}")
    print(f"Reference: TeX Live (pdflatex)")
    print(f"Suite:     {suite_name.upper()}")
    print(f"================================================================================\n")

    tests_to_run = []
    if suite_name in ("math", "all"):
        tests_to_run.extend([("math", cat, name, expr) for cat, name, expr in MATH_SPECIMENS])
    if suite_name in ("table", "all"):
        tests_to_run.extend([("table", cat, name, path) for cat, name, path in TABLE_SPECIMENS])
    if suite_name in ("page", "all"):
        tests_to_run.extend([("page", cat, name, path) for cat, name, path in PAGE_SPECIMENS])

    total_items = 0
    grade_a_count = 0
    grade_b_count = 0
    failed_count = 0

    header = f"{'Category':<10} | {'Test Name':<26} | {'Pg':<3} | {'RushTeX':<10} | {'pdfLaTeX':<10} | {'W_Ratio':<7} | {'H_Ratio':<7} | {'Chamfer':<8} | {'Cov(1px)':<8} | {'Grade':<5}"
    print(header)
    print("-" * len(header))

    with tempfile.TemporaryDirectory() as tmpdir:
        for kind, cat, name, payload in tests_to_run:
            if kind == "math":
                r_pages, p_pages = compile_and_rasterize_formula(rushtex_bin, name, payload, dpi, tmpdir)
                is_standalone = True
            else:
                r_pages, p_pages = compile_and_rasterize_doc(rushtex_bin, name, payload, dpi, tmpdir)
                is_standalone = True

            if r_pages is None or p_pages is None:
                print(f"{cat:<10} | {name:<26} | {'-':<3} | {'ERR':<10} | {'ERR':<10} | {'-':<7} | {'-':<7} | {'-':<8} | {'-':<8} | FAIL")
                failed_count += 1
                total_items += 1
                continue

            # Verify page count match
            if len(r_pages) != len(p_pages):
                print(
                    f"{cat:<10} | {name:<26} | {'ALL':<3} | "
                    f"{len(r_pages)} pages    | {len(p_pages)} pages    | "
                    f"{'-':<7} | {'-':<7} | {'-':<8} | {'-':<8} | FAIL (PgCount)"
                )
                failed_count += 1
                total_items += 1
                continue

            num_pages = len(r_pages)
            for page_idx in range(num_pages):
                total_items += 1
                im_rush = r_pages[page_idx]
                im_pdf = p_pages[page_idx]

                metrics = compute_perceptual_metrics(im_rush, im_pdf, is_standalone_crop=is_standalone, suite_kind=kind)
                grade = metrics["grade"]

                if grade == "A":
                    grade_a_count += 1
                elif grade == "B":
                    grade_b_count += 1
                else:
                    failed_count += 1

                w_r, h_r = int(metrics["rush_w"]), int(metrics["rush_h"])
                w_p, h_p = int(metrics["pdf_w"]), int(metrics["pdf_h"])
                r_dim = f"{w_r}x{h_r}"
                p_dim = f"{w_p}x{h_p}"
                pg_label = f"{page_idx+1}/{num_pages}" if num_pages > 1 else "1"

                print(
                    f"{cat:<10} | {name:<26} | {pg_label:<3} | {r_dim:<10} | {p_dim:<10} | "
                    f"{metrics['w_ratio']:<7.3f} | {metrics['h_ratio']:<7.3f} | {metrics['chamfer']:<6.2f}px | {metrics['coverage_1px']*100:<7.1f}% | {grade:<5}"
                )

                if diff_dir and (grade == "FAIL" or save_all):
                    diff_name = f"{name}_p{page_idx+1}_diff.png" if num_pages > 1 else f"{name}_diff.png"
                    diff_path = os.path.join(diff_dir, diff_name)
                    generate_diff_heatmap(im_rush, im_pdf, metrics, diff_path)

    passed_count = grade_a_count + grade_b_count
    print("\n" + "=" * 80)
    print(
        f"Summary: {passed_count}/{total_items} Pages/Tests Passed ({passed_count/total_items*100:.1f}%) "
        f"[Grade A: {grade_a_count}, Grade B: {grade_b_count}], Failed: {failed_count}."
    )
    print("=" * 80)

    if ci_mode and failed_count > 0:
        return False
    return True


def main():
    parser = argparse.ArgumentParser(description="RushTeX Automated Differential Regression Harness")
    parser.add_argument("--suite", choices=["math", "table", "page", "all"], default="all", help="Test suite to run")
    parser.add_argument("--dpi", type=int, default=300, help="Rasterization resolution (default: 300 DPI)")
    parser.add_argument("--ci", action="store_true", help="CI mode: exit with non-zero status on any failure")
    parser.add_argument("--diff-dir", type=str, default="comparison/diff", help="Directory for differential heatmaps")
    parser.add_argument("--save-all", action="store_true", help="Save diff heatmaps for all tests, including passing ones")
    parser.add_argument("--rushtex-bin", type=str, default=None, help="Path to rushtex executable")

    args = parser.parse_args()

    check_tool_dependencies()
    bin_path = find_rushtex_binary(args.rushtex_bin)

    success = run_suite(
        rushtex_bin=bin_path,
        suite_name=args.suite,
        dpi=args.dpi,
        diff_dir=args.diff_dir,
        ci_mode=args.ci,
        save_all=args.save_all,
    )

    if not success and args.ci:
        sys.exit(1)


if __name__ == "__main__":
    main()
