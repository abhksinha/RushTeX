//! Unit tests for the mathematical typography subsystem.

#[cfg(test)]
mod tests {
    use crate::backend::fonts::{Face, FontFamily};
    use crate::compile_str;
    use crate::layout::layout_engine::WordKind;
    use crate::math::math_ast::{MathNode, MathStyle, MatrixDelim};
    use crate::math::math_box::MathBox;
    use crate::math::math_fonts::lookup_math_command;
    use crate::math::math_layout::{measure_math, render_math};
    use crate::math::math_parser::parse_math_str;
    use crate::math::math_spacing::{inter_atom_spacing, MathAtomKind, MATH_SPACING_TABLE};

    #[test]
    fn test_math_cache_fit_invariants() {
        println!("MathNode size: {}", std::mem::size_of::<MathNode>());
        assert_eq!(std::mem::size_of::<MathBox>(), 16);
        assert_eq!(std::mem::size_of::<MathAtomKind>(), 1);
        assert_eq!(std::mem::size_of::<MathStyle>(), 1);
        assert_eq!(std::mem::size_of::<WordKind>(), 1);
        assert_eq!(std::mem::size_of_val(&MATH_SPACING_TABLE), 64);
        assert_eq!(
            std::mem::size_of::<crate::math::math_fonts::MathFontParameters>(),
            64
        );
        assert_eq!(
            std::mem::align_of::<crate::math::math_fonts::MathFontParameters>(),
            64
        );
        assert_eq!(
            std::mem::size_of::<crate::frontend::frontend_ast::Block>(),
            32
        );
        assert_eq!(
            std::mem::size_of::<crate::frontend::frontend_ast::Inline>(),
            32
        );
    }

    #[test]
    fn test_math_font_parameters_and_italic_correction() {
        use crate::math::math_fonts::{
            italic_correction, math_font_parameters, symbol_italic_correction,
        };
        let times_params = math_font_parameters(FontFamily::Times);
        assert_eq!(times_params.axis_height, 250);
        assert_eq!(times_params.rule_thickness, 56);
        assert_eq!(times_params.num1, 680);
        assert_eq!(times_params.denom1, 680);

        let cm_params = math_font_parameters(FontFamily::ComputerModern);
        assert_eq!(cm_params.axis_height, 250);
        assert_eq!(cm_params.rule_thickness, 40);
        assert_eq!(cm_params.num1, 677);
        assert_eq!(cm_params.denom1, 686);

        // Italic correction for 'f' is significant in italic face, 0 in roman
        let ic_f_italic = italic_correction('f', Face::Italic, FontFamily::Times);
        assert!(ic_f_italic >= 100);
        let ic_f_roman = italic_correction('f', Face::Roman, FontFamily::Times);
        assert_eq!(ic_f_roman, 0);

        // Integral symbol has italic correction
        let ic_int = symbol_italic_correction(0xF2);
        assert_eq!(ic_int, 100);
    }

    #[test]
    fn test_content_sensitive_matrix_layout() {
        let node =
            parse_math_str(r"\begin{pmatrix} \frac{1}{2} & x \\ y & \frac{3}{4} \end{pmatrix}");
        let mb = measure_math(&node, 10.0, FontFamily::Times, MathStyle::Display);
        // Ensure content-sensitive row heights accommodate fractions properly without collision
        assert!(mb.total_h() > 20.0);
        assert!(mb.w > 0.0);
    }

    #[test]
    fn test_math_spacing_table_rules() {
        // Ord followed by Bin in display style gets med_space (4/18 em = 0.2222...)
        let sp = inter_atom_spacing(MathAtomKind::Ord, MathAtomKind::Bin, true);
        assert!((sp - 4.0 / 18.0).abs() < 1e-4);

        // Ord followed by Rel in display style gets thick_space (5/18 em = 0.2777...)
        let sp_rel = inter_atom_spacing(MathAtomKind::Ord, MathAtomKind::Rel, true);
        assert!((sp_rel - 5.0 / 18.0).abs() < 1e-4);

        // In script style, Ord followed by Bin has zero space
        let sp_script = inter_atom_spacing(MathAtomKind::Ord, MathAtomKind::Bin, false);
        assert_eq!(sp_script, 0.0);
    }

    #[test]
    fn test_math_command_lookup() {
        let alpha = lookup_math_command("alpha").unwrap();
        assert_eq!(alpha.face, Face::Symbol);
        assert_eq!(alpha.glyph, "a");
        assert_eq!(alpha.kind, MathAtomKind::Ord);

        let sum = lookup_math_command("sum").unwrap();
        assert_eq!(sum.face, Face::Symbol);
        assert_eq!(sum.kind, MathAtomKind::Op);

        let leq = lookup_math_command("le").unwrap();
        assert_eq!(leq.face, Face::Symbol);
        assert_eq!(leq.kind, MathAtomKind::Rel);
    }

    #[test]
    fn test_parse_basic_equation() {
        let node = parse_math_str("E = mc^2");
        match node {
            MathNode::Row(nodes) => {
                assert!(!nodes.is_empty());
            }
            _ => panic!("Expected Row"),
        }
    }

    #[test]
    fn test_parse_fraction_and_scripts() {
        let node = parse_math_str(r"\frac{a + b}{c_1^2}");
        match node {
            MathNode::Fraction {
                num, den, has_rule, ..
            } => {
                assert!(has_rule);
                assert!(matches!(*num, MathNode::Row(_)));
                assert!(matches!(*den, MathNode::SubSup { .. }));
            }
            _ => panic!("Expected Fraction"),
        }
    }

    #[test]
    fn test_parse_matrix_environment() {
        let node = parse_math_str(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}");
        match node {
            MathNode::Matrix(data) => {
                assert_eq!(data.delim, MatrixDelim::Parentheses);
                assert_eq!(data.rows.len(), 2);
                assert_eq!(data.rows[0].len(), 2);
                assert_eq!(data.rows[1].len(), 2);
            }
            _ => panic!("Expected Matrix"),
        }
    }

    #[test]
    fn test_parse_calculus_integrals_and_derivatives() {
        let doc_src = r"\documentclass{article}
\begin{document}
The fundamental theorem of calculus:
\[
\int_a^b f(x) \mathrm{d}x = F(b) - F(a)
\]
and the Gaussian integral:
$$
\int_{-\infty}^\infty e^{-x^2} \mathrm{d}x = \sqrt{\pi}
$$
\end{document}";
        let pdf = compile_str(doc_src).expect("Compilation must succeed");
        assert!(!pdf.is_empty());
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn test_parse_physics_equations() {
        let doc_src = r"\documentclass{article}
\begin{document}
Newton's second law:
\begin{equation}
\vec{F} = m \vec{a}
\end{equation}
Maxwell's equations in vacuum:
\begin{equation*}
\nabla \cdot \mathbf{E} = 0, \quad \nabla \times \mathbf{B} = \frac{1}{c^2} \frac{\partial \mathbf{E}}{\partial t}
\end{equation*}
Schrodinger equation:
\[
i \hbar \frac{\partial}{\partial t} \Psi = \hat{H} \Psi
\]
Einstein field equations:
\begin{equation}
G_{\mu\nu} + \Lambda g_{\mu\nu} = \frac{8\pi G}{c^4} T_{\mu\nu}
\end{equation}
\end{document}";
        let pdf = compile_str(doc_src).expect("Compilation must succeed");
        assert!(!pdf.is_empty());
        assert!(pdf.starts_with(b"%PDF-1.4"));
    }

    #[test]
    fn test_inline_math_in_paragraph() {
        let doc_src = r"\documentclass{article}
\begin{document}
The mass-energy equivalence $E = mc^2$ was proposed by Einstein.
Alternatively, \( a^2 + b^2 = c^2 \) defines a right triangle.
\end{document}";
        let pdf = compile_str(doc_src).expect("Compilation must succeed");
        assert!(!pdf.is_empty());
    }

    #[test]
    fn test_equation_numbering() {
        let doc_src = r"\documentclass{article}
\begin{document}
First equation:
\begin{equation}
a = b + c
\end{equation}
Unnumbered equation:
\begin{equation*}
x = y + z
\end{equation*}
Second numbered equation:
\begin{equation}
e^{i\pi} + 1 = 0
\end{equation}
\end{document}";
        let doc = crate::tex_parser::parse(doc_src).expect("Parsing must succeed");
        let pages = crate::layout::layout_engine::layout(&doc);
        assert!(!pages.is_empty());

        // Check Paint operations for EquationNumber
        let mut eq_nums = Vec::new();
        for op in &pages[0].ops {
            if let crate::layout::layout_engine::Paint::EquationNumber { n, .. } = op {
                eq_nums.push(*n);
            }
        }
        assert_eq!(eq_nums, vec![1, 2]);
    }

    #[test]
    fn test_math_in_table_cell() {
        let doc_src = r"\documentclass{article}
\begin{document}
\begin{tabular}{|l|c|}
\hline
Constant & Formula \\
\hline
Speed of light & $c = 3 \times 10^8$ \\
Euler identity & $e^{i\pi} + 1 = 0$ \\
\hline
\end{tabular}
\end{document}";
        let pdf = compile_str(doc_src).expect("Table compilation with math must succeed");
        assert!(!pdf.is_empty());
    }

    #[test]
    fn test_math_measure_and_render() {
        let node = parse_math_str(r"\frac{\sqrt{x^2 + 1}}{2}");
        let mb = measure_math(&node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(mb.w > 0.0);
        assert!(mb.h > 0.0);

        let mut ops = Vec::new();
        let mut pool = Vec::new();
        let rb = render_math(
            &node,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            50.0,
            500.0,
            &mut ops,
            &mut pool,
        );
        assert_eq!(mb.w, rb.w);
        assert!(!ops.is_empty());
        // Verify fraction rule and radical rule were emitted
        let has_rule = ops
            .iter()
            .any(|op| matches!(op, crate::layout::layout_engine::Paint::Rule { .. }));
        assert!(has_rule);
    }

    #[test]
    fn test_delimiters_and_matrix_variants() {
        // Test vmatrix (single vertical bars), Vmatrix (double vertical bars), bmatrix (brackets), cases (left brace)
        let vmat = parse_math_str(r"\begin{vmatrix} a & b \\ c & d \end{vmatrix}");
        assert!(matches!(
            vmat,
            MathNode::Matrix(ref d) if d.delim == MatrixDelim::VBar
        ));
        let vmat_box = measure_math(&vmat, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(vmat_box.w > 0.0);

        let vmat_double = parse_math_str(r"\begin{Vmatrix} a & b \\ c & d \end{Vmatrix}");
        assert!(matches!(
            vmat_double,
            MathNode::Matrix(ref d) if d.delim == MatrixDelim::DoubleVBar
        ));
        let vmat_double_box =
            measure_math(&vmat_double, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(vmat_double_box.w > vmat_box.w); // Double vertical bar takes slightly more lateral space

        let bmat = parse_math_str(r"\begin{bmatrix} 1 & 0 \\ 0 & 1 \end{bmatrix}");
        assert!(matches!(
            bmat,
            MathNode::Matrix(ref d) if d.delim == MatrixDelim::Brackets
        ));
        let bmat_box = measure_math(&bmat, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(bmat_box.w > 0.0);

        let cases_node = parse_math_str(r"\begin{cases} 1 & x > 0 \\ 0 & x \le 0 \end{cases}");
        assert!(matches!(
            cases_node,
            MathNode::Matrix(ref d) if d.delim == MatrixDelim::LeftBrace
        ));
        let cases_box = measure_math(&cases_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(cases_box.w > 0.0);
    }

    #[test]
    fn test_accents_and_operators() {
        let vec_node = parse_math_str(r"\vec{v} + \hat{x} + \tilde{y} + \bar{z}");
        let vec_box = measure_math(&vec_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(vec_box.w > 0.0);
        assert!(vec_box.h > 0.0);

        let over_node = parse_math_str(r"\overline{A + B} + \underline{C + D}");
        let over_box = measure_math(&over_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(over_box.w > 0.0);

        let ops_node = parse_math_str(
            r"\coprod_{i=1}^n x_i + \bigcap_{k} A_k + \bigcup_{j} B_j + \liminf_{n \to \infty} a_n",
        );
        let ops_box = measure_math(&ops_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(ops_box.w > 0.0);
    }

    #[test]
    fn test_tex_appendix_g_rule_5_spacing() {
        let unary = parse_math_str("-x");
        let unary_box = measure_math(&unary, 10.0, FontFamily::Times, MathStyle::Display);

        let x_only = parse_math_str("x");
        let x_box = measure_math(&x_only, 10.0, FontFamily::Times, MathStyle::Display);

        let minus_only = parse_math_str("-");
        let minus_box = measure_math(&minus_only, 10.0, FontFamily::Times, MathStyle::Display);

        // In unary "-x", Rule 5 downgrades "-" to Ord, so Ord + Ord has 0 inter-atom space.
        // Therefore unary_box.w should equal minus_box.w + x_box.w
        assert!((unary_box.w - (minus_box.w + x_box.w)).abs() < 1e-3);

        // In binary "y - x", "-" is preceded and followed by Ord, so it receives MedSpace (4/18 em) on both sides.
        let binary = parse_math_str("y - x");
        let binary_box = measure_math(&binary, 10.0, FontFamily::Times, MathStyle::Display);

        let y_only = parse_math_str("y");
        let y_box = measure_math(&y_only, 10.0, FontFamily::Times, MathStyle::Display);

        let med_space = crate::math::math_spacing::atom_spacing(
            MathAtomKind::Ord,
            MathAtomKind::Bin,
            true,
            true,
        ) * 10.0;
        let expected_binary_w = y_box.w + med_space + minus_box.w + med_space + x_box.w;
        assert!((binary_box.w - expected_binary_w).abs() < 1e-3);
    }

    #[test]
    fn test_math_cache_alignment_and_size_invariants() {
        use std::mem::{size_of, size_of_val};
        assert_eq!(size_of::<MathNode>(), 32);
        assert_eq!(size_of::<crate::math::math_box::MathBox>(), 16);
        assert_eq!(size_of::<crate::math::math_fonts::MathFontParameters>(), 64);
        assert_eq!(
            size_of_val(&crate::math::math_spacing::MATH_SPACING_TABLE),
            64
        );
        assert_eq!(size_of::<crate::math::math_fonts::RadicalMetrics>(), 16);
        assert_eq!(size_of::<crate::math::math_fonts::DelimiterMetrics>(), 16);
        assert_eq!(size_of::<crate::math::math_fonts::BigOpMetrics>(), 16);
        assert_eq!(size_of::<crate::math::math_fonts::AccentMetrics>(), 32);
        assert_eq!(size_of::<crate::math::math_fonts::MatrixMetrics>(), 16);
        assert_eq!(size_of::<crate::math::math_fonts::HBarMetrics>(), 16);
        assert_eq!(size_of::<crate::math::math_tfm::GlyphMetric>(), 8);
        assert_eq!(size_of::<crate::math::math_tfm::DelimiterRecipe>(), 4);
        assert_eq!(size_of::<crate::math::math_spacing::SpacingLut>(), 768);
        assert_eq!(
            std::mem::align_of::<crate::math::math_spacing::SpacingLut>(),
            64
        );
    }

    #[test]
    fn test_appendix_f_metrics_and_radical_pi_centering() {
        use crate::math::math_tfm::{
            cmex10_metric, cmmi10_metric, cmsy10_metric, RADICAL_RECIPE_CMEX, RADICAL_VARIANTS_CMEX,
        };
        use crate::math::{measure_math, parse_math_str, trace_math_geometry, MathStyle};

        // 1. Verify Appendix F canonical metrics
        let minus_metric = cmsy10_metric(0x00);
        assert_eq!(minus_metric.w, 778);
        assert_eq!(minus_metric.h, 583);
        assert_eq!(minus_metric.d, 83);

        let pi_metric = cmmi10_metric(0x19); // octal 31 = TeX \pi
        assert_eq!(pi_metric.w, 570);
        assert_eq!(pi_metric.h, 431);
        assert_eq!(pi_metric.d, 0);

        let small_rad = cmex10_metric(0x70);
        assert_eq!(small_rad.w, 1000);
        assert_eq!(small_rad.h, 40);
        assert_eq!(small_rad.d, 1160);

        assert_eq!(RADICAL_VARIANTS_CMEX[0], (40, 1160));
        assert_eq!(RADICAL_RECIPE_CMEX.top, 0x76);
        assert_eq!(RADICAL_RECIPE_CMEX.bot, 0x74);
        assert_eq!(RADICAL_RECIPE_CMEX.rep, 0x75);

        // 2. Verify TeX Appendix G Rule 11 vertical centering for \sqrt{\pi}
        let ast = parse_math_str(r"\sqrt{\pi}");
        let trace = trace_math_geometry(&ast, 10.0, FontFamily::Times, MathStyle::Text);

        // Locate radical rule and glyph
        let overbar = trace
            .rules
            .iter()
            .find(|r| r.h <= 1.0 && r.w >= 4.0)
            .expect("overbar rule");
        let radical_glyph = trace
            .glyphs
            .iter()
            .find(|g| g.text == "\u{00D6}")
            .expect("radical glyph");
        let pi_glyph = trace
            .glyphs
            .iter()
            .find(|g| g.text == "p")
            .expect("pi glyph");

        // Radicand \pi sits on baseline y=0
        assert_eq!(pi_glyph.y, 0.0);

        // Clearance above \pi to overbar bottom
        let top_gap = overbar.y - 4.50; // pi ink height is 4.50
                                        // Clearance below \pi baseline to radical hook bottom
        let bottom_gap = -(radical_glyph.y);

        // TeX Rule 11: Top and bottom clearances must be balanced within 0.6pt (not 8x skewed)
        assert!(
            (top_gap - bottom_gap).abs() < 0.6,
            "top_gap ({}) and bottom_gap ({}) must be balanced",
            top_gap,
            bottom_gap
        );

        // Total bounding box must properly encompass the balanced radical
        let bbox = measure_math(&ast, 10.0, FontFamily::Times, MathStyle::Text);
        assert!(bbox.h >= 6.0);
        assert!(bbox.d >= 2.0);
    }

    #[test]
    fn test_authoritative_metrics_lookups() {
        use crate::math::math_fonts::{
            big_op_metrics, delimiter_metrics, glyph_top_accent_offset, hbar_metrics,
            lookup_accent_metrics, matrix_metrics, radical_metrics, symbol_height_depth,
        };

        let rm = radical_metrics(FontFamily::Times);
        assert_eq!(rm.junction_y, 857);
        assert_eq!(rm.vertex_depth, 35);
        assert_eq!(rm.vspan, 892);

        let dm = delimiter_metrics(FontFamily::Times);
        assert_eq!(dm.min_scale, 1150);
        assert_eq!(dm.max_scale, 3800);
        assert_eq!(dm.bracket_cap_w, 280);

        let int_m = big_op_metrics("int", FontFamily::Times);
        assert_eq!(int_m.display_scale, 1350);
        assert_eq!(int_m.sup_kern, 120);
        assert_eq!(int_m.sub_kern, -220);

        let sum_m = big_op_metrics("sum", FontFamily::Times);
        assert_eq!(sum_m.display_scale, 1300);
        assert_eq!(sum_m.sup_kern, 0);

        let vec_acc = lookup_accent_metrics("vec", FontFamily::Times).unwrap();
        assert_eq!(vec_acc.glyph, "\u{00AE}");
        assert_eq!(vec_acc.lift, 50);
        assert!(!vec_acc.is_rule);

        let bar_acc = lookup_accent_metrics("bar", FontFamily::Times).unwrap();
        assert!(bar_acc.is_rule);
        assert_eq!(bar_acc.lift, 80);

        let mm = matrix_metrics(FontFamily::Times);
        assert_eq!(mm.min_row_height, 700);
        assert_eq!(mm.min_row_depth, 250);

        let hm = hbar_metrics(FontFamily::Times);
        assert_eq!(hm.bar_w, 350);
        assert_eq!(hm.bar_x, 50);

        // Top accent offset for 'f' in italic should be positive
        let f_offset = glyph_top_accent_offset('f', Face::Italic, FontFamily::Times);
        assert!(f_offset >= 50);

        // Adobe Symbol binary operators should have accurate centered height/depth
        let (pm_h, pm_d) = symbol_height_depth(0xB1);
        assert_eq!((pm_h, pm_d), (500, 50));
    }

    #[test]
    fn test_milestone_c_style_state_machine_and_cramped() {
        // 1. Bit-packed cramped/uncramped transitions
        assert_eq!(MathStyle::Display.cramped(), MathStyle::CrampedDisplay);
        assert_eq!(MathStyle::Text.cramped(), MathStyle::CrampedText);
        assert_eq!(MathStyle::Script.cramped(), MathStyle::CrampedScript);
        assert_eq!(
            MathStyle::ScriptScript.cramped(),
            MathStyle::CrampedScriptScript
        );

        assert_eq!(MathStyle::CrampedDisplay.uncramped(), MathStyle::Display);
        assert_eq!(MathStyle::CrampedText.uncramped(), MathStyle::Text);
        assert_eq!(MathStyle::CrampedScript.uncramped(), MathStyle::Script);
        assert_eq!(
            MathStyle::CrampedScriptScript.uncramped(),
            MathStyle::ScriptScript
        );

        assert!(MathStyle::CrampedDisplay.is_cramped());
        assert!(MathStyle::CrampedText.is_cramped());
        assert!(!MathStyle::Display.is_cramped());
        assert!(!MathStyle::Text.is_cramped());

        // 2. Matrix cell style transitions (Display -> Text, CrampedDisplay -> CrampedText)
        assert_eq!(MathStyle::Display.matrix_cell_style(), MathStyle::Text);
        assert_eq!(
            MathStyle::CrampedDisplay.matrix_cell_style(),
            MathStyle::CrampedText
        );
        assert_eq!(MathStyle::Text.matrix_cell_style(), MathStyle::Text);

        // 3. Vertical layout difference: Cramped styles lower superscript shifts (TeX Appendix G Rule 18)
        let x_sq = parse_math_str("x^2");
        let box_display = measure_math(&x_sq, 10.0, FontFamily::Times, MathStyle::Display);
        let box_cramped = measure_math(&x_sq, 10.0, FontFamily::Times, MathStyle::CrampedDisplay);

        assert!(
            box_display.h > box_cramped.h,
            "Uncramped display height ({}) must exceed cramped display height ({})",
            box_display.h,
            box_cramped.h
        );

        // 4. Radicands are rendered in cramped style (TeX Appendix G Rule 11)
        let sqrt_x_sq = parse_math_str(r"\sqrt{x^2}");
        let sqrt_box = measure_math(&sqrt_x_sq, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(sqrt_box.h > 0.0);
        assert!(sqrt_box.w > 0.0);
    }

    #[test]
    fn test_tex_appendix_g_rule_5_comprehensive_normalization() {
        use crate::math::math_spacing::normalize_atom_kinds;

        // Unary minus: [- , x] -> [Ord, Ord]
        let mut kinds1 = [MathAtomKind::Bin, MathAtomKind::Ord];
        normalize_atom_kinds(&mut kinds1);
        assert_eq!(kinds1, [MathAtomKind::Ord, MathAtomKind::Ord]);

        // Binary minus: [y, -, x] -> [Ord, Bin, Ord]
        let mut kinds2 = [MathAtomKind::Ord, MathAtomKind::Bin, MathAtomKind::Ord];
        normalize_atom_kinds(&mut kinds2);
        assert_eq!(
            kinds2,
            [MathAtomKind::Ord, MathAtomKind::Bin, MathAtomKind::Ord]
        );

        // Relational followed by unary: [x, =, -, y] -> [Ord, Rel, Ord, Ord]
        let mut kinds3 = [
            MathAtomKind::Ord,
            MathAtomKind::Rel,
            MathAtomKind::Bin,
            MathAtomKind::Ord,
        ];
        normalize_atom_kinds(&mut kinds3);
        assert_eq!(
            kinds3,
            [
                MathAtomKind::Ord,
                MathAtomKind::Rel,
                MathAtomKind::Ord,
                MathAtomKind::Ord
            ]
        );

        // Binary followed by unary: [x, +, -, y] -> [Ord, Bin, Ord, Ord]
        let mut kinds4 = [
            MathAtomKind::Ord,
            MathAtomKind::Bin,
            MathAtomKind::Bin,
            MathAtomKind::Ord,
        ];
        normalize_atom_kinds(&mut kinds4);
        assert_eq!(
            kinds4,
            [
                MathAtomKind::Ord,
                MathAtomKind::Bin,
                MathAtomKind::Ord,
                MathAtomKind::Ord
            ]
        );

        // Delimiter followed by unary: [(, -, x, )] -> [Open, Ord, Ord, Close]
        let mut kinds5 = [
            MathAtomKind::Open,
            MathAtomKind::Bin,
            MathAtomKind::Ord,
            MathAtomKind::Close,
        ];
        normalize_atom_kinds(&mut kinds5);
        assert_eq!(
            kinds5,
            [
                MathAtomKind::Open,
                MathAtomKind::Ord,
                MathAtomKind::Ord,
                MathAtomKind::Close
            ]
        );

        // Terminal binary: [x, +] -> [Ord, Ord]
        let mut kinds6 = [MathAtomKind::Ord, MathAtomKind::Bin];
        normalize_atom_kinds(&mut kinds6);
        assert_eq!(kinds6, [MathAtomKind::Ord, MathAtomKind::Ord]);

        // Operator followed by binary: [\log, +, x] -> [Op, Ord, Ord] (tex.web line 14331)
        let mut kinds_op = [MathAtomKind::Op, MathAtomKind::Bin, MathAtomKind::Ord];
        normalize_atom_kinds(&mut kinds_op);
        assert_eq!(kinds_op, [MathAtomKind::Op, MathAtomKind::Ord, MathAtomKind::Ord]);

        // End-to-end measurement: "x = - y" should have thick space after "=" and zero space between "-" and "y"
        let eq_expr = parse_math_str("x = - y");
        let eq_box = measure_math(&eq_expr, 10.0, FontFamily::Times, MathStyle::Display);

        let x_box = measure_math(
            &parse_math_str("x"),
            10.0,
            FontFamily::Times,
            MathStyle::Display,
        );
        let rel_box = measure_math(
            &parse_math_str("="),
            10.0,
            FontFamily::Times,
            MathStyle::Display,
        );
        let minus_box = measure_math(
            &parse_math_str("-"),
            10.0,
            FontFamily::Times,
            MathStyle::Display,
        );
        let y_box = measure_math(
            &parse_math_str("y"),
            10.0,
            FontFamily::Times,
            MathStyle::Display,
        );

        let thick_space = crate::math::math_spacing::atom_spacing(
            MathAtomKind::Ord,
            MathAtomKind::Rel,
            true,
            true,
        ) * 10.0;
        let expected_w =
            x_box.w + thick_space + rel_box.w + thick_space + minus_box.w + 0.0 + y_box.w;
        assert!(
            (eq_box.w - expected_w).abs() < 1e-3,
            "Expected width {}, got {}",
            expected_w,
            eq_box.w
        );
    }

    #[test]
    fn test_explicit_style_scopes_and_dfrac_tfrac() {
        // \displaystyle scope parsing
        let disp_expr = parse_math_str(r"\displaystyle \sum_{k=1}^n k");
        assert!(matches!(
            disp_expr,
            MathNode::StyleScope {
                style: MathStyle::Display,
                ..
            }
        ));

        // \dfrac vs \tfrac parsing and measurement
        let dfrac = parse_math_str(r"\dfrac{1}{2}");
        let tfrac = parse_math_str(r"\tfrac{1}{2}");

        assert!(matches!(
            dfrac,
            MathNode::StyleScope {
                style: MathStyle::Display,
                ..
            }
        ));
        assert!(matches!(
            tfrac,
            MathNode::StyleScope {
                style: MathStyle::Text,
                ..
            }
        ));

        // When measured under Text style, \dfrac forces Display parameters (taller clearance/shift)
        let dfrac_box = measure_math(&dfrac, 10.0, FontFamily::Times, MathStyle::Text);
        let tfrac_box = measure_math(&tfrac, 10.0, FontFamily::Times, MathStyle::Text);
        assert!(
            dfrac_box.total_h() > tfrac_box.total_h(),
            "\\dfrac total height ({}) should exceed \\tfrac total height ({})",
            dfrac_box.total_h(),
            tfrac_box.total_h()
        );
    }

    #[test]
    fn test_milestone_d_rule_18_script_constraints() {
        // 1. TeX Rule 18c: Deep descender in superscript forces larger elevation
        let x_simple_sup = parse_math_str("x^2");
        let x_fraction_sup = parse_math_str(r"x^{\frac{1}{2}}");

        let simple_box = measure_math(&x_simple_sup, 10.0, FontFamily::Times, MathStyle::Text);
        let frac_sup_box = measure_math(&x_fraction_sup, 10.0, FontFamily::Times, MathStyle::Text);

        // Fraction in superscript has deep descender, so total height must expand
        assert!(
            frac_sup_box.h > simple_box.h + 1.0,
            "Superscript with fraction must elevate higher ({} > {})",
            frac_sup_box.h,
            simple_box.h
        );

        // 2. TeX Rule 18d: Tall subscript forces larger drop
        let x_simple_sub = parse_math_str("x_1");
        let x_fraction_sub = parse_math_str(r"x_{\frac{1}{2}}");

        let simple_sub_box = measure_math(&x_simple_sub, 10.0, FontFamily::Times, MathStyle::Text);
        let frac_sub_box = measure_math(&x_fraction_sub, 10.0, FontFamily::Times, MathStyle::Text);

        assert!(
            frac_sub_box.d > simple_sub_box.d + 1.0,
            "Subscript with fraction must drop lower ({} > {})",
            frac_sub_box.d,
            simple_sub_box.d
        );

        // 3. TeX Rule 17: Slanted base vs Roman base italic correction
        let f_sup = parse_math_str("f^2");
        let f_sub = parse_math_str("f_1");
        let f_sub_sup = parse_math_str("f_1^2");

        let f_sup_box = measure_math(&f_sup, 10.0, FontFamily::Times, MathStyle::Display);
        let f_sub_box = measure_math(&f_sub, 10.0, FontFamily::Times, MathStyle::Display);
        let f_both_box = measure_math(&f_sub_sup, 10.0, FontFamily::Times, MathStyle::Display);

        // Because 'f' has a positive italic correction, superscript is shifted right,
        // so total width of f_1^2 must be at least as wide as f^2 and f_1
        assert!(f_both_box.w >= f_sup_box.w);
        assert!(f_both_box.w > f_sub_box.w);
    }

    #[test]
    fn test_milestone_d_rule_10_fractions_and_binomials() {
        // 1. Binomial coefficient parsing and delimiter wrapping
        let binom_node = parse_math_str(r"\binom{n}{k}");
        match binom_node {
            MathNode::Delimited(ref data) => {
                assert_eq!(data.open, "(");
                assert_eq!(data.close, ")");
                match *data.content {
                    MathNode::Fraction {
                        ref num,
                        ref den,
                        has_rule,
                        ..
                    } => {
                        assert!(!has_rule, "Binomial coefficient must have no rule");
                        assert!(matches!(**num, MathNode::Variable('n')));
                        assert!(matches!(**den, MathNode::Variable('k')));
                    }
                    _ => panic!("Expected Fraction content in Delimited"),
                }
            }
            _ => panic!("Expected Delimited node for binom"),
        }

        // 2. Binomial coefficient measurement: no rule drawn, centered vertically
        let binom_box = measure_math(&binom_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(binom_box.w > 0.0);
        assert!(binom_box.h > 0.0);
        assert!(binom_box.d > 0.0);

        // 3. Nested fractions: \frac{\frac{1}{2}}{\frac{3}{4}}
        let nested_frac = parse_math_str(r"\frac{\frac{1}{2}}{\frac{3}{4}}");
        let nested_box = measure_math(&nested_frac, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(nested_box.total_h() > 20.0);

        // 4. Fractions with rule emit Rule paint op, binom does not emit rule inside
        let mut ops = Vec::new();
        let mut pool = Vec::new();
        let standard_frac = parse_math_str(r"\frac{1}{2}");
        render_math(
            &standard_frac,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let rule_count = ops
            .iter()
            .filter(|op| matches!(op, crate::layout::layout_engine::Paint::Rule { .. }))
            .count();
        assert_eq!(rule_count, 1, "Standard fraction must emit exactly 1 rule");

        ops.clear();
        pool.clear();
        let pure_atop = MathNode::Fraction {
            num: Box::new(MathNode::Variable('n')),
            den: Box::new(MathNode::Variable('k')),
            has_rule: false,
            options: 0,
        };
        render_math(
            &pure_atop,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let no_rule_count = ops
            .iter()
            .filter(|op| matches!(op, crate::layout::layout_engine::Paint::Rule { .. }))
            .count();
        assert_eq!(
            no_rule_count, 0,
            "Fraction with has_rule: false must emit 0 rules"
        );
    }

    #[test]
    fn test_milestone_e_rule_19_delimiters_and_extensible_constructions() {
        use crate::font_metrics::FontFamily;
        use crate::layout::layout_engine::Paint;
        use crate::math::math_fonts::math_font_parameters;

        let params = math_font_parameters(FontFamily::Times);
        let axis_h = (params.axis_height as f32) * (10.0 * 0.001);

        // 1. TeX Appendix G Rule 19: Symmetric axis-centered delimiter sizing
        let asym_delim = parse_math_str(r"\left( \frac{1}{2} \right)");
        let asym_box = measure_math(&asym_delim, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(asym_box.w > 0.0);
        // Box center is at math axis: (box.h - box.d) == 2 * axis_h
        let center_offset = (asym_box.h - asym_box.d) - 2.0 * axis_h;
        assert!(
            center_offset.abs() < 1e-2,
            "Rule 19 requires delimiter to be strictly centered on the math axis: offset={}",
            center_offset
        );

        // 2. Syntax recognition: \langle, \rangle, \lfloor, \rfloor, \lceil, \rceil, \vert, \Vert, \left.
        let angle_delim = parse_math_str(r"\left\langle x \right\rangle");
        let angle_box = measure_math(&angle_delim, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(angle_box.w > 0.0);

        let floor_delim = parse_math_str(r"\left\lfloor x \right\rfloor");
        let floor_box = measure_math(&floor_delim, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(floor_box.w > 0.0);

        let ceil_delim = parse_math_str(r"\left\lceil x \right\rceil");
        let ceil_box = measure_math(&ceil_delim, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(ceil_box.w > 0.0);

        let vert_delim = parse_math_str(r"\left\vert x \right\vert");
        let vert_box = measure_math(&vert_delim, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(vert_box.w > 0.0);

        let double_vert_delim = parse_math_str(r"\left\Vert x \right\Vert");
        let dvert_box = measure_math(
            &double_vert_delim,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
        );
        assert!(dvert_box.w > vert_box.w);

        let null_left = parse_math_str(r"\left. \frac{a}{b} \right|");
        let null_box = measure_math(&null_left, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(null_box.w > 0.0);

        // 3. Extensible multi-piece assembly for large delimiters (tall 4-row matrix)
        let tall_matrix = parse_math_str(r"\begin{pmatrix} 1 \\ 2 \\ 3 \\ 4 \end{pmatrix}");
        let mut ops = Vec::new();
        let mut pool = Vec::new();
        render_math(
            &tall_matrix,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        assert!(!ops.is_empty());
        // Verify multi-piece assembly glyphs were emitted in pool
        let has_top_hook = pool.iter().any(|&s| s == "\u{00E6}" || s == "\u{00F6}");
        let has_bot_hook = pool.iter().any(|&s| s == "\u{00E8}" || s == "\u{00F8}");
        let has_extender = pool.iter().any(|&s| s == "\u{00E7}" || s == "\u{00F7}");
        assert!(
            has_top_hook && has_bot_hook && has_extender,
            "Tall matrix must assemble extensible parentheses from top, bottom, and extender pieces"
        );

        // 4. Extensible Radicals (Rule 11):
        // Small radical: single glyph \u{00D6} + 1 overbar rule
        ops.clear();
        pool.clear();
        let small_rad = parse_math_str(r"\sqrt{x}");
        render_math(
            &small_rad,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let small_rule_count = ops
            .iter()
            .filter(|op| matches!(op, Paint::Rule { .. }))
            .count();
        assert_eq!(
            small_rule_count, 1,
            "Small radical emits exactly 1 overbar rule"
        );

        // Large radical (matrix inside sqrt): multi-piece assembly emits 2 rules (vertical stem + overbar)
        ops.clear();
        pool.clear();
        let large_rad = parse_math_str(r"\sqrt{\begin{pmatrix} 1 & 0 \\ 0 & 1 \end{pmatrix}}");
        render_math(
            &large_rad,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let large_rule_count = ops
            .iter()
            .filter(|op| matches!(op, Paint::Rule { .. }))
            .count();
        assert_eq!(
            large_rule_count, 2,
            "Large radical must assemble with vertical stem rule and overbar rule (2 rules)"
        );
    }

    #[test]
    fn test_milestone_f_operators_and_accents() {
        use crate::font_metrics::FontFamily;
        use crate::layout::layout_engine::Paint;
        use crate::math::math_fonts::math_font_parameters;

        let params = math_font_parameters(FontFamily::Times);
        let axis_h = (params.axis_height as f32) * (10.0 * 0.001);

        // 1. Rule 13 Axis Centering for Large Operators
        let sum_node = parse_math_str(r"\sum");
        let mut ops = Vec::new();
        let mut pool = Vec::new();
        render_math(
            &sum_node,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        assert!(!ops.is_empty());
        let sum_glyph = ops
            .iter()
            .find(|op| matches!(op, Paint::GlyphRun { .. }))
            .expect("Sum must emit glyph run");
        if let Paint::GlyphRun { y, size, .. } = sum_glyph {
            let op_h = 0.850 * size;
            let op_d = 0.350 * size;
            let center_y = y + (op_h - op_d) * 0.5;
            assert!(
                (center_y - axis_h).abs() < 1e-2,
                "Rule 13: sum optical center ({}) must be aligned with math axis ({})",
                center_y,
                axis_h
            );
        }

        // 2. Rule 13a Limit Placement and Half-Italic Correction
        let sum_limits = parse_math_str(r"\sum_{i=0}^n");
        ops.clear();
        pool.clear();
        render_math(
            &sum_limits,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        assert!(ops.len() >= 3, "Must render sum, upper, and lower limits");

        // Slanted operator with \limits: \int\limits_0^1
        let int_limits = parse_math_str(r"\int\limits_0^1");
        ops.clear();
        pool.clear();
        render_math(
            &int_limits,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let sup_op = ops
            .iter()
            .find(|op| match op {
                Paint::GlyphRun { part_start, .. } => pool[*part_start as usize] == "1",
                _ => false,
            })
            .expect("Upper limit '1' found");
        let sub_op = ops
            .iter()
            .find(|op| match op {
                Paint::GlyphRun { part_start, .. } => pool[*part_start as usize] == "0",
                _ => false,
            })
            .expect("Lower limit '0' found");
        if let (Paint::GlyphRun { x: sup_x, .. }, Paint::GlyphRun { x: sub_x, .. }) =
            (sup_op, sub_op)
        {
            assert!(
                sup_x > sub_x,
                "Rule 13a: upper limit of slanted integral must be staggered to the right of lower limit: sup_x={}, sub_x={}",
                sup_x,
                sub_x
            );
        }

        // 3. Contour integral (\oint) and STEM operators
        ops.clear();
        pool.clear();
        let oint_node = parse_math_str(r"\oint f(z)\,dz");
        render_math(
            &oint_node,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let has_ring = pool.contains(&"\u{00B0}");
        let has_int = pool.contains(&"\u{00F2}");
        assert!(
            has_ring && has_int,
            "Contour integral must emit both integral stem and contour ring"
        );

        // 3b. Computer Modern native contour integral (CMEX10 0x48/0x49)
        ops.clear();
        pool.clear();
        render_math(
            &oint_node,
            10.0,
            FontFamily::ComputerModern,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        assert!(
            pool.contains(&"I"),
            "Display-style contour integral in CM must emit native CMEX10 0x49 ('I')"
        );
        assert!(
            !pool.contains(&"\u{000E}") && !pool.contains(&"\u{00B0}"),
            "Native CM contour integral must not emit synthetic circle overlay"
        );

        ops.clear();
        pool.clear();
        render_math(
            &oint_node,
            10.0,
            FontFamily::ComputerModern,
            MathStyle::Text,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        assert!(
            pool.contains(&"H"),
            "Text-style contour integral in CM must emit native CMEX10 0x48 ('H')"
        );

        let stem_ops =
            parse_math_str(r"\bigvee \bigwedge \bigoplus \bigotimes \bigodot \biguplus \bigsqcup");
        let stem_box = measure_math(&stem_ops, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(stem_box.w > 0.0);

        // 3c. Computer Modern native big operators (\bigoplus, \bigotimes, \bigodot, \bigsqcup)
        ops.clear();
        pool.clear();
        let bigop_test = parse_math_str(r"\bigoplus \bigotimes \bigodot \bigsqcup");
        render_math(
            &bigop_test,
            10.0,
            FontFamily::ComputerModern,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        assert!(pool.contains(&"M"), "Display \\bigoplus in CM must emit 0x4D ('M')");
        assert!(pool.contains(&"O"), "Display \\bigotimes in CM must emit 0x4F ('O')");
        assert!(pool.contains(&"K"), "Display \\bigodot in CM must emit 0x4B ('K')");
        assert!(pool.contains(&"G"), "Display \\bigsqcup in CM must emit 0x47 ('G')");

        // 3d. Delimiter resolution for vertical bars and slashes
        use crate::math::math_delim::resolve_cmex_delimiter;
        let (vert_chain, vert_recipe) = resolve_cmex_delimiter("|", true).expect("vert must resolve");
        assert_eq!(vert_chain.len(), 4);
        assert_eq!(vert_recipe.expect("vert recipe").rep, 0x0C);

        let (vert_d_chain, vert_d_recipe) = resolve_cmex_delimiter(r"\|", true).expect("double vert must resolve");
        assert_eq!(vert_d_chain.len(), 4);
        assert_eq!(vert_d_recipe.expect("double vert recipe").rep, 0x0D);

        let (slash_chain, slash_recipe) = resolve_cmex_delimiter("/", true).expect("slash must resolve");
        assert_eq!(slash_chain[0], 0x0E);
        assert!(slash_recipe.is_none());

        // 3e. Atom class coercion (\mathbin, \mathrel, \mathord, \mathop)
        let coerced_bin = parse_math_str(r"\mathbin{*}");
        assert_eq!(coerced_bin.atom_kind(), MathAtomKind::Bin);
        let coerced_rel = parse_math_str(r"\mathrel{:}");
        assert_eq!(coerced_rel.atom_kind(), MathAtomKind::Rel);
        let coerced_ord = parse_math_str(r"\mathord{+}");
        assert_eq!(coerced_ord.atom_kind(), MathAtomKind::Ord);

        // 3f. smallmatrix compact environment
        let sm_node = parse_math_str(r"\begin{smallmatrix} a & b \\ c & d \end{smallmatrix}");
        let sm_box = measure_math(&sm_node, 10.0, FontFamily::ComputerModern, MathStyle::Text);
        assert!(sm_box.w > 0.0 && sm_box.h > 0.0);

        // 3g. Skewchar optical accent kerning (CMMI10 char 127)
        let offset_a = crate::math::glyph_top_accent_offset('A', Face::MathItalic, FontFamily::ComputerModern);
        assert_eq!(offset_a, 139, "CMMI10 'A' skewchar kern must equal 139");
        let offset_f = crate::math::glyph_top_accent_offset('f', Face::MathItalic, FontFamily::ComputerModern);
        assert_eq!(offset_f, 167, "CMMI10 'f' skewchar kern must equal 167");

        // 4. Higher-order dot accents (Rule 12)
        ops.clear();
        pool.clear();
        let dddot_node = parse_math_str(r"\dddot{x}");
        render_math(
            &dddot_node,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let dddot_rules = ops
            .iter()
            .filter(|op| matches!(op, Paint::Rule { .. }))
            .count();
        assert_eq!(dddot_rules, 3, r"\dddot must emit exactly 3 dot rules");

        ops.clear();
        pool.clear();
        let ddddot_node = parse_math_str(r"\ddddot{x}");
        render_math(
            &ddddot_node,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let ddddot_rules = ops
            .iter()
            .filter(|op| matches!(op, Paint::Rule { .. }))
            .count();
        assert_eq!(ddddot_rules, 4, r"\ddddot must emit exactly 4 dot rules");

        // Mathring accent
        ops.clear();
        pool.clear();
        let mathring_node = parse_math_str(r"\mathring{A}");
        render_math(
            &mathring_node,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let has_mathring = pool.contains(&"\u{00B0}");
        assert!(has_mathring, "\\mathring must emit ring symbol");

        // 5. Wide accents and extensible over/under arrows
        let narrow_hat = measure_math(
            &parse_math_str(r"\hat{A}"),
            10.0,
            FontFamily::Times,
            MathStyle::Display,
        );
        let wide_hat = measure_math(
            &parse_math_str(r"\widehat{A + B + C}"),
            10.0,
            FontFamily::Times,
            MathStyle::Display,
        );
        assert!(
            wide_hat.w > narrow_hat.w * 2.0,
            "Wide hat compound expression must be wider than single char"
        );

        ops.clear();
        pool.clear();
        let over_arrow = parse_math_str(r"\overrightarrow{AB}");
        render_math(
            &over_arrow,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let has_arrowhead = pool.contains(&"\u{00AE}");
        let has_stem_rule = ops.iter().any(|op| matches!(op, Paint::Rule { .. }));
        assert!(
            has_arrowhead && has_stem_rule,
            r"\overrightarrow must emit arrowhead glyph and stem rule"
        );

        ops.clear();
        pool.clear();
        let back_arrow = parse_math_str(r"\overleftarrow{AB}");
        render_math(
            &back_arrow,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let has_left_arrowhead = pool.contains(&"\u{00AC}");
        let has_left_stem_rule = ops.iter().any(|op| matches!(op, Paint::Rule { .. }));
        assert!(
            has_left_arrowhead && has_left_stem_rule,
            r"\overleftarrow must emit left arrowhead glyph and stem rule"
        );
    }

    #[test]
    fn test_milestone_g_content_sensitive_grids() {
        use crate::font_metrics::FontFamily;
        use crate::layout::layout_engine::Paint;
        use crate::math::math_fonts::math_font_parameters;

        let params = math_font_parameters(FontFamily::Times);
        let axis_h = (params.axis_height as f32) * (10.0 * 0.001);

        // 1. Content-Sensitive Row Metrics: Mixed-Complexity Matrix
        // Row 0: simple variable x
        // Row 1: tall fraction \frac{a + b}{c + d}
        // Row 2: simple variable z
        let mixed_matrix =
            parse_math_str(r"\begin{pmatrix} x \\ \frac{a + b}{c + d} \\ z \end{pmatrix}");
        let mut ops = Vec::new();
        let mut pool = Vec::new();
        render_math(
            &mixed_matrix,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        assert!(!ops.is_empty());

        // Extract y-positions of x (row 0), fraction rule (row 1), and z (row 2)
        let x_glyph = ops
            .iter()
            .find(|op| match op {
                Paint::GlyphRun { part_start, .. } => pool[*part_start as usize] == "x",
                _ => false,
            })
            .expect("Row 0 glyph 'x' found");
        let z_glyph = ops
            .iter()
            .find(|op| match op {
                Paint::GlyphRun { part_start, .. } => pool[*part_start as usize] == "z",
                _ => false,
            })
            .expect("Row 2 glyph 'z' found");
        let frac_rule = ops
            .iter()
            .find(|op| matches!(op, Paint::Rule { .. }))
            .expect("Row 1 fraction rule found");

        let y_row0 = match x_glyph {
            Paint::GlyphRun { y, .. } => *y,
            _ => 0.0,
        };
        let y_row2 = match z_glyph {
            Paint::GlyphRun { y, .. } => *y,
            _ => 0.0,
        };
        let y_rule = match frac_rule {
            Paint::Rule { y, .. } => *y,
            _ => 0.0,
        };

        // Row 0 is above the fraction rule, and the fraction rule is above Row 2
        assert!(
            y_row0 > y_rule,
            "Row 0 baseline ({}) must be above fraction rule ({})",
            y_row0,
            y_rule
        );
        assert!(
            y_rule > y_row2,
            "Fraction rule ({}) must be above Row 2 baseline ({})",
            y_rule,
            y_row2
        );

        // The step between Row 0 and Row 1 accommodates the fraction height
        let step_0_1 = y_row0 - y_rule;
        assert!(
            step_0_1 > 8.0,
            "Content-sensitive step ({}) must accommodate tall fraction",
            step_0_1
        );

        // 2. Delimiter Wrapping Around Content-Sensitive Grid: Strictly Axis-Centered (Rule 19)
        let mb = measure_math(&mixed_matrix, 10.0, FontFamily::Times, MathStyle::Display);
        let center_offset = (mb.h - mb.d) - 2.0 * axis_h;
        assert!(
            center_offset.abs() < 1e-2,
            "Matrix wrapped in delimiters must be symmetrically centered on math axis: offset={}",
            center_offset
        );

        // 3. Column Centering in Standard Matrices
        // Row 0 has a wide element "XXXX", Row 1 has a single character "1".
        // In standard pmatrix, "1" must be centered in its column.
        let center_mat = parse_math_str(r"\begin{matrix} XXXX \\ 1 \end{matrix}");
        ops.clear();
        pool.clear();
        render_math(
            &center_mat,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let op_xxxx = ops
            .iter()
            .find(|op| match op {
                Paint::GlyphRun { part_start, .. } => pool[*part_start as usize] == "X",
                _ => false,
            })
            .expect("XXXX found");
        let op_1 = ops
            .iter()
            .find(|op| match op {
                Paint::GlyphRun { part_start, .. } => pool[*part_start as usize] == "1",
                _ => false,
            })
            .expect("1 found");
        let x_wide = match op_xxxx {
            Paint::GlyphRun { x, .. } => *x,
            _ => 0.0,
        };
        let x_narrow = match op_1 {
            Paint::GlyphRun { x, .. } => *x,
            _ => 0.0,
        };
        assert!(
            x_narrow > x_wide,
            "Narrow item '1' must be centered in its column, so x_narrow ({}) > x_wide ({})",
            x_narrow,
            x_wide
        );

        // 4. Array Environment with Column Alignments (l, c, r)
        let array_node =
            parse_math_str(r"\begin{array}{lcr} XXX & XXX & XXX \\ 1 & 1 & 1 \end{array}");
        ops.clear();
        pool.clear();
        render_math(
            &array_node,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let ones: Vec<f32> = ops
            .iter()
            .filter_map(|op| match op {
                Paint::GlyphRun { x, part_start, .. } if pool[*part_start as usize] == "1" => {
                    Some(*x)
                }
                _ => None,
            })
            .collect();
        assert_eq!(ones.len(), 3, "Must render three '1' elements in row 1");

        let wide_xs: Vec<f32> = ops
            .iter()
            .filter_map(|op| match op {
                Paint::GlyphRun { x, part_start, .. } if pool[*part_start as usize] == "X" => {
                    Some(*x)
                }
                _ => None,
            })
            .collect();
        assert!(wide_xs.len() >= 3, "Must render 'X' elements in row 0");
        // Col 0 (left-aligned): one[0] == wide_xs[0]
        assert!(
            (ones[0] - wide_xs[0]).abs() < 1e-2,
            "Left-aligned col: '1' aligns with start of wide element"
        );
        // Col 2 (right-aligned): one[2] is shifted to the right of wide_xs[2]
        assert!(
            ones[2] > wide_xs[wide_xs.len() - 1],
            "Right-aligned col: '1' is right-aligned"
        );

        // 5. Cases Environment: Left-Aligned with Bracket
        let cases_node = parse_math_str(r"\begin{cases} 2x & x > 0 \\ -x & x \le 0 \end{cases}");
        let cases_box = measure_math(&cases_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(cases_box.w > 0.0);
        assert!(cases_box.total_h() > 15.0);
    }

    #[test]
    fn test_milestone_h_automated_geometric_differential_testing() {
        use crate::font_metrics::FontFamily;
        use crate::math::math_diff::{compare_geometry, trace_math_geometry, GeometryTolerance};

        // 1. Snapshot traces across core mathematical categories
        let formulas = [
            r"x^2 + y_1 = z",
            r"\frac{a + b}{c + d}",
            r"\sqrt{x^2 + 1}",
            r"\sum_{i=0}^n x_i",
            r"\int\limits_0^1 f(x)\,dx",
            r"\left( \frac{1}{2} \right)",
            r"\vec{v} + \widehat{A + B}",
            r"\begin{pmatrix} 1 & 0 \\ 0 & 1 \end{pmatrix}",
        ];

        for &f in &formulas {
            let node = parse_math_str(f);
            let trace = trace_math_geometry(&node, 10.0, FontFamily::Times, MathStyle::Display);
            assert!(trace.width() > 0.0);
            assert!(trace.total_height() > 0.0);

            // Self-comparison against strict tolerance must always succeed (determinism)
            let result = compare_geometry(&trace, &trace, GeometryTolerance::STRICT);
            assert!(
                result.is_ok(),
                "Self-comparison for '{}' failed: {:?}",
                f,
                result.err()
            );

            // Cross-run reproducibility (identical output on repeated evaluation)
            let trace2 = trace_math_geometry(&node, 10.0, FontFamily::Times, MathStyle::Display);
            assert_eq!(
                trace, trace2,
                "Geometric layout for '{}' must be 100% deterministic",
                f
            );
        }

        // 2. Automated Regression Detection: Exit Criterion for Milestone H
        // A geometry regression must be detected automatically before merge.
        let base_node = parse_math_str(r"\frac{\sqrt{x^2 + 1}}{2}");
        let reference =
            trace_math_geometry(&base_node, 10.0, FontFamily::Times, MathStyle::Display);

        // Simulated height regression (+0.5 pt deviation)
        let mut height_regressed = reference.clone();
        height_regressed.bounding_box.h += 0.5;
        let h_diff = compare_geometry(&height_regressed, &reference, GeometryTolerance::STRICT);
        assert!(
            h_diff.is_err(),
            "Height regression must be caught by differential testing"
        );
        let err_h = h_diff.unwrap_err();
        assert!(!err_h.passed);
        assert!(err_h
            .diagnostics
            .iter()
            .any(|d| d.contains("Height mismatch")));

        // Simulated coordinate drift regression on child glyph (+0.1 pt shift)
        let mut coord_regressed = reference.clone();
        if let Some(first_glyph) = coord_regressed.glyphs.first_mut() {
            first_glyph.x += 0.1;
        }
        let coord_diff = compare_geometry(&coord_regressed, &reference, GeometryTolerance::STRICT);
        assert!(
            coord_diff.is_err(),
            "Glyph coordinate drift must be caught by differential testing"
        );
        let err_c = coord_diff.unwrap_err();
        assert!(!err_c.passed);
        assert!(err_c
            .diagnostics
            .iter()
            .any(|d| d.contains("Glyph[0] mismatch")));

        // Simulated rule count regression
        let mut rule_regressed = reference.clone();
        rule_regressed.rules.pop();
        let rule_diff = compare_geometry(&rule_regressed, &reference, GeometryTolerance::STRICT);
        assert!(
            rule_diff.is_err(),
            "Missing rule must be caught by differential testing"
        );
        let err_r = rule_diff.unwrap_err();
        assert!(!err_r.passed);
        assert!(err_r
            .diagnostics
            .iter()
            .any(|d| d.contains("Rule count mismatch")));
    }

    #[test]
    fn test_milestone_i_compatibility_expansion() {
        use crate::font_metrics::FontFamily;
        use crate::layout::layout_engine::Paint;

        // 1. Binomial variants lowering to Delimited + Fraction without rules
        let binom_node = parse_math_str(r"\binom{n}{k}");
        let mut ops = Vec::new();
        let mut pool = Vec::new();
        render_math(
            &binom_node,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let rule_count = ops
            .iter()
            .filter(|op| matches!(op, Paint::Rule { .. }))
            .count();
        assert_eq!(
            rule_count, 0,
            r"\binom must lower to fraction with has_rule: false (0 rules)"
        );
        assert!(pool.contains(&"("));
        assert!(pool.contains(&")"));

        let dbinom_node = parse_math_str(r"\dbinom{n}{k}");
        let tbinom_node = parse_math_str(r"\tbinom{n}{k}");
        let d_box = measure_math(&dbinom_node, 10.0, FontFamily::Times, MathStyle::Text);
        let t_box = measure_math(&tbinom_node, 10.0, FontFamily::Times, MathStyle::Text);
        assert!(
            d_box.total_h() > t_box.total_h(),
            r"\dbinom forces display style, must be taller than \tbinom"
        );

        // 2. Continued fraction \cfrac and universal \genfrac
        let cfrac_node = parse_math_str(r"\cfrac{1}{1 + \cfrac{1}{2}}");
        let c_box = measure_math(&cfrac_node, 10.0, FontFamily::Times, MathStyle::Text);
        assert!(
            c_box.total_h() > 25.0,
            r"\cfrac preserves display style across nested levels"
        );

        let genfrac_binom = parse_math_str(r"\genfrac{(}{)}{0pt}{0}{n}{k}");
        ops.clear();
        pool.clear();
        render_math(
            &genfrac_binom,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        let gf_rules = ops
            .iter()
            .filter(|op| matches!(op, Paint::Rule { .. }))
            .count();
        assert_eq!(gf_rules, 0, r"\genfrac with 0pt thickness emits 0 rules");

        // 3. AMS Multi-equation environments lowering into Matrix
        let align_node = parse_math_str(r"\begin{align*} x &= 1 \\ y &= 2 + 3 \end{align*}");
        let align_box = measure_math(&align_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(align_box.w > 0.0);
        assert!(align_box.total_h() > 15.0);

        let gather_node = parse_math_str(r"\begin{gather*} x + y = z \\ a = b \end{gather*}");
        let gather_box = measure_math(&gather_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(gather_box.w > 0.0);

        let multline_node = parse_math_str(r"\begin{multline*} a + b \\ + c \end{multline*}");
        let multline_box =
            measure_math(&multline_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(multline_box.w > 0.0);

        // 4. Modular arithmetic \bmod and \pmod
        let bmod_node = parse_math_str(r"a \bmod b");
        ops.clear();
        pool.clear();
        render_math(
            &bmod_node,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        assert!(pool.contains(&"m") && pool.contains(&"o") && pool.contains(&"d"));

        let pmod_node = parse_math_str(r"x \equiv y \pmod{m}");
        ops.clear();
        pool.clear();
        render_math(
            &pmod_node,
            10.0,
            FontFamily::Times,
            MathStyle::Display,
            0.0,
            0.0,
            &mut ops,
            &mut pool,
        );
        assert!(pool.contains(&"(") && pool.contains(&")"));

        // 5. Math font switches
        let fonts_node = parse_math_str(r"\mathbf{A} + \mathtt{x} + \mathcal{F} + \mathbb{R}");
        let fonts_box = measure_math(&fonts_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(fonts_box.w > 0.0);

        // 6. Extended spacing
        let space_node =
            parse_math_str(r"a \enspace b \quad c \qquad d \thinspace e \negthinspace f");
        let space_box = measure_math(&space_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(space_box.w > 40.0);
    }

    #[test]
    fn test_milestone_j_performance_consolidation_and_benchmarks() {
        use crate::math::math_ast::{ColAlign, DelimitedData};
        use crate::math::math_fonts::{
            AccentMetrics, BigOpMetrics, DelimiterMetrics, HBarMetrics, MatrixMetrics,
            RadicalMetrics,
        };
        use std::time::Instant;

        // 1. Data structure packing invariants
        assert_eq!(std::mem::size_of::<MathNode>(), 32);
        assert_eq!(std::mem::size_of::<MathBox>(), 16);
        assert_eq!(std::mem::size_of::<MathStyle>(), 1);
        assert_eq!(std::mem::size_of::<MathAtomKind>(), 1);
        assert_eq!(std::mem::size_of::<MatrixDelim>(), 1);
        assert_eq!(std::mem::size_of::<ColAlign>(), 1);
        assert_eq!(std::mem::size_of::<DelimitedData>(), 64);
        assert_eq!(std::mem::size_of::<RadicalMetrics>(), 16);
        assert_eq!(std::mem::size_of::<DelimiterMetrics>(), 16);
        assert_eq!(std::mem::size_of::<BigOpMetrics>(), 16);
        assert_eq!(std::mem::size_of::<AccentMetrics>(), 32);
        assert_eq!(std::mem::size_of::<MatrixMetrics>(), 16);
        assert_eq!(std::mem::size_of::<HBarMetrics>(), 16);

        // 2. Parser zero-allocation single-node unwrapping verification
        let single_var = parse_math_str("x");
        assert_eq!(single_var, MathNode::Variable('x'));

        let grouped_var = parse_math_str("{x}");
        assert_eq!(grouped_var, MathNode::Variable('x'));

        let frac = parse_math_str(r"\frac{a}{b}");
        if let MathNode::Fraction {
            num, den, has_rule, ..
        } = frac
        {
            assert!(has_rule);
            assert_eq!(*num, MathNode::Variable('a'));
            assert_eq!(*den, MathNode::Variable('b'));
        } else {
            panic!("Expected Fraction");
        }

        // 3. Representative math corpus for performance benchmarking
        let specimens = [
            // Algebra & Polynomials
            r"x^2 + 2xy + y^2 = (x + y)^2",
            // Continued Fraction
            r"\frac{1}{1 + \frac{1}{1 + \frac{1}{x}}}",
            // Calculus Integral with Limits
            r"\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}",
            // Matrix Grid
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            // Piecewise cases
            r"\begin{cases} x & x > 0 \\ -x & x \le 0 \end{cases}",
            // Multi-line aligned equations
            r"\begin{align*} a &= b + c \\ d &= e - f \end{align*}",
        ];

        let num_specimens = specimens.len();
        let iters = 5_000;

        // Microbenchmark 1: Parse throughput
        let t0 = Instant::now();
        let mut parsed_nodes = Vec::with_capacity(num_specimens);
        for _ in 0..iters {
            for eq in &specimens {
                let node = parse_math_str(eq);
                if parsed_nodes.len() < num_specimens {
                    parsed_nodes.push(node);
                }
            }
        }
        let parse_dur = t0.elapsed();
        let total_parses = iters * num_specimens;
        let ns_per_parse = (parse_dur.as_nanos() as f64) / (total_parses as f64);
        let parses_per_sec = (total_parses as f64) / parse_dur.as_secs_f64();

        // Microbenchmark 2: Layout measurement throughput
        let t1 = Instant::now();
        let mut dummy_w = 0.0f32;
        let measure_iters = 10_000;
        for _ in 0..measure_iters {
            for node in &parsed_nodes {
                let mb = measure_math(node, 10.0, FontFamily::Times, MathStyle::Display);
                dummy_w += mb.w;
            }
        }
        let measure_dur = t1.elapsed();
        let total_measures = measure_iters * num_specimens;
        let ns_per_measure = (measure_dur.as_nanos() as f64) / (total_measures as f64);
        let measures_per_sec = (total_measures as f64) / measure_dur.as_secs_f64();

        // Microbenchmark 3: Layout render throughput (with reused buffers)
        let t2 = Instant::now();
        let render_iters = 10_000;
        let mut ops = Vec::with_capacity(64);
        let mut pool = Vec::with_capacity(64);
        for _ in 0..render_iters {
            for node in &parsed_nodes {
                ops.clear();
                pool.clear();
                let rb = render_math(
                    node,
                    10.0,
                    FontFamily::Times,
                    MathStyle::Display,
                    0.0,
                    0.0,
                    &mut ops,
                    &mut pool,
                );
                dummy_w += rb.w;
            }
        }
        let render_dur = t2.elapsed();
        let total_renders = render_iters * num_specimens;
        let ns_per_render = (render_dur.as_nanos() as f64) / (total_renders as f64);
        let renders_per_sec = (total_renders as f64) / render_dur.as_secs_f64();

        println!("\n=== MATHEMATICAL ENGINE PERFORMANCE CONSOLIDATION (MILESTONE J) ===");
        println!(
            "  Parse (AST) throughput:        {:>7.2} ns/eq  ({:>9.0} eq/sec)",
            ns_per_parse, parses_per_sec
        );
        println!(
            "  Layout measurement throughput: {:>7.2} ns/eq  ({:>9.0} eq/sec)",
            ns_per_measure, measures_per_sec
        );
        println!(
            "  Layout render throughput:      {:>7.2} ns/eq  ({:>9.0} eq/sec)",
            ns_per_render, renders_per_sec
        );
        assert!(dummy_w > 0.0);
    }

    #[test]
    fn test_appendix_g_comprehensive_22_rules() {
        // Rule 1: Null list
        let empty_node = MathNode::Row(vec![]);
        let empty_box = measure_math(&empty_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert_eq!(empty_box.w, 0.0);
        assert_eq!(empty_box.h, 0.0);
        assert_eq!(empty_box.d, 0.0);

        // Rule 2: Current style (all 8 styles)
        for s in [
            MathStyle::Display,
            MathStyle::CrampedDisplay,
            MathStyle::Text,
            MathStyle::CrampedText,
            MathStyle::Script,
            MathStyle::CrampedScript,
            MathStyle::ScriptScript,
            MathStyle::CrampedScriptScript,
        ] {
            assert!(s.cramped().is_cramped());
        }

        // Rule 3: Character conversion from fonts
        let char_node = parse_math_str("x");
        let char_box = measure_math(&char_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(char_box.w > 0.0 && char_box.h > 0.0);

        // Rule 4: Accents with horizontal skew/ic offset
        let acc_node = parse_math_str(r"\hat{f}");
        let acc_box = measure_math(&acc_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(acc_box.h > char_box.h);

        // Rule 5: Delimiters and \left \right
        let delim_node = parse_math_str(r"\left( \frac{a}{b} \right)");
        let delim_box = measure_math(&delim_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(delim_box.h > 10.0);

        // Rule 6: Subformulas / groups
        let grp_node = parse_math_str("{x + y}");
        let grp_box = measure_math(&grp_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(grp_box.w > char_box.w);

        // Rule 7: Big operators with display limits and \nolimits
        let sum_node = parse_math_str(r"\sum_{i=0}^n i");
        let sum_box = measure_math(&sum_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(sum_box.h > 10.0);

        // Rule 8, 9, 10: Fractions and Atop
        let frac_node = parse_math_str(r"\frac{a}{b}");
        let frac_box = measure_math(&frac_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(frac_box.h > 0.0 && frac_box.d > 0.0);

        let binom_node = parse_math_str(r"\binom{n}{k}");
        let binom_box = measure_math(&binom_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(binom_box.h > 0.0 && binom_box.d > 0.0);

        // Rule 11: Radicals
        let rad_node = parse_math_str(r"\sqrt{x}");
        let rad_box = measure_math(&rad_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(rad_box.h > 0.0 && rad_box.d > 0.0);

        // Rule 12: Vcenter (\vcenter centers on math axis sigma_22)
        let vc_node = parse_math_str(r"\vcenter{A}");
        let vc_box = measure_math(&vc_node, 10.0, FontFamily::Times, MathStyle::Display);
        let axis_h = 2.5; // 250 in 10pt
        let midpoint = 0.5 * (vc_box.h - vc_box.d);
        assert!(
            (midpoint - axis_h).abs() < 1e-3,
            "Vcenter midpoint ({}) must equal axis_height ({})",
            midpoint,
            axis_h
        );

        // Rule 13: Overline (cramped style, extra height)
        let over_node = parse_math_str(r"\overline{x}");
        let over_box = measure_math(&over_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(over_box.h > char_box.h);

        // Rule 14: Underline (depth increased)
        let under_node = parse_math_str(r"\underline{x}");
        let under_box = measure_math(&under_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(under_box.d > char_box.d);

        // Rule 15: Text / Hbox (treated as Ord)
        let text_node = parse_math_str(r"\text{hello}");
        let text_box = measure_math(&text_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(text_box.w > 0.0);

        // Rule 16: Atom spacing and Bin->Ord normalization
        let unary_node = parse_math_str("-x");
        let unary_box = measure_math(&unary_node, 10.0, FontFamily::Times, MathStyle::Display);
        let minus_only = measure_math(
            &parse_math_str("-"),
            10.0,
            FontFamily::Times,
            MathStyle::Display,
        );
        assert!((unary_box.w - (minus_only.w + char_box.w)).abs() < 1e-3);

        // Rule 17: Italic correction before superscripts
        let f_sup = parse_math_str("f^2");
        let f_sup_box = measure_math(&f_sup, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(f_sup_box.w > 0.0);

        // Rule 18: Superscripts and subscripts (18a..18e)
        let subsup_node = parse_math_str("x_1^2");
        let subsup_box = measure_math(&subsup_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(subsup_box.h > char_box.h);
        assert!(subsup_box.d > char_box.d);

        // Rule 19: Delimiter sizing formula
        let tall_delim = parse_math_str(r"\left( \int_0^1 x dx \right)");
        let tall_delim_box = measure_math(&tall_delim, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(tall_delim_box.h > 12.0);

        // Rule 20: Matrices
        let mat_node = parse_math_str(r"\begin{matrix} 1 & 2 \\ 3 & 4 \end{matrix}");
        let mat_box = measure_math(&mat_node, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(mat_box.h > 10.0);

        // Rule 21 & 22: Assembly and line penalties
        let assembled = parse_math_str(r"a + b = c");
        let assembled_box = measure_math(&assembled, 10.0, FontFamily::Times, MathStyle::Display);
        assert!(assembled_box.w > 0.0);
    }

    #[test]
    fn test_batch_1_style_state_machine_and_scoping() {
        use crate::math::math_ast::{
            CrampedStyleGovernor, FontVariantTier, MathContextFlags, MathStyle, StyleFrameStack,
            StyleHierarchy, StyleScaleLut, StyleTransitionLut, STYLE_SCALE_LUT,
            STYLE_TRANSITION_LUT,
        };

        // 1. StyleHierarchy Bit-Packing invariants
        assert_eq!(std::mem::size_of::<StyleHierarchy>(), 1);
        assert_eq!(std::mem::size_of::<MathStyle>(), 1);
        for s in 0u8..8 {
            let style = MathStyle::from_u8(s).expect("Valid style value");
            assert_eq!(style.as_u8(), s);
            assert_eq!(MathStyle::from_u8_clamped(s | 0b1000_0000), style);
        }
        assert_eq!(MathStyle::from_u8(8), None);

        // Tier bit extraction: 0 = Display, 1 = Text, 2 = Script, 3 = ScriptScript
        assert_eq!(MathStyle::Display.tier(), 0);
        assert_eq!(MathStyle::CrampedDisplay.tier(), 0);
        assert_eq!(MathStyle::Text.tier(), 1);
        assert_eq!(MathStyle::CrampedText.tier(), 1);
        assert_eq!(MathStyle::Script.tier(), 2);
        assert_eq!(MathStyle::CrampedScript.tier(), 2);
        assert_eq!(MathStyle::ScriptScript.tier(), 3);
        assert_eq!(MathStyle::CrampedScriptScript.tier(), 3);

        // 2. StyleFrameStack cacheline & scoping invariants
        assert_eq!(std::mem::size_of::<StyleFrameStack>(), 32);
        assert_eq!(std::mem::align_of::<StyleFrameStack>(), 32);

        let mut stack = StyleFrameStack::new(MathStyle::Display);
        assert_eq!(stack.current(), MathStyle::Display);
        assert_eq!(stack.depth(), 1);
        assert!(stack.is_root());

        stack.push(MathStyle::Text);
        assert_eq!(stack.current(), MathStyle::Text);
        assert_eq!(stack.depth(), 2);
        assert!(!stack.is_root());

        stack.push(MathStyle::Script);
        assert_eq!(stack.current(), MathStyle::Script);
        assert_eq!(stack.depth(), 3);

        assert_eq!(stack.pop(), MathStyle::Script);
        assert_eq!(stack.current(), MathStyle::Text);
        assert_eq!(stack.depth(), 2);

        assert_eq!(stack.pop(), MathStyle::Text);
        assert_eq!(stack.current(), MathStyle::Display);
        assert_eq!(stack.depth(), 1);
        assert!(stack.is_root());

        // Saturating pop does not underflow
        assert_eq!(stack.pop(), MathStyle::Display);
        assert_eq!(stack.current(), MathStyle::Display);
        assert_eq!(stack.depth(), 1);

        // Saturation at max capacity (31 frames)
        for _ in 0..40 {
            stack.push(MathStyle::ScriptScript);
        }
        assert_eq!(stack.depth(), 31);
        assert_eq!(stack.current(), MathStyle::ScriptScript);

        // 3. CrampedStyleGovernor branchless transitions
        assert!(!CrampedStyleGovernor::is_cramped(MathStyle::Display));
        assert!(CrampedStyleGovernor::is_cramped(MathStyle::CrampedDisplay));
        assert_eq!(
            CrampedStyleGovernor::to_cramped(MathStyle::Display),
            MathStyle::CrampedDisplay
        );
        assert_eq!(
            CrampedStyleGovernor::to_cramped(MathStyle::CrampedDisplay),
            MathStyle::CrampedDisplay
        );
        assert_eq!(
            CrampedStyleGovernor::to_uncramped(MathStyle::CrampedDisplay),
            MathStyle::Display
        );
        assert_eq!(
            CrampedStyleGovernor::to_uncramped(MathStyle::Display),
            MathStyle::Display
        );
        assert_eq!(
            CrampedStyleGovernor::to_cramped(MathStyle::Text),
            MathStyle::CrampedText
        );
        assert_eq!(
            CrampedStyleGovernor::to_uncramped(MathStyle::CrampedText),
            MathStyle::Text
        );
        assert_eq!(
            CrampedStyleGovernor::to_cramped(MathStyle::Script),
            MathStyle::CrampedScript
        );
        assert_eq!(
            CrampedStyleGovernor::to_cramped(MathStyle::ScriptScript),
            MathStyle::CrampedScriptScript
        );

        // 4. StyleScaleLut and StyleTransitionLut cacheline alignment
        assert_eq!(std::mem::size_of::<StyleScaleLut>(), 64);
        assert_eq!(std::mem::align_of::<StyleScaleLut>(), 64);
        assert_eq!(std::mem::size_of::<StyleTransitionLut>(), 64);
        assert_eq!(std::mem::align_of::<StyleTransitionLut>(), 64);

        assert_eq!(STYLE_SCALE_LUT.scales[0], 1.0);
        assert_eq!(STYLE_SCALE_LUT.scales[1], 1.0);
        assert_eq!(STYLE_SCALE_LUT.scales[2], 1.0);
        assert_eq!(STYLE_SCALE_LUT.scales[3], 1.0);
        assert_eq!(STYLE_SCALE_LUT.scales[4], 0.70);
        assert_eq!(STYLE_SCALE_LUT.scales[5], 0.70);
        assert_eq!(STYLE_SCALE_LUT.scales[6], 0.50);
        assert_eq!(STYLE_SCALE_LUT.scales[7], 0.50);

        for s in 0u8..8 {
            let style = MathStyle::from_u8(s).unwrap();
            assert_eq!(style.scale(), STYLE_SCALE_LUT.scales[s as usize]);
            assert_eq!(style.sup_style(), STYLE_TRANSITION_LUT.sup[s as usize]);
            assert_eq!(style.sub_style(), STYLE_TRANSITION_LUT.sub[s as usize]);
            assert_eq!(
                style.frac_num_style(),
                STYLE_TRANSITION_LUT.frac_num[s as usize]
            );
            assert_eq!(
                style.frac_den_style(),
                STYLE_TRANSITION_LUT.frac_den[s as usize]
            );
            assert_eq!(
                style.matrix_cell_style(),
                STYLE_TRANSITION_LUT.matrix_cell[s as usize]
            );
        }

        // 5. MathContextFlags 1-byte bit-packing
        assert_eq!(std::mem::size_of::<MathContextFlags>(), 1);
        let mut flags = MathContextFlags::empty();
        assert!(!flags.contains(MathContextFlags::IN_DISPLAY));
        flags.set(MathContextFlags::IN_DISPLAY);
        assert!(flags.contains(MathContextFlags::IN_DISPLAY));
        flags.set(MathContextFlags::IN_RADICAL);
        assert!(flags.contains(MathContextFlags::IN_DISPLAY));
        assert!(flags.contains(MathContextFlags::IN_RADICAL));
        flags.unset(MathContextFlags::IN_DISPLAY);
        assert!(!flags.contains(MathContextFlags::IN_DISPLAY));
        assert!(flags.contains(MathContextFlags::IN_RADICAL));

        let with_flags = flags.with(MathContextFlags::IN_FRACTION_NUMERATOR);
        assert!(with_flags.contains(MathContextFlags::IN_FRACTION_NUMERATOR));
        let without_flags = with_flags.without(MathContextFlags::IN_RADICAL);
        assert!(!without_flags.contains(MathContextFlags::IN_RADICAL));
        assert!(without_flags.contains(MathContextFlags::IN_FRACTION_NUMERATOR));

        // 6. FontVariantTier 32-byte cache
        assert_eq!(std::mem::size_of::<FontVariantTier>(), 32);
        assert_eq!(std::mem::align_of::<FontVariantTier>(), 32);
        let tier = FontVariantTier::new(10.0);
        assert_eq!(tier.font_size(MathStyle::Display), 10.0);
        assert_eq!(tier.font_size(MathStyle::CrampedDisplay), 10.0);
        assert_eq!(tier.font_size(MathStyle::Text), 10.0);
        assert_eq!(tier.font_size(MathStyle::CrampedText), 10.0);
        assert_eq!(tier.font_size(MathStyle::Script), 7.0);
        assert_eq!(tier.font_size(MathStyle::CrampedScript), 7.0);
        assert_eq!(tier.font_size(MathStyle::ScriptScript), 5.0);
        assert_eq!(tier.font_size(MathStyle::CrampedScriptScript), 5.0);
    }

    #[test]
    fn test_batch_2_atom_layout_acceleration() {
        use crate::backend::fonts::Face;
        use crate::math::math_spacing::{
            AsciiMathFastPath, AtomBoundaryClassifier, AtomLimits, KernPairLut, MathAtomKind,
            MathHorizontalListSoA, PackedAtomMetadata, TransientAtomView, ASCII_MATH_FAST_PATH,
            KERN_PAIR_LUT,
        };

        // 7. PackedAtomMetadata 1-byte invariants
        assert_eq!(std::mem::size_of::<PackedAtomMetadata>(), 1);
        let mut meta = PackedAtomMetadata::new(MathAtomKind::Op);
        assert_eq!(meta.kind(), MathAtomKind::Op);
        assert_eq!(meta.limits(), AtomLimits::Default);
        assert!(!meta.has_accent());
        assert!(!meta.is_boundary_converted());

        meta = meta.with_limits(AtomLimits::DisplayLimits);
        assert_eq!(meta.limits(), AtomLimits::DisplayLimits);
        assert_eq!(meta.kind(), MathAtomKind::Op);

        meta.set_accent(true);
        assert!(meta.has_accent());
        meta.set_boundary_converted(true);
        assert!(meta.is_boundary_converted());
        assert_eq!(meta.kind(), MathAtomKind::Op);
        assert_eq!(meta.limits(), AtomLimits::DisplayLimits);

        // 8. KernPairLut 64-byte alignment and values
        assert_eq!(std::mem::size_of::<KernPairLut>(), 64);
        assert_eq!(std::mem::align_of::<KernPairLut>(), 64);
        assert_eq!(KERN_PAIR_LUT.italic_correction('f'), 120);
        assert_eq!(KERN_PAIR_LUT.italic_correction('d'), 40);
        assert_eq!(KERN_PAIR_LUT.italic_correction('j'), 50);
        assert_eq!(KERN_PAIR_LUT.italic_correction('a'), 20);
        assert_eq!(KERN_PAIR_LUT.italic_correction('V'), 90);
        assert_eq!(KERN_PAIR_LUT.italic_correction('A'), 40);
        assert_eq!(KERN_PAIR_LUT.italic_correction('1'), 0);
        assert_eq!(KERN_PAIR_LUT.pair_kern('f', ')'), 40);
        assert_eq!(KERN_PAIR_LUT.pair_kern('f', ';'), -30);
        assert_eq!(KERN_PAIR_LUT.pair_kern('P', '_'), -50);
        assert_eq!(KERN_PAIR_LUT.pair_kern('d', 'x'), 30);

        // 9. TransientAtomView 16-byte cacheline packing
        assert_eq!(std::mem::size_of::<TransientAtomView>(), 16);
        let view = TransientAtomView::new(5.0, 8.0, 2.0, MathAtomKind::Bin);
        assert_eq!(view.w, 5.0);
        assert_eq!(view.h, 8.0);
        assert_eq!(view.d, 2.0);
        assert_eq!(view.kind, MathAtomKind::Bin);
        let mb = view.to_box();
        assert_eq!(mb.w, 5.0);
        assert_eq!(mb.h, 8.0);
        assert_eq!(mb.d, 2.0);
        let view2 = TransientAtomView::from_box(mb, MathAtomKind::Rel);
        assert_eq!(view2.kind, MathAtomKind::Rel);

        // 10. AtomBoundaryClassifier branchless bitmask classification
        assert!(AtomBoundaryClassifier::is_valid_left(MathAtomKind::Ord));
        assert!(!AtomBoundaryClassifier::is_valid_left(MathAtomKind::Op)); // tex.web line 14331: Op converts following Bin to Ord
        assert!(AtomBoundaryClassifier::is_valid_left(MathAtomKind::Close));
        assert!(AtomBoundaryClassifier::is_valid_left(MathAtomKind::Inner));
        assert!(!AtomBoundaryClassifier::is_valid_left(MathAtomKind::Bin));
        assert!(!AtomBoundaryClassifier::is_valid_left(MathAtomKind::Rel));
        assert!(!AtomBoundaryClassifier::is_valid_left(MathAtomKind::Open));
        assert!(!AtomBoundaryClassifier::is_valid_left(MathAtomKind::Punct));

        assert!(AtomBoundaryClassifier::is_valid_right(MathAtomKind::Ord));
        assert!(AtomBoundaryClassifier::is_valid_right(MathAtomKind::Op));
        assert!(AtomBoundaryClassifier::is_valid_right(MathAtomKind::Open));
        assert!(AtomBoundaryClassifier::is_valid_right(MathAtomKind::Inner));
        assert!(!AtomBoundaryClassifier::is_valid_right(MathAtomKind::Bin));
        assert!(!AtomBoundaryClassifier::is_valid_right(MathAtomKind::Rel));
        assert!(!AtomBoundaryClassifier::is_valid_right(MathAtomKind::Close));
        assert!(!AtomBoundaryClassifier::is_valid_right(MathAtomKind::Punct));

        // Binary converted to Ordinary at edges
        assert_eq!(
            AtomBoundaryClassifier::classify(MathAtomKind::Bin, None, Some(MathAtomKind::Ord)),
            MathAtomKind::Ord
        );
        assert_eq!(
            AtomBoundaryClassifier::classify(MathAtomKind::Bin, Some(MathAtomKind::Ord), None),
            MathAtomKind::Ord
        );
        assert_eq!(
            AtomBoundaryClassifier::classify(
                MathAtomKind::Bin,
                Some(MathAtomKind::Ord),
                Some(MathAtomKind::Ord)
            ),
            MathAtomKind::Bin
        );
        assert_eq!(
            AtomBoundaryClassifier::classify(
                MathAtomKind::Bin,
                Some(MathAtomKind::Rel),
                Some(MathAtomKind::Ord)
            ),
            MathAtomKind::Ord
        );

        // 11. MathHorizontalListSoA SIMD-friendly layout
        let mut soa = MathHorizontalListSoA::with_capacity(4);
        assert!(soa.is_empty());
        soa.push(5.0, 10.0, 2.0, MathAtomKind::Ord);
        soa.push(6.0, 8.0, 1.0, MathAtomKind::Bin);
        soa.push(7.0, 12.0, 3.0, MathAtomKind::Ord);
        soa.kerns.push(1.5);
        soa.kerns.push(2.0);

        assert_eq!(soa.len(), 3);
        assert_eq!(soa.total_width(), 5.0 + 6.0 + 7.0 + 1.5 + 2.0);
        assert_eq!(soa.max_height(), 12.0);
        assert_eq!(soa.max_depth(), 3.0);

        soa.clear();
        assert!(soa.is_empty());
        assert_eq!(soa.total_width(), 0.0);

        // 12. AsciiMathFastPath 128-byte L1 resident table
        assert_eq!(std::mem::size_of::<AsciiMathFastPath>(), 128);
        assert_eq!(std::mem::align_of::<AsciiMathFastPath>(), 64);

        assert_eq!(
            ASCII_MATH_FAST_PATH.lookup(b'x'),
            (MathAtomKind::Ord, Face::Italic, true)
        );
        assert_eq!(
            ASCII_MATH_FAST_PATH.lookup(b'A'),
            (MathAtomKind::Ord, Face::Italic, true)
        );
        assert_eq!(
            ASCII_MATH_FAST_PATH.lookup(b'7'),
            (MathAtomKind::Ord, Face::Roman, true)
        );
        assert_eq!(
            ASCII_MATH_FAST_PATH.lookup(b'+'),
            (MathAtomKind::Bin, Face::Roman, true)
        );
        assert_eq!(
            ASCII_MATH_FAST_PATH.lookup(b'='),
            (MathAtomKind::Rel, Face::Roman, true)
        );
        assert_eq!(
            ASCII_MATH_FAST_PATH.lookup(b'('),
            (MathAtomKind::Open, Face::Roman, true)
        );
        assert_eq!(
            ASCII_MATH_FAST_PATH.lookup(b')'),
            (MathAtomKind::Close, Face::Roman, true)
        );
        assert_eq!(
            ASCII_MATH_FAST_PATH.lookup(b','),
            (MathAtomKind::Punct, Face::Roman, true)
        );
        assert!(!ASCII_MATH_FAST_PATH.lookup(b' ').2);
    }

    #[test]
    fn test_batches_3_to_9_all_50_optimizations() {
        use crate::backend::fonts::Face;
        use crate::math::math_assembly::{
            ComprehensiveMathVerifier, FormulaBoundingBoxUnion, MathGeometricTraceBuffer,
            MathGeometricTraceRecord, MathPaintEmitter,
        };
        use crate::math::math_ast::{MathStyle, MatrixDelim};
        use crate::math::math_bigop::{
            BigOpVariantTier, IntegralOverhangCorrector, LimitClearanceGovernor,
            OperatorCenteringEngine, OperatorLimitSwitch, PackedBigOpMetrics, BIG_OP_VARIANT_TIER,
        };
        use crate::math::math_box::MathBox;
        use crate::math::math_delim::{
            DelimiterPairMatch, DelimiterSizingGovernor, DelimiterVariantResolver,
            ExtensibleAssemblyEngine, NullDelimiterEliminator, DELIMITER_VARIANT_RESOLVER,
        };
        use crate::math::math_fraction::{
            BinomialDelimiterCache, ContinuedFractionPipeline, FractionClearanceGovernor,
            GeneralizedFractionEngine, NumeratorDenominatorAligner, PackedFractionMetrics,
            BINOMIAL_DELIMITER_CACHE,
        };
        use crate::math::math_matrix::{
            MatrixColumnSoA, MatrixDelimBinder, RowBaselineAligner, SparseGridCompressor,
            VcenterGeometryEngine,
        };
        use crate::math::math_radical::{
            ExtensibleRadicalAssembler, PackedRadicalMetrics, RadicalClearanceGovernor,
            RadicalExcessDistributor, RadicalVariantSelector, RootDegreePositioner,
            RADICAL_VARIANT_SELECTOR,
        };
        use crate::math::math_scripts::{
            KernBeforeScriptLut, PackedScriptMetrics, ScriptClearanceGovernor,
            ScriptNestingLimiter, SubSupShiftOptimizer, TransientScriptPair,
            KERN_BEFORE_SCRIPT_LUT,
        };
        use crate::math::math_spacing::AtomLimits;

        // BATCH 3: Fractions & Binomials (Items 13–18)
        assert_eq!(std::mem::size_of::<PackedFractionMetrics>(), 16);
        let frac_metrics = PackedFractionMetrics::new(8.0, 6.0, 0.4, 2.0);
        assert_eq!(frac_metrics.num_shift, 8.0);
        assert_eq!(frac_metrics.den_shift, 6.0);

        // Rule 10 clearances
        assert_eq!(
            FractionClearanceGovernor::min_clearance_above(0.4, true),
            1.2
        );
        assert_eq!(
            FractionClearanceGovernor::min_clearance_above(0.4, false),
            0.4
        );
        assert_eq!(
            FractionClearanceGovernor::min_clearance_atop(0.4, true),
            2.8
        );
        assert_eq!(
            FractionClearanceGovernor::min_clearance_atop(0.4, false),
            1.2
        );

        let (tw, nx, dx) = NumeratorDenominatorAligner::align_centered(10.0, 20.0);
        assert_eq!(tw, 20.0);
        assert_eq!(nx, 5.0);
        assert_eq!(dx, 0.0);

        assert_eq!(std::mem::size_of::<BinomialDelimiterCache>(), 64);
        assert_eq!(std::mem::align_of::<BinomialDelimiterCache>(), 64);
        assert_eq!(BINOMIAL_DELIMITER_CACHE.resolve_glyph(8.0), 0x28);
        assert_eq!(BINOMIAL_DELIMITER_CACHE.resolve_glyph(14.0), 0x70);
        assert_eq!(BINOMIAL_DELIMITER_CACHE.resolve_glyph(50.0), 0xF3);

        let g_box = GeneralizedFractionEngine::compute_box(
            MathBox::with_dims(10.0, 7.0, 2.0),
            MathBox::with_dims(12.0, 6.0, 3.0),
            8.0,
            6.0,
            0.4,
            true,
        );
        assert_eq!(g_box.w, 12.0);
        assert_eq!(g_box.h, 15.0);
        assert_eq!(g_box.d, 9.0);

        assert_eq!(ContinuedFractionPipeline::cfrac_style(), MathStyle::Display);

        // BATCH 4: Radicals & Roots (Items 19–24)
        assert_eq!(std::mem::size_of::<PackedRadicalMetrics>(), 16);
        let rad_metrics = PackedRadicalMetrics::new(0.4, 1.1, 0.5, 3.0);
        assert_eq!(rad_metrics.rule_thickness, 0.4);

        assert_eq!(RadicalClearanceGovernor::min_clearance(0.4, 0.4, true), 0.5);
        assert_eq!(
            RadicalClearanceGovernor::min_clearance(0.4, 0.4, false),
            0.5
        );

        assert_eq!(std::mem::size_of::<RadicalVariantSelector>(), 64);
        assert_eq!(std::mem::align_of::<RadicalVariantSelector>(), 64);
        let (v_code, ext) = RADICAL_VARIANT_SELECTOR.select_variant(8.0);
        assert_eq!(v_code, 0x70);
        assert!(!ext);
        let (v_code_ext, ext2) = RADICAL_VARIANT_SELECTOR.select_variant(35.0);
        assert_eq!(v_code_ext, 0x74);
        assert!(ext2);

        let (ex_top, ex_bot) = RadicalExcessDistributor::distribute_excess(12.0, 10.0);
        assert_eq!(ex_top, 1.0);
        assert_eq!(ex_bot, 1.0);

        let (deg_x, deg_y) =
            RootDegreePositioner::degree_offset(10.0, 12.0, &MathBox::with_dims(3.0, 4.0, 1.0));
        assert!(deg_x > 0.0);
        assert!(deg_y > 0.0);

        let rad_box = ExtensibleRadicalAssembler::compute_box(
            &MathBox::with_dims(10.0, 8.0, 2.0),
            6.0,
            0.4,
            1.5,
        );
        assert!(rad_box.w > 16.0);
        assert_eq!(rad_box.h, 8.0 + 1.5 + 0.4);
        assert_eq!(rad_box.d, 2.0);

        // BATCH 5: Scripts (Items 25–30)
        assert_eq!(std::mem::size_of::<PackedScriptMetrics>(), 16);
        let (adj_u, adj_v) =
            ScriptClearanceGovernor::enforce_combined_clearance(5.0, 2.0, 1.0, 4.0, 0.5, 10.0);
        assert!(adj_u >= 5.0);
        assert!(adj_v >= 2.0);

        let init_u =
            SubSupShiftOptimizer::initial_sup_shift(MathStyle::Display, 10.0, 1.0, 4.0, 3.0, 2.0);
        assert!(init_u >= 4.0);
        let init_v = SubSupShiftOptimizer::initial_sub_shift(2.0, 3.0, 2.0, 1.5);
        assert!(init_v >= 2.0);

        assert_eq!(std::mem::size_of::<KernBeforeScriptLut>(), 64);
        assert_eq!(std::mem::align_of::<KernBeforeScriptLut>(), 64);
        assert_eq!(KERN_BEFORE_SCRIPT_LUT.sub_kern('P'), -50);
        assert_eq!(KERN_BEFORE_SCRIPT_LUT.sub_kern('T'), -60);
        assert_eq!(KERN_BEFORE_SCRIPT_LUT.sub_kern('A'), -30);

        assert_eq!(
            ScriptNestingLimiter::next_script_style(MathStyle::Display),
            MathStyle::Script
        );
        assert_eq!(std::mem::size_of::<TransientScriptPair>(), 32);
        assert_eq!(std::mem::align_of::<TransientScriptPair>(), 32);

        // BATCH 6: Big Operators & Limits (Items 31–36)
        assert_eq!(std::mem::size_of::<PackedBigOpMetrics>(), 16);
        assert!(OperatorLimitSwitch::should_render_limits(
            false,
            AtomLimits::Default,
            MathStyle::Display
        ));
        assert!(!OperatorLimitSwitch::should_render_limits(
            true,
            AtomLimits::Default,
            MathStyle::Display
        ));
        assert!(OperatorLimitSwitch::should_render_limits(
            true,
            AtomLimits::Limits,
            MathStyle::Text
        ));
        assert!(!OperatorLimitSwitch::should_render_limits(
            false,
            AtomLimits::NoLimits,
            MathStyle::Display
        ));

        assert_eq!(std::mem::size_of::<BigOpVariantTier>(), 64);
        assert_eq!(std::mem::align_of::<BigOpVariantTier>(), 64);
        assert_eq!(BIG_OP_VARIANT_TIER.text_codes[0], 0xE5); // Sum
        assert_eq!(BIG_OP_VARIANT_TIER.display_codes[0], 0x72);

        let (oh_up, oh_dn) = IntegralOverhangCorrector::limit_offsets(2.0);
        assert_eq!(oh_up, 1.0);
        assert_eq!(oh_dn, -1.0);

        let big_metrics = PackedBigOpMetrics::new(1.0, 1.0, 0.5, 0.5);
        let (up_s, dn_s) = LimitClearanceGovernor::compute_limit_shifts(
            8.0,
            3.0,
            Some(&MathBox::with_dims(5.0, 3.0, 1.0)),
            Some(&MathBox::with_dims(5.0, 3.0, 1.0)),
            &big_metrics,
        );
        assert_eq!(up_s, 8.0 + 1.0 + 1.0);
        assert_eq!(dn_s, 3.0 + 1.0 + 3.0);

        let (mw, ox, ux, lx) =
            OperatorCenteringEngine::center_limits(10.0, Some(6.0), Some(8.0), 1.0, -1.0);
        assert_eq!(mw, 10.0);
        assert_eq!(ox, 0.0);
        assert_eq!(ux, 2.0 + 1.0);
        assert_eq!(lx, 1.0 - 1.0);

        // BATCH 7: Extensible Delimiters (Items 37–41)
        let delim_match = DelimiterPairMatch::new(DelimiterPairMatch::PAREN);
        assert!(!delim_match.is_null());
        let null_match = DelimiterPairMatch::new(DelimiterPairMatch::NULL);
        assert!(null_match.is_null());

        assert_eq!(std::mem::size_of::<DelimiterVariantResolver>(), 64);
        assert_eq!(std::mem::align_of::<DelimiterVariantResolver>(), 64);
        let (v_idx, ext_delim) = DELIMITER_VARIANT_RESOLVER.resolve_variant(10.0);
        assert_eq!(v_idx, 0);
        assert!(!ext_delim);
        let (_, ext_delim2) = DELIMITER_VARIANT_RESOLVER.resolve_variant(35.0);
        assert!(ext_delim2);

        assert!(NullDelimiterEliminator::is_null("."));
        assert!(NullDelimiterEliminator::is_null(""));
        assert!(!NullDelimiterEliminator::is_null("("));
        assert_eq!(NullDelimiterEliminator::null_box(), MathBox::zero());

        let target_dh = DelimiterSizingGovernor::target_height(10.0, 5.0, 2.5, 10.0);
        assert!(target_dh >= 10.0);

        let reps = ExtensibleAssemblyEngine::calculate_repeats(40.0, 10.0, 15.0);
        assert_eq!(reps, 3);

        // BATCH 8: Matrices & Vcenter (Items 42–46)
        let mut col_soa = MatrixColumnSoA::with_capacity(3);
        col_soa.widths[0] = 10.0;
        col_soa.widths[1] = 15.0;
        col_soa.widths[2] = 20.0;
        let total_mat_w = col_soa.compute_offsets(5.0);
        assert_eq!(total_mat_w, 10.0 + 5.0 + 15.0 + 5.0 + 20.0);
        assert_eq!(col_soa.offsets[0], 0.0);
        assert_eq!(col_soa.offsets[1], 15.0);
        assert_eq!(col_soa.offsets[2], 35.0);

        let row_boxes = [
            MathBox::with_dims(10.0, 5.0, 2.0),
            MathBox::with_dims(10.0, 7.0, 1.0),
            MathBox::with_dims(10.0, 4.0, 3.0),
        ];
        let (rmax_h, rmax_d) = RowBaselineAligner::compute_row_extents(&row_boxes);
        assert_eq!(rmax_h, 7.0);
        assert_eq!(rmax_d, 3.0);

        let (vc_delta, vc_h, vc_d) = VcenterGeometryEngine::compute_centering_shift(2.5, 8.0, 2.0);
        assert_eq!(vc_delta, 2.5 - 0.5 * (8.0 - 2.0));
        assert_eq!(vc_h, 2.5 + 5.0);
        assert_eq!(vc_d, 5.0 - 2.5);

        assert_eq!(
            MatrixDelimBinder::resolve_delims(MatrixDelim::Parentheses),
            ("(", ")")
        );
        assert_eq!(
            MatrixDelimBinder::resolve_delims(MatrixDelim::Brackets),
            ("[", "]")
        );
        assert_eq!(
            MatrixDelimBinder::resolve_delims(MatrixDelim::VBar),
            ("|", "|")
        );

        assert!(SparseGridCompressor::is_empty_cell(&MathBox::zero()));
        assert!(!SparseGridCompressor::is_empty_cell(&MathBox::with_dims(
            1.0, 0.0, 0.0
        )));

        // BATCH 9: Assembly & Verification (Items 47–50)
        let mut ops = Vec::new();
        let mut pool = Vec::new();
        MathPaintEmitter::emit_glyph_run(&mut ops, &mut pool, 0.0, 0.0, Face::Roman, 10.0, "test");
        assert_eq!(ops.len(), 1);
        assert_eq!(pool.len(), 1);
        MathPaintEmitter::emit_rule(&mut ops, 0.0, 0.0, 10.0, 1.0);
        assert_eq!(ops.len(), 2);

        assert_eq!(std::mem::size_of::<MathGeometricTraceRecord>(), 64);
        assert_eq!(std::mem::align_of::<MathGeometricTraceRecord>(), 64);

        let mut trace_buf = MathGeometricTraceBuffer::new();
        assert!(trace_buf.is_empty());
        trace_buf.push(MathGeometricTraceRecord::default());
        assert_eq!(trace_buf.len(), 1);
        assert!(!trace_buf.is_empty());

        let u_box = FormulaBoundingBoxUnion::union_horiz(
            &MathBox::with_dims(10.0, 5.0, 2.0),
            &MathBox::with_dims(15.0, 7.0, 1.0),
        );
        assert_eq!(u_box.w, 25.0);
        assert_eq!(u_box.h, 7.0);
        assert_eq!(u_box.d, 2.0);

        let uv_box = FormulaBoundingBoxUnion::union_vert(
            &MathBox::with_dims(10.0, 5.0, 2.0),
            &MathBox::with_dims(15.0, 4.0, 1.0),
            6.0,
        );
        assert_eq!(uv_box.w, 15.0);
        assert_eq!(uv_box.h, 10.0);

        assert!(ComprehensiveMathVerifier::verify_invariants());
    }

    #[test]
    fn test_batches_1_to_9_items_51_to_100() {
        use crate::math::*;

        // BATCH 1: Math Mode Grammar, Infix Fractions & Fences (Items 51–56)
        // Item 51: Infix \over, \atop, \atopwithdelims
        let over_node = parse_math_str(r"a + b \over c + d");
        match over_node {
            MathNode::Fraction {
                num, den, has_rule, ..
            } => {
                assert!(has_rule);
                assert!(matches!(*num, MathNode::Row(_)));
                assert!(matches!(*den, MathNode::Row(_)));
            }
            _ => panic!("Expected Fraction from \\over"),
        }

        let atop_node = parse_math_str(r"n \atop k");
        match atop_node {
            MathNode::Fraction { has_rule, .. } => assert!(!has_rule),
            _ => panic!("Expected Fraction from \\atop"),
        }

        let atop_delims = parse_math_str(r"n \atopwithdelims() k");
        match atop_delims {
            MathNode::Delimited(ref data) => {
                assert_eq!(data.open, "(");
                assert_eq!(data.close, ")");
            }
            _ => panic!("Expected Delimited from \\atopwithdelims"),
        }

        // Item 52: FractionOptionDecoder
        let above_node = parse_math_str(r"a \above exact norule 2pt b");
        match above_node {
            MathNode::Fraction {
                options, has_rule, ..
            } => {
                assert!(!has_rule);
                assert_ne!(options & math_ast::FRACTION_OPT_EXACT, 0);
                assert_ne!(options & math_ast::FRACTION_OPT_NORULE, 0);
            }
            _ => panic!("Expected Fraction from \\above"),
        }

        // Item 53: SubformulaFlattener
        let flattened = parse_math_str("{x}");
        assert_eq!(flattened, MathNode::Variable('x'));

        // Item 54: MathChoiceQuadBranches
        let choice_node = parse_math_str(r"\mathchoice{D}{T}{S}{SS}");
        match choice_node {
            MathNode::MathChoice(ref c) => {
                assert_eq!(c.display, MathNode::Variable('D'));
                assert_eq!(c.text, MathNode::Variable('T'));
                assert_eq!(c.script, MathNode::Variable('S'));
                assert_eq!(
                    c.script_script,
                    MathNode::Row(vec![MathNode::Variable('S'), MathNode::Variable('S')])
                );
            }
            _ => panic!("Expected MathChoice node"),
        }

        // Item 55 & 56: DelimiterOptionDecoder & MiddleDelimiterSupport
        assert_eq!(math_ast::DELIMITER_OPT_NONE, 0);
        let middle_node = parse_math_str(r"\left( a \middle| b \right)");
        match middle_node {
            MathNode::Delimited(ref data) => {
                assert_eq!(data.open, "(");
                assert_eq!(data.close, ")");
                match *data.content {
                    MathNode::Row(ref nodes) => {
                        assert!(nodes
                            .iter()
                            .any(|n| matches!(n, MathNode::MiddleDelim { delim: "|", .. })));
                    }
                    _ => panic!("Expected Row with MiddleDelim"),
                }
            }
            _ => panic!("Expected Delimited"),
        }

        // BATCH 2: Script Invariants & Error Recovery (Items 57–62)
        // Item 57: EmptyNucleusScriptAttacher
        let empty_nuc = parse_math_str("_1");
        assert!(matches!(empty_nuc, MathNode::SubSup { .. }));

        // Item 58: DoubleScriptConflictDetector
        let double_sub = parse_math_str("x_1_2");
        assert!(matches!(double_sub, MathNode::SubSup { .. }));
        let double_sup = parse_math_str("x^1^2");
        assert!(matches!(double_sup, MathNode::SubSup { .. }));

        // Item 59: NoScriptFlagEnforcer
        let no_sub = parse_math_str(r"x \nosubscript _1");
        assert_eq!(no_sub, MathNode::Variable('x'));

        // Item 61: SuperSubOrderInvariance
        let sub_sup1 = parse_math_str("x_1^2");
        let sub_sup2 = parse_math_str("x^2_1");
        assert_eq!(sub_sup1, sub_sup2);

        // Item 62: ScriptStyleDivergence
        assert_eq!(
            MathStyle::Display.sup_style().sup_style(),
            MathStyle::ScriptScript
        );
        assert_eq!(MathStyle::Display.sub_style(), MathStyle::CrampedScript);

        // BATCH 3: Math Family & Font Resolution (Items 63–68)
        assert_eq!(std::mem::size_of::<MathFamilyTable>(), 64);
        let mut fam_table = MathFamilyTable::new();
        fam_table.set(2, 0, 5);
        assert_eq!(fam_table.get(2, 0), 5);

        let mut fam_stack = MathFamilyScopeStack::new();
        fam_stack.push(2, 0, 3, 1);
        fam_table.set(2, 0, 7);
        fam_stack.unwind_to_level(0, &mut fam_table);
        assert_eq!(fam_table.get(2, 0), 3);

        let decoded_code = MathCodeDecoder::decode(0x212B);
        assert_eq!(decoded_code.class, MathAtomKind::Bin);
        assert_eq!(decoded_code.family, 1);
        assert_eq!(decoded_code.char_code, 0x2B);

        let ext_code = ExtendedMathCodeDecoder::decode(3, 2, 0x2264);
        assert_eq!(ext_code.class, MathAtomKind::Rel);
        assert_eq!(ext_code.family, 2);
        assert_eq!(ext_code.char_code, 0x2264);

        let del_code = DelimiterCodeDecoder::decode(0x028000).unwrap();
        assert_eq!(del_code.small_fam, 0);
        assert_eq!(del_code.small_char, 0x28);

        assert!(ActiveMathCodeInterceptor::is_active(0x8000));
        assert!(!ActiveMathCodeInterceptor::is_active(0x7FFF));

        // BATCH 4: Dynamic Math Engine Parameters (Items 69–74)
        assert_eq!(std::mem::size_of::<MathParameterRegistry>(), 512);
        let mut param_reg = MathParameterRegistry::new();
        param_reg.set(MATH_PARAM_AXIS, MathStyle::Display, 250);
        assert_eq!(param_reg.get(MATH_PARAM_AXIS, MathStyle::Display), 250);

        let mut param_stack = MathParamScopeStack::new();
        param_stack.push(MATH_PARAM_AXIS, MathStyle::Display, 250, 1);
        param_reg.set(MATH_PARAM_AXIS, MathStyle::Display, 300);
        param_stack.unwind_to_level(0, &mut param_reg);
        assert_eq!(param_reg.get(MATH_PARAM_AXIS, MathStyle::Display), 250);

        let mut spacing_override = MathSpacingParameterOverride::new();
        assert_eq!(
            spacing_override.get(MathAtomKind::Ord, MathAtomKind::Ord),
            None
        );
        spacing_override.set(MathAtomKind::Ord, MathAtomKind::Ord, 3);
        assert_eq!(
            spacing_override.get(MathAtomKind::Ord, MathAtomKind::Ord),
            Some(3)
        );

        assert_eq!(
            MathAxisHeightParameter::get(&param_reg, MathStyle::Display, 2.5),
            0.25
        );
        assert_eq!(
            RuleThicknessParameter::get(&param_reg, MathStyle::Display, 0.4),
            0.4
        );
        assert_eq!(
            ScriptDropParameters::sup_drop(&param_reg, MathStyle::Display, 1.5),
            1.5
        );
        assert_eq!(
            ScriptDropParameters::sub_drop(&param_reg, MathStyle::Display, 0.5),
            0.5
        );

        // BATCH 5: Display Math Entry, Geometry & Alignment (Items 75–80)
        assert_eq!(
            DisplayMathParagraphBreaker::compute_predisplaysize(100.0, 10.0, 2000.0),
            120.0
        );
        assert_eq!(
            EmptyParagraphDisplayFilter::filter_predisplaysize(false, 100.0),
            EmptyParagraphDisplayFilter::NEG_MAX_DIMEN
        );
        assert_eq!(
            EmptyParagraphDisplayFilter::filter_predisplaysize(true, 100.0),
            100.0
        );

        let (dw, di) = DisplayShapeGeometryResolver::resolve_geometry(400.0, 20.0, 1, 2);
        assert_eq!(dw, 380.0);
        assert_eq!(di, 20.0);

        assert!(EveryDisplayHookTrigger::has_hook(true));
        assert_eq!(
            DisplayHalignAdapter::center_table(200.0, 400.0, 10.0),
            110.0
        );

        let mut prev_graf = 3;
        let mut space_factor = 500;
        DisplayResumeStateRestorer::restore_state(&mut prev_graf, &mut space_factor);
        assert_eq!(prev_graf, 6);
        assert_eq!(space_factor, 1000);

        // BATCH 6: Equation Numbering & Tag Placement (Items 81–86)
        assert!(EquationTagSideSelector::is_left_side(true, false, false));
        assert!(!EquationTagSideSelector::is_left_side(false, false, false));
        assert!(!EquationTagSideSelector::is_left_side(true, true, false));

        let d1 = DisplayEquationCentering::compute_displacement(400.0, 200.0, 30.0);
        assert_eq!(d1, 100.0);
        let d2 = DisplayEquationCentering::compute_displacement(400.0, 300.0, 60.0);
        assert_eq!(d2, 20.0);

        assert!(EquationTagCollisionDisplacement::must_split_line(
            350.0, 50.0, 400.0, 10.0
        ));
        assert!(!EquationTagCollisionDisplacement::must_split_line(
            200.0, 30.0, 400.0, 10.0
        ));

        assert!(DisplaySkipModeSelector::should_emit_skip(
            DisplaySkipMode::Normal,
            10.0
        ));
        assert!(!DisplaySkipModeSelector::should_emit_skip(
            DisplaySkipMode::Suppress,
            10.0
        ));

        assert!(PreDisplayClearanceComparator::uses_short_skip(
            150.0, 0.0, 100.0, false
        ));
        assert!(!PreDisplayClearanceComparator::uses_short_skip(
            80.0, 0.0, 100.0, false
        ));
        assert_eq!(EquationTagPageBreakGuard::penalty(), 10000);

        // BATCH 7: Advanced Accents & Over/Under (Items 87–92)
        let double_acc = DoubleMathAccentComposer::compose(10.0, 5.0, 2.0, 1.0, 1.0);
        assert_eq!(double_acc.top_y, 16.0);
        assert_eq!(double_acc.bot_y, 7.0);

        let (ox, oy) = OverlayAccentApplier::compute_overlay_pos(10.0, 10.0, 8.0, 6.0, 2.0, 4.0);
        assert_eq!(ox, 12.0);
        assert_eq!(oy, 12.0);

        assert!(AccentFixedModifierHandler::is_fixed(1));
        assert!(!AccentFixedModifierHandler::is_fixed(0));

        assert!((AccentFractionAdjustment::adjust_clearance(2.0, 1500) - 3.0).abs() < 1e-5);
        assert_eq!(
            UnderDelimiterBuilder::compute_under_y(10.0, 2.0, 3.0, 1.0),
            4.0
        );
        assert_eq!(OverDelimiterBuilder::compute_over_y(10.0, 5.0, 1.0), 16.0);

        // BATCH 8: Generalized Delimiters (Items 93–96)
        assert_eq!(
            DelimiterSearchChain::select_tier(10.0, 10.0),
            DelimiterSearchTier::Small
        );
        assert_eq!(
            DelimiterSearchChain::select_tier(20.0, 10.0),
            DelimiterSearchTier::Large
        );
        assert_eq!(
            DelimiterSearchChain::select_tier(30.0, 10.0),
            DelimiterSearchTier::Extensible
        );

        assert!(MissingDelimiterFallback::is_null_delim("."));
        assert!(MissingDelimiterFallback::is_null_delim(""));
        assert!(!MissingDelimiterFallback::is_null_delim("("));

        let (th, td) = DelimiterExactSizing::compute_target_span(15.0, 5.0, 8.0, 2.0, 2.5);
        assert_eq!(th, 15.0);
        assert_eq!(td, 5.0);

        assert_eq!(
            DelimiterAxisAlignmentToggle::compute_y_center(10.0, 2.5, false),
            12.5
        );
        assert_eq!(
            DelimiterAxisAlignmentToggle::compute_y_center(10.0, 2.5, true),
            10.0
        );

        // BATCH 9: Inline Math Boundaries, Kerning & BiDi (Items 97–100)
        let spec = MathSurroundSpec {
            surround_w: 3.0,
            skip_mode: 0,
        };
        assert_eq!(InlineMathSurroundInjector::before_surround(spec), 3.0);
        assert_eq!(InlineMathSurroundInjector::after_surround(spec), 3.0);

        assert_eq!(
            InlineMathPenaltyInserter::break_penalty(MathAtomKind::Bin),
            Some(700)
        );
        assert_eq!(
            InlineMathPenaltyInserter::break_penalty(MathAtomKind::Rel),
            Some(500)
        );
        assert_eq!(
            InlineMathPenaltyInserter::break_penalty(MathAtomKind::Ord),
            None
        );

        assert!(MathDirectionalityManager::is_direction_reversed(
            MathDirection::LeftToRight,
            MathDirection::RightToLeft
        ));
        assert!(!MathDirectionalityManager::is_direction_reversed(
            MathDirection::LeftToRight,
            MathDirection::LeftToRight
        ));

        let mut sf = 1500u16;
        MathSpaceFactorNormalizer::normalize_space_factor(&mut sf);
        assert_eq!(sf, 1000);
    }

    #[test]
    fn test_pdflatex_parity_check() {
        use crate::font_metrics::FontFamily;
        use crate::math::math_parser::parse_math_str;
        use crate::math::math_layout::measure_math;
        use crate::math::math_ast::MathStyle;

        let b0 = measure_math(&parse_math_str("x^2"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        let b1 = measure_math(&parse_math_str("x^2 + y_1 = z"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        let b2 = measure_math(&parse_math_str(r"\mathbb{R}"), 10.0, FontFamily::ComputerModern, MathStyle::Text);

        // Ground-truth pdflatex verification:
        // pdflatex B0 ($x^2$):            10.2014 pt width, 8.1400 pt height
        // pdflatex B1 ($x^2 + y_1 = z$): 50.2360 pt width, 8.1400 pt height
        // pdflatex B2 ($\mathbb{R}$):     7.2222 pt width, 6.8889 pt height
        assert!((b0.w - 10.2014).abs() < 0.005, "Expected 10.2014pt for $x^2$, got {}", b0.w);
        assert!((b0.h - 8.1400).abs() < 0.005, "Expected 8.1400pt height for $x^2$, got {}", b0.h);

        assert!((b1.w - 50.2360).abs() < 0.005, "Expected 50.2360pt for $x^2 + y_1 = z$, got {}", b1.w);
        assert!((b1.h - 8.1400).abs() < 0.005, "Expected 8.1400pt height for $x^2 + y_1 = z$, got {}", b1.h);

        assert!((b2.w - 7.2222).abs() < 0.005, "Expected 7.2222pt for $\\mathbb{{R}}$, got {}", b2.w);
        assert!((b2.h - 6.8889).abs() < 0.005, "Expected 6.8889pt height for $\\mathbb{{R}}$, got {}", b2.h);

        let b_xy = measure_math(&parse_math_str("x + y"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        assert!((b_xy.w - 23.1990).abs() < 0.01, "Expected 23.1990pt for $x + y$, got {}", b_xy.w);

        let b_ab = measure_math(&parse_math_str("a - b"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        assert!((b_ab.w - 21.7997).abs() < 0.01, "Expected 21.7997pt for $a - b$, got {}", b_ab.w);

        let b_greek = measure_math(&parse_math_str(r"\alpha + \beta = \gamma"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        assert!((b_greek.w - 43.9063).abs() < 0.03, "Expected 43.9063pt for $\\alpha + \\beta = \\gamma$, got {}", b_greek.w);

        let b_fx = measure_math(&parse_math_str("f(x) = x^2"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        assert!((b_fx.w - 43.0000).abs() < 0.02, "Expected 43.0000pt for $f(x) = x^2$, got {}", b_fx.w);

        let b_iint = measure_math(&parse_math_str(r"\iint"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        let b_iiint = measure_math(&parse_math_str(r"\iiint"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        let b_iint_d = measure_math(&parse_math_str(r"\iint"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_iiint_d = measure_math(&parse_math_str(r"\iiint"), 10.0, FontFamily::ComputerModern, MathStyle::Display);

        assert!((b_iint.w - 11.6667).abs() < 0.02, "Expected 11.6667pt for text \\iint, got {}", b_iint.w);
        assert!((b_iiint.w - 16.6667).abs() < 0.02, "Expected 16.6667pt for text \\iiint, got {}", b_iiint.w);
        assert!((b_iint_d.w - 16.6668).abs() < 0.02, "Expected 16.6668pt for display \\iint, got {}", b_iint_d.w);
        assert!((b_iiint_d.w - 23.3335).abs() < 0.02, "Expected 23.3335pt for display \\iiint, got {}", b_iiint_d.w);

        let b_tight = measure_math(&parse_math_str(r"\iint\! f \, \mathrm{d}A"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        let b_loose = measure_math(&parse_math_str(r"\iint f \, \mathrm{d}A"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        // \! cancels \thinmuskip (1.6667pt) exactly:
        assert!((b_loose.w - b_tight.w - 1.6667).abs() < 0.01, "Expected 1.6667pt difference between loose and tight, got {}", b_loose.w - b_tight.w);

        // Ground-truth pdflatex verification for integrals:
        let b_int_t = measure_math(&parse_math_str(r"\int"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        let b_int_d = measure_math(&parse_math_str(r"\int"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_int_fx = measure_math(&parse_math_str(r"\int f(x) dx"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        assert!((b_int_t.w - 6.6667).abs() < 0.01, "Expected 6.6667pt for text \\int, got {}", b_int_t.w);
        assert!((b_int_t.h - 8.0556).abs() < 0.01, "Expected 8.0556pt height for text \\int, got {}", b_int_t.h);
        assert!((b_int_t.d - 3.0556).abs() < 0.01, "Expected 3.0556pt depth for text \\int, got {}", b_int_t.d);
        assert!((b_int_d.w - 10.0000).abs() < 0.01, "Expected 10.0000pt for display \\int, got {}", b_int_d.w);
        assert!((b_int_d.h - 13.6112).abs() < 0.01, "Expected 13.6112pt height for display \\int, got {}", b_int_d.h);
        assert!((b_int_d.d - 8.6112).abs() < 0.01, "Expected 8.6112pt depth for display \\int, got {}", b_int_d.d);
        assert!((b_int_fx.w - 38.7188).abs() < 0.02, "Expected 38.7188pt for \\int f(x) dx, got {}", b_int_fx.w);

        // Ground-truth pdflatex verification for radicals:
        let b_sqrt_x = measure_math(&parse_math_str(r"\sqrt{x}"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        let b_sqrt_2 = measure_math(&parse_math_str(r"\sqrt{2}"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        let b_sqrt_half = measure_math(&parse_math_str(r"\sqrt{\frac{1}{2}}"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        assert!((b_sqrt_x.w - 14.0486).abs() < 0.01, "Expected 14.0486pt for \\sqrt{{x}}, got {}", b_sqrt_x.w);
        assert!((b_sqrt_2.w - 13.3334).abs() < 0.01, "Expected 13.3334pt for \\sqrt{{2}}, got {}", b_sqrt_2.w);
        assert!((b_sqrt_half.w - 16.3861).abs() < 0.01, "Expected 16.3861pt for \\sqrt{{\\frac{{1}}{{2}}}}, got {}", b_sqrt_half.w);

        // Verify CMEX Unicode mapping parity:
        use crate::backend::backend_cm_fonts::CMEX_TO_UNICODE;
        assert_eq!(CMEX_TO_UNICODE[0x52], 0x222B, "CMEX 0x52 (text int) maps to U+222B");
        assert_eq!(CMEX_TO_UNICODE[0x5A], 0x222B, "CMEX 0x5A (display int) maps to U+222B");
        assert_eq!(CMEX_TO_UNICODE[0x70], 0x221A, "CMEX 0x70 (radical var 0) maps to U+221A");
        assert_eq!(CMEX_TO_UNICODE[0x72], 0x221A, "CMEX 0x72 (radical var 2) maps to U+221A");
        assert_eq!(CMEX_TO_UNICODE[0x50], 0x2211, "CMEX 0x50 (text sum) maps to U+2211");
        assert_eq!(CMEX_TO_UNICODE[0x58], 0x2211, "CMEX 0x58 (display sum) maps to U+2211");
    }

    #[test]
    fn test_milestone_8_multiline_grid_parity() {
        use crate::font_metrics::FontFamily;
        use crate::math::math_parser::parse_math_str;
        use crate::math::math_layout::measure_math;
        use crate::math::math_ast::MathStyle;

        let b_matrix = measure_math(&parse_math_str(r"\begin{matrix} a & b \\ c & d \end{matrix}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_aligned = measure_math(&parse_math_str(r"\begin{aligned} a &= b \\ c &= d \end{aligned}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_cases = measure_math(&parse_math_str(r"\begin{cases} a & b \\ c & d \end{cases}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_gathered = measure_math(&parse_math_str(r"\begin{gathered} a = b \\ c = d \end{gathered}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_pmatrix = measure_math(&parse_math_str(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_bmatrix = measure_math(&parse_math_str(r"\begin{bmatrix} a & b \\ c & d \end{bmatrix}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_multline = measure_math(&parse_math_str(r"\begin{multline*} a + b \\ + c + d \end{multline*}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);

        // 1. Matrix: 20.4908 pt width, 14.5000 pt height, 9.5000 pt depth
        assert!((b_matrix.w - 20.4908).abs() < 0.01, "Matrix width parity: expected ~20.4908pt, got {}", b_matrix.w);
        assert!((b_matrix.h - 14.5000).abs() < 0.01, "Matrix height parity: expected 14.5000pt, got {}", b_matrix.h);
        assert!((b_matrix.d - 9.5000).abs() < 0.01, "Matrix depth parity: expected 9.5000pt, got {}", b_matrix.d);

        // 2. Aligned: 33.8240 pt width, 16.0000 pt height, 11.0000 pt depth (\jot = 3pt baselineskip expansion)
        assert!((b_aligned.w - 33.8240).abs() < 0.01, "Aligned width parity: expected ~33.8240pt, got {}", b_aligned.w);
        assert!((b_aligned.h - 16.0000).abs() < 0.01, "Aligned height parity: expected 16.0000pt, got {}", b_aligned.h);
        assert!((b_aligned.d - 11.0000).abs() < 0.01, "Aligned depth parity: expected 11.0000pt, got {}", b_aligned.d);

        // 3. Cases: 29.7464 pt width, 17.5001 pt height, 12.5002 pt depth (\arraystretch = 1.2, left brace + null delim)
        assert!((b_cases.w - 29.7464).abs() < 0.01, "Cases width parity: expected ~29.7464pt, got {}", b_cases.w);
        assert!((b_cases.h - 17.5001).abs() < 0.01, "Cases height parity: expected ~17.5001pt, got {}", b_cases.h);
        assert!((b_cases.d - 12.5002).abs() < 0.01, "Cases depth parity: expected ~12.5002pt, got {}", b_cases.d);

        // 4. Gathered: 22.9108 pt width, 16.0000 pt height, 11.0000 pt depth (\jot = 3pt)
        assert!((b_gathered.w - 22.9108).abs() < 0.01, "Gathered width parity: expected ~22.9108pt, got {}", b_gathered.w);
        assert!((b_gathered.h - 16.0000).abs() < 0.01, "Gathered height parity: expected 16.0000pt, got {}", b_gathered.h);
        assert!((b_gathered.d - 11.0000).abs() < 0.01, "Gathered depth parity: expected 11.0000pt, got {}", b_gathered.d);

        // 5. Pmatrix & Bmatrix: 35.2130 pt and 31.0464 pt width, 14.5001 pt height, 9.5001 pt depth
        assert!((b_pmatrix.w - 35.2130).abs() < 0.01, "Pmatrix width parity: expected ~35.2130pt, got {}", b_pmatrix.w);
        assert!((b_pmatrix.h - 14.5001).abs() < 0.01, "Pmatrix height parity: expected ~14.5001pt, got {}", b_pmatrix.h);
        assert!((b_bmatrix.w - 31.0464).abs() < 0.01, "Bmatrix width parity: expected ~31.0464pt, got {}", b_bmatrix.w);
        assert!((b_bmatrix.h - 14.5001).abs() < 0.01, "Bmatrix height parity: expected ~14.5001pt, got {}", b_bmatrix.h);

        // 6. Multline: 16.0000 pt height, 11.0000 pt depth, left/right \multlinegap alignment
        assert!((b_multline.h - 16.0000).abs() < 0.01, "Multline height parity: expected 16.0000pt, got {}", b_multline.h);
        assert!((b_multline.d - 11.0000).abs() < 0.01, "Multline depth parity: expected 11.0000pt, got {}", b_multline.d);

        // Verify Multline rendering offsets:
        let mut ops = Vec::new();
        let mut pool = Vec::new();
        let multline_node = parse_math_str(r"\begin{multline*} a + b \\ + c + d \end{multline*}");
        crate::math::render_math(&multline_node, 10.0, FontFamily::ComputerModern, MathStyle::Display, 0.0, 0.0, &mut ops, &mut pool);

        let op_a = ops.iter().find(|op| match op {
            crate::layout::engine::Paint::GlyphRun { part_start, .. } => pool[*part_start as usize] == "a",
            _ => false,
        }).expect("glyph 'a' rendered");
        let op_c = ops.iter().find(|op| match op {
            crate::layout::engine::Paint::GlyphRun { part_start, .. } => pool[*part_start as usize] == "c",
            _ => false,
        }).expect("glyph 'c' rendered");

        let x_a = match op_a { crate::layout::engine::Paint::GlyphRun { x, .. } => *x, _ => 0.0 };
        let x_c = match op_c { crate::layout::engine::Paint::GlyphRun { x, .. } => *x, _ => 0.0 };

        // Row 0 starts at multline_gap (10.0pt)
        assert!((x_a - 10.0).abs() < 1e-2, "First line of multline indented by multlinegap (10pt): got {}", x_a);
        // Row 1 is shifted further to the right
        assert!(x_c > x_a, "Subsequent line in multline shifted toward right margin");
    }

    #[test]
    fn test_milestone_9_advanced_accents_and_wide_delimiters() {
        use crate::font_metrics::FontFamily;
        use crate::math::math_parser::parse_math_str;
        use crate::math::math_layout::measure_math;
        use crate::math::math_ast::MathStyle;

        // 1. Wide accents: \widehat and \widetilde variant selection
        let b_wh_x = measure_math(&parse_math_str(r"\widehat{x}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_wh_xyz = measure_math(&parse_math_str(r"\widehat{xyz}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_wt_x = measure_math(&parse_math_str(r"\widetilde{x}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_wt_xyz = measure_math(&parse_math_str(r"\widetilde{xyz}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);

        // Variant 0 (0x62) for single char 'x': ht=7.22223pt, wd=5.71527pt
        assert!((b_wh_x.h - 7.22223).abs() < 0.01, r"\widehat{{x}} height parity: expected 7.22223pt, got {}", b_wh_x.h);
        assert!((b_wh_x.w - 5.71527).abs() < 0.01, r"\widehat{{x}} width parity: expected 5.71527pt, got {}", b_wh_x.w);
        assert!((b_wt_x.h - 7.22223).abs() < 0.01, r"\widetilde{{x}} height parity: expected 7.22223pt, got {}", b_wt_x.h);

        // Variant 2 (0x64) for wider string 'xyz': ht=7.50000pt
        assert!((b_wh_xyz.h - 7.50000).abs() < 0.01, r"\widehat{{xyz}} height parity: expected 7.50000pt, got {}", b_wh_xyz.h);
        assert!((b_wt_xyz.h - 7.50000).abs() < 0.01, r"\widetilde{{xyz}} height parity: expected 7.50000pt, got {}", b_wt_xyz.h);

        // 2. Canonical single accents and italic/slanted nuclei skew positioning
        let b_vec_x = measure_math(&parse_math_str(r"\vec{x}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_vec_a = measure_math(&parse_math_str(r"\vec{A}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_hat_x = measure_math(&parse_math_str(r"\hat{x}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_bar_x = measure_math(&parse_math_str(r"\bar{x}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_overline = measure_math(&parse_math_str(r"\overline{x}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_underline = measure_math(&parse_math_str(r"\underline{x}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);

        assert!((b_vec_x.h - 7.14444).abs() < 0.02, r"\vec{{x}} height parity: expected 7.14444pt, got {}", b_vec_x.h);
        assert!((b_vec_a.h - 9.67221).abs() < 0.02, r"\vec{{A}} height parity: expected 9.67221pt, got {}", b_vec_a.h);
        assert!((b_hat_x.h - 6.94444).abs() < 0.02, r"\hat{{x}} height parity: expected 6.94444pt, got {}", b_hat_x.h);
        assert!((b_bar_x.h - 5.67776).abs() < 0.02, r"\bar{{x}} height parity: expected 5.67776pt, got {}", b_bar_x.h);
        assert!((b_overline.h - 6.30544).abs() < 0.02, r"\overline{{x}} height parity: expected 6.30544pt, got {}", b_overline.h);
        assert!((b_underline.d - 1.99990).abs() < 0.02, r"\underline{{x}} depth parity: expected 1.99990pt, got {}", b_underline.d);

        // Verify that \vec{A} accent glyph is shifted right by skew kern:
        let mut ops_vec_a = Vec::new();
        let mut pool_vec_a = Vec::new();
        crate::math::render_math(&parse_math_str(r"\vec{A}"), 10.0, FontFamily::ComputerModern, MathStyle::Display, 0.0, 0.0, &mut ops_vec_a, &mut pool_vec_a);
        let arrow_glyph = ops_vec_a.iter().find(|op| match op {
            crate::layout::engine::Paint::GlyphRun { face, part_start, .. } => *face == Face::MathItalic && pool_vec_a[*part_start as usize] != "A",
            _ => false,
        }).expect("arrow glyph rendered");
        let arrow_x = match arrow_glyph { crate::layout::engine::Paint::GlyphRun { x, .. } => *x, _ => 0.0 };
        // Unskewed center would be (7.50002 - 5.00002)/2 = 1.25pt. Skew kern adds 1.38893pt -> 2.63893pt!
        assert!((arrow_x - 2.6389).abs() < 0.05, r"\vec{{A}} skew placement: expected ~2.6389pt, got {}", arrow_x);

        // 3. Extensible horizontal braces: \overbrace and \underbrace
        let b_over = measure_math(&parse_math_str(r"\overbrace{a+b+c}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_over_sup = measure_math(&parse_math_str(r"\overbrace{a+b+c}^k"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_under = measure_math(&parse_math_str(r"\underbrace{a+b+c}"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_under_sub = measure_math(&parse_math_str(r"\underbrace{a+b+c}_k"), 10.0, FontFamily::ComputerModern, MathStyle::Display);

        assert!((b_over.h - 14.14441).abs() < 0.01, r"\overbrace height parity: expected 14.14441pt, got {}", b_over.h);
        assert!((b_over.d - 0.83333).abs() < 0.01, r"\overbrace depth parity: expected 0.83333pt, got {}", b_over.d);
        assert!((b_over.w - 38.34938).abs() < 0.01, r"\overbrace width parity: expected 38.34938pt, got {}", b_over.w);

        assert!((b_over_sup.h - 22.0055).abs() < 0.01, r"\overbrace^k height parity: expected 22.0055pt, got {}", b_over_sup.h);
        assert!((b_over_sup.d - 0.83333).abs() < 0.01, r"\overbrace^k depth parity: expected 0.83333pt, got {}", b_over_sup.d);

        assert!((b_under.h - 6.94444).abs() < 0.01, r"\underbrace height parity: expected 6.94444pt, got {}", b_under.h);
        assert!((b_under.d - 8.03330).abs() < 0.01, r"\underbrace depth parity: expected 8.03330pt, got {}", b_under.d);
        assert!((b_under.w - 38.34938).abs() < 0.01, r"\underbrace width parity: expected 38.34938pt, got {}", b_under.w);

        assert!((b_under_sub.h - 6.94444).abs() < 0.01, r"\underbrace_k height parity: expected 6.94444pt, got {}", b_under_sub.h);
        assert!((b_under_sub.d - 15.56105).abs() < 0.01, r"\underbrace_k depth parity: expected 15.56105pt, got {}", b_under_sub.d);

        // 4. Authoritative \mathchoice 4-style evaluation
        let choice_node = parse_math_str(r"\mathchoice{D}{T}{S}{SS}");
        let b_d = measure_math(&choice_node, 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let b_t = measure_math(&choice_node, 10.0, FontFamily::ComputerModern, MathStyle::Text);
        let b_s = measure_math(&choice_node, 10.0, FontFamily::ComputerModern, MathStyle::Script);
        let b_ss = measure_math(&choice_node, 10.0, FontFamily::ComputerModern, MathStyle::ScriptScript);

        let d_only = measure_math(&parse_math_str("D"), 10.0, FontFamily::ComputerModern, MathStyle::Display);
        let t_only = measure_math(&parse_math_str("T"), 10.0, FontFamily::ComputerModern, MathStyle::Text);
        let s_only = measure_math(&parse_math_str("S"), 10.0, FontFamily::ComputerModern, MathStyle::Script);
        let ss_only = measure_math(&parse_math_str("SS"), 10.0, FontFamily::ComputerModern, MathStyle::ScriptScript);

        assert!((b_d.w - d_only.w).abs() < 1e-4, r"\mathchoice Display branch mismatch");
        assert!((b_t.w - t_only.w).abs() < 1e-4, r"\mathchoice Text branch mismatch");
        assert!((b_s.w - s_only.w).abs() < 1e-4, r"\mathchoice Script branch mismatch");
        assert!((b_ss.w - ss_only.w).abs() < 1e-4, r"\mathchoice ScriptScript branch mismatch");
    }

    #[test]
    fn test_display_math_vertical_spacing_and_page_builder() {
        // Case 1: Short preceding line -> abovedisplayshortskip (0pt)
        let short_doc_src = r"\documentclass{article}
\begin{document}
Hi
\begin{equation*}
E = mc^2
\end{equation*}
\end{document}";
        let doc_short = crate::tex_parser::parse(short_doc_src).expect("Parsing must succeed");
        let pages_short = crate::layout::layout_engine::layout(&doc_short);
        assert!(!pages_short.is_empty());

        for (idx, op) in pages_short[0].ops.iter().enumerate() {
            eprintln!("short op[{}]: {:?}", idx, op);
        }

        // Case 2: Long preceding line that fills almost the entire measure (> 400pt)
        let long_doc_src = r"\documentclass{article}
\begin{document}
This is a very long paragraph that spans across the entire line so that the predisplaysize is large enough to exceed the equation clearance and reaches almost the right margin of the page.
\begin{equation*}
E = mc^2
\end{equation*}
\end{document}";
        let doc_long = crate::tex_parser::parse(long_doc_src).expect("Parsing must succeed");
        let pages_long = crate::layout::layout_engine::layout(&doc_long);
        assert!(!pages_long.is_empty());

        let text_y_short = match pages_short[0].ops[0] {
            crate::layout::layout_engine::Paint::GlyphRun { y, .. } => y,
            _ => panic!("Expected GlyphRun"),
        };
        let math_y_short = match pages_short[0].ops[1] {
            crate::layout::layout_engine::Paint::GlyphRun { y, .. } => y,
            _ => panic!("Expected GlyphRun"),
        };

        let last_text_y_long = match pages_long[0].ops[1] {
            crate::layout::layout_engine::Paint::GlyphRun { y, .. } => y,
            _ => panic!("Expected GlyphRun"),
        };
        let math_y_long = match pages_long[0].ops[2] {
            crate::layout::layout_engine::Paint::GlyphRun { y, .. } => y,
            _ => panic!("Expected GlyphRun"),
        };

        let delta_short = text_y_short - math_y_short;
        let delta_long = last_text_y_long - math_y_long;
        // The long paragraph's line reaches past the equation clearance, triggering
        // abovedisplayskip (11.0pt), while the short line triggers abovedisplayshortskip (0.0pt).
        // The difference must be exactly base_font_size (11.0pt).
        assert!(
            ((delta_long - delta_short) - 11.0).abs() < 1e-4,
            "Expected exact 11.0pt skip difference, got {}",
            delta_long - delta_short
        );
    }

    #[test]
    fn test_milestone_10_differential_regression_consolidation_and_certification() {
        use crate::font_metrics::FontFamily;
        use crate::math::math_parser::parse_math_str;
        use crate::math::math_layout::measure_math;
        use crate::math::math_ast::MathStyle;

        // 50 Canonical Expressions directly measured from TeX Live / pdfLaTeX ground truth ($ ... $ mode)
        let corpus: [(&str, f32, f32, f32); 50] = [
            ("x", 5.7153, 4.3055, 0.0000),
            ("x + y", 23.1990, 5.8333, 1.9444),
            ("a - b", 21.7997, 6.9444, 0.8333),
            ("a = b", 22.9108, 6.9444, 0.0000),
            ("x < y", 24.3101, 5.3910, 1.9444),
            ("x > y", 24.3101, 5.3910, 1.9444),
            ("x^2", 10.2014, 8.1400, 0.0000),
            ("x_1", 10.2014, 4.3055, 1.5000),
            ("x_1^2", 10.2014, 8.1400, 2.4821),
            ("x^2 + y_1 = z", 50.2360, 8.1400, 1.9444),
            (r"\alpha + \beta = \gamma", 43.9063, 6.9444, 1.9444),
            (r"\pi r^2", 15.3345, 8.1400, 0.0000),
            (r"e^{-x^2}", 19.8438, 9.8689, 0.0000),
            ("f(x) = x^2", 43.0000, 8.1400, 2.5000),
            (r"\sin(x) + \cos(y)", 64.4213, 7.5000, 2.5000),
            (r"\log(x) = 0", 44.7430, 7.5000, 2.5000),
            (r"\theta \le \phi", 24.2638, 6.9444, 1.9444),
            (r"\lambda \ge \mu", 25.1921, 6.9444, 1.9444),
            (r"A \subset B", 28.9200, 6.8333, 0.3910),
            (r"x \in \mathbb{R}", 25.1596, 6.8889, 0.3910),
            (r"\frac{1}{2}", 6.3861, 8.4484, 3.4484),
            (r"\frac{a}{b}", 6.7376, 6.9512, 3.4484),
            (r"\frac{a+b}{c+d}", 16.3932, 8.7984, 4.2818),
            (r"\frac{x^2}{y^2}", 10.8376, 10.1773, 4.8095),
            (r"\frac{1}{1+x}", 17.0598, 8.4484, 4.2818),
            (r"\frac{\alpha}{\beta}", 7.5988, 6.9512, 4.8095),
            (r"\frac{x+y}{2}", 17.3804, 8.7944, 3.4484),
            (r"\frac{-b}{2a}", 12.1667, 8.7984, 3.4484),
            (r"\frac{d f}{d x}", 11.0976, 9.3222, 3.4484),
            (r"\frac{\partial u}{\partial t}", 11.8291, 8.7984, 3.4484),
            (r"\sqrt{x}", 14.0486, 8.0027, 2.3972),
            (r"\sqrt{2}", 13.3334, 9.0722, 1.3278),
            (r"\sqrt{x+y}", 31.5324, 7.7944, 2.6056),
            (r"\sqrt{x^2+y^2}", 42.1713, 9.5778, 2.8223),
            (r"\sqrt{\frac{1}{2}}", 16.3861, 12.3501, 6.0501),
            (r"\sqrt{\frac{a}{b}}", 16.7377, 8.6014, 3.7987),
            (r"\sqrt{1+x^2}", 35.7569, 9.1333, 1.2667),
            (r"\sqrt{a^2+b^2}", 39.1053, 9.1333, 1.2667),
            (r"\sqrt{x_1}", 18.5348, 7.2527, 3.1472),
            (r"\sqrt{2\pi}", 19.3924, 9.0722, 1.3278),
            (r"\int f(x) dx", 38.7188, 8.0556, 3.0556),
            (r"\iint", 11.6667, 8.0556, 3.0556),
            (r"\iiint", 16.6668, 8.0556, 3.0556),
            (r"\sum x_i", 21.2668, 7.5001, 2.5001),
            (r"\prod a_k", 21.3172, 7.5001, 2.5001),
            ("(a + b)", 29.5775, 7.5000, 2.5000),
            ("[x - y]", 28.7546, 7.5000, 2.5000),
            ("(x + y)^2", 35.4629, 8.1400, 2.5000),
            (r"\pm \sqrt{D}", 24.6681, 9.2666, 1.1334),
            ("E = m c^2", 38.8854, 8.1400, 0.0000),
        ];

        let mut max_w_err = 0.0f32;
        let mut max_h_err = 0.0f32;
        let mut max_d_err = 0.0f32;

        for (idx, &(expr, exp_w, exp_h, exp_d)) in corpus.iter().enumerate() {
            let node = parse_math_str(expr);
            let mb = measure_math(&node, 10.0, FontFamily::ComputerModern, MathStyle::Text);

            let err_w = (mb.w - exp_w).abs();
            let err_h = (mb.h - exp_h).abs();
            let err_d = (mb.d - exp_d).abs();

            max_w_err = max_w_err.max(err_w);
            max_h_err = max_h_err.max(err_h);
            max_d_err = max_d_err.max(err_d);

            assert!(
                err_w < 0.05,
                "Formula [{}] '{}' width mismatch: actual={:.4}, expected={:.4}, err={:.4}",
                idx, expr, mb.w, exp_w, err_w
            );
            assert!(
                err_h < 0.05,
                "Formula [{}] '{}' height mismatch: actual={:.4}, expected={:.4}, err={:.4}",
                idx, expr, mb.h, exp_h, err_h
            );
            assert!(
                err_d < 0.05,
                "Formula [{}] '{}' depth mismatch: actual={:.4}, expected={:.4}, err={:.4}",
                idx, expr, mb.d, exp_d, err_d
            );
        }

        eprintln!(
            "\n[Milestone 10 Parity Certification: 50/50 Formulas Validated]\n  Max width deviation:  {:.4} pt\n  Max height deviation: {:.4} pt\n  Max depth deviation:  {:.4} pt",
            max_w_err, max_h_err, max_d_err
        );
    }
}

