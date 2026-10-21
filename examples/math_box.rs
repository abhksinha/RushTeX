//! Math box oracle, our side: reads `D|T|S|SS<TAB>formula` lines on stdin and prints the
//! formula's width, height and depth in scaled points (10pt Computer Modern), for
//! `scripts/parity/mathbox.py` to compare with pdfTeX's `\wd`, `\ht`, `\dp`.
use rushtex::backend::fonts::FontFamily;
use rushtex::math::math_ast::MathStyle;
use std::io::BufRead;

fn main() {
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let Some((style, formula)) = line.split_once('\t') else { continue };
        let style = match style {
            "D" => MathStyle::Display,
            "T" => MathStyle::Text,
            "S" => MathStyle::Script,
            _ => MathStyle::ScriptScript,
        };
        let sp = |pt: f32| (pt as f64 * 65536.0).round() as i64;
        // The path inline and display math take: the formula's box with its trailing italic
        // correction, as TeX's mlist ends (§752).
        let node = rushtex::math::math_parser::parse_math_str(formula);
        let b = rushtex::math::math_layout::measure_math(&node, 10.0, FontFamily::ComputerModern, style);
        println!("{} {} {}", sp(b.w), sp(b.h), sp(b.d));
    }
}
