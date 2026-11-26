use super::Design;
use super::lex::{balanced_parens, split_top_level, strip_comments_sv};
use super::vhdl::{guess_direction, squash};
use crate::model::{Direction, Instance, Port};
use regex::Regex;
use std::collections::HashMap;

/// Words that can start a statement but are never a module type name.
const KEYWORDS: &[&str] = &[
    "assign",
    "always",
    "always_comb",
    "always_ff",
    "always_latch",
    "initial",
    "final",
    "if",
    "else",
    "for",
    "foreach",
    "while",
    "repeat",
    "case",
    "casez",
    "casex",
    "endcase",
    "begin",
    "end",
    "generate",
    "endgenerate",
    "function",
    "endfunction",
    "task",
    "endtask",
    "genvar",
    "localparam",
    "parameter",
    "typedef",
    "import",
    "export",
    "wire",
    "logic",
    "reg",
    "input",
    "output",
    "inout",
    "integer",
    "int",
    "bit",
    "byte",
    "real",
    "time",
    "assert",
    "assume",
    "cover",
    "property",
    "sequence",
    "class",
    "interface",
    "endinterface",
    "modport",
    "package",
    "endpackage",
    "return",
    "defparam",
    "module",
    "endmodule",
    "var",
    "struct",
    "enum",
    "union",
];
const PRIMITIVES: &[&str] = &[
    "and", "nand", "or", "nor", "xor", "xnor", "not", "buf", "bufif0", "bufif1", "notif0", "notif1",
];
/// Words stripped from a port/signal declaration to leave the bare name.
const DECL_WORDS: &[&str] = &[
    "input", "output", "inout", "ref", "wire", "logic", "reg", "var", "signed", "unsigned", "bit",
    "tri", "uwire", "supply0", "supply1", "integer", "int", "byte", "shortint", "longint", "real",
    "time",
];

struct Raw {
    label: String,
    module: String,
    conns: Vec<String>,
}

#[derive(Default)]
struct Module {
    name: String,
    ports: Vec<Port>,
    signals: HashMap<String, String>,
    raws: Vec<Raw>,
    warnings: Vec<String>,
}

fn is_ident(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

fn tokens(src: &str) -> Vec<(usize, usize)> {
    let re = Regex::new(r#"[A-Za-z_][A-Za-z0-9_$]*|\d[\w']*|"(?:\\.|[^"\\])*"|\S"#).unwrap();
    re.find_iter(src).map(|m| (m.start(), m.end())).collect()
}

/// Bare declared name in one comma-separated declaration entry (ranges, defaults, type words removed).
fn last_ident(entry: &str) -> Option<String> {
    let no_range = Regex::new(r"\[[^\]]*\]")
        .unwrap()
        .replace_all(entry, " ")
        .into_owned();
    let before_eq = no_range.split('=').next().unwrap_or("");
    before_eq
        .split_whitespace()
        .rfind(|w| !DECL_WORDS.contains(w) && is_ident(w))
        .map(str::to_string)
}

fn dir_of(word: &str) -> Direction {
    match word {
        "input" => Direction::In,
        "output" => Direction::Out,
        _ => Direction::InOut,
    }
}

/// Returns (ports, is_ansi). Non-ANSI ports start as InOut until the body declares them.
fn parse_header(h: &str, warnings: &mut Vec<String>) -> (Vec<Port>, bool) {
    let dir_re = Regex::new(r"\b(input|output|inout)\b").unwrap();
    let entries = split_top_level(h, ',');
    let ansi = entries.iter().any(|e| dir_re.is_match(e));
    let (mut cur, mut ports) = (None::<Direction>, Vec::new());
    for e in entries {
        if e.is_empty() {
            continue;
        }
        if let Some(m) = dir_re.find(e) {
            cur = Some(dir_of(m.as_str()));
        } else if e.contains('.') {
            warnings.push(format!("interface port '{e}' skipped"));
            continue;
        }
        if let Some(n) = last_ident(e) {
            ports.push(Port::new(
                &n,
                if ansi {
                    cur.unwrap_or(Direction::InOut)
                } else {
                    Direction::InOut
                },
                &n,
            ));
        }
    }
    (ports, ansi)
}

/// Index of the first token after the paren group opening at token `i`, plus the group's inner text.
fn paren<'a>(src: &'a str, tk: &[(usize, usize)], i: usize) -> (Option<&'a str>, usize) {
    match balanced_parens(src, tk[i].0) {
        Some(inner) => {
            let end = tk[i].0 + inner.len() + 2;
            let mut n = i;
            while n < tk.len() && tk[n].0 < end {
                n += 1;
            }
            (Some(inner), n)
        }
        None => (None, tk.len()),
    }
}

fn parse_body(src: &str, tk: &[(usize, usize)], (bs, be): (usize, usize), m: &mut Module) {
    let txt = |i: usize| -> &str {
        if i < tk.len() {
            &src[tk[i].0..tk[i].1]
        } else {
            ""
        }
    };
    let (mut in_gen, mut cond) = (0usize, Vec::<bool>::new());
    let mut k = bs;
    while k < be {
        let w = txt(k);
        match w {
            "generate" => in_gen += 1,
            "endgenerate" => in_gen = in_gen.saturating_sub(1),
            // shortcut: any `begin` after `)` or `else` counts as conditional (also true of always blocks); upgrade with a real scope parser
            "begin" => cond.push(matches!(txt(k.wrapping_sub(1)), ")" | "else") && k > bs),
            "end" => {
                cond.pop();
            }
            _ => {}
        }
        let prev = if k > bs { txt(k - 1) } else { ";" };
        let labelled = k >= bs + 3 && txt(k - 2) == ":" && matches!(txt(k - 3), "begin" | "end");
        // `)` / `else` cover bodiless `if (..) sub u(..);`; the end-keywords cover code after a block with no `;`
        let bare = matches!(prev, ")" | "else");
        let start = bare
            || labelled
            || matches!(
                prev,
                ";" | "begin"
                    | "end"
                    | "generate"
                    | "endgenerate"
                    | "endfunction"
                    | "endtask"
                    | "endcase"
            );
        if !(start && is_ident(w) && !KEYWORDS.contains(&w)) {
            k += 1;
            continue;
        }
        if PRIMITIVES.contains(&w) {
            if is_ident(txt(k + 1)) || txt(k + 1) == "(" {
                m.warnings.push(format!("gate primitive '{w}' not drawn"));
            }
            k += 1;
            continue;
        }
        // module_type [#(...)] inst [dims] (conns) [, inst2 (conns)] ;
        let mut p = k + 1;
        if txt(p) == "#" && txt(p + 1) == "(" {
            p = paren(src, tk, p + 1).1;
        }
        let mut found = false;
        loop {
            let label = txt(p);
            if !(p < be && is_ident(label) && !KEYWORDS.contains(&label)) {
                break;
            }
            let mut q = p + 1;
            while txt(q) == "[" {
                while q < be && txt(q) != "]" {
                    q += 1;
                }
                q += 1;
            }
            if txt(q) != "(" {
                break;
            }
            let (inner, nq) = paren(src, tk, q);
            let Some(inner) = inner else { break };
            if in_gen > 0 || bare || cond.iter().any(|&c| c) {
                let msg = format!(
                    "instance {label} is inside a generate/conditional block; listed once, not expanded"
                );
                m.warnings.push(msg);
            }
            m.raws.push(Raw {
                label: label.to_string(),
                module: w.to_string(),
                conns: split_top_level(inner, ',')
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            });
            found = true;
            p = nq;
            if txt(p) == "," {
                p += 1;
                continue;
            }
            break;
        }
        k = if found { p } else { k + 1 };
    }
}

fn parse_modules(src: &str) -> Vec<Module> {
    let tk = tokens(src);
    let txt = |i: usize| -> &str {
        if i < tk.len() {
            &src[tk[i].0..tk[i].1]
        } else {
            ""
        }
    };
    let (dir_decl, sig_decl) = (
        Regex::new(r"(?s)\b(input|output|inout)\b([^;]*);").unwrap(),
        Regex::new(r"(?s)\b(wire|logic|reg)\b([^;]*);").unwrap(),
    );
    let range_re = Regex::new(r"\[[^\]]*\]").unwrap();
    let mut mods = Vec::new();
    let mut i = 0;
    while i < tk.len() {
        if !matches!(txt(i), "module" | "macromodule") {
            i += 1;
            continue;
        }
        i += 1;
        while matches!(txt(i), "automatic" | "static") {
            i += 1;
        }
        if i >= tk.len() {
            break;
        }
        let mut m = Module {
            name: txt(i).to_string(),
            ..Default::default()
        };
        i += 1;
        if txt(i) == "#" && txt(i + 1) == "(" {
            i = paren(src, &tk, i + 1).1;
        }
        let mut ansi = true;
        if txt(i) == "(" {
            let (inner, ni) = paren(src, &tk, i);
            match inner {
                Some(h) => (m.ports, ansi) = parse_header(h, &mut m.warnings),
                None => m
                    .warnings
                    .push(format!("module {}: unbalanced port list", m.name)),
            }
            i = ni;
        }
        while i < tk.len() && txt(i) != ";" {
            i += 1;
        }
        let bs = (i + 1).min(tk.len());
        let mut be = bs;
        while be < tk.len() && txt(be) != "endmodule" {
            be += 1;
        }
        if be >= tk.len() && !m.warnings.iter().any(|w| w.contains("unbalanced")) {
            m.warnings
                .push(format!("module {}: missing endmodule", m.name));
        }
        let body = if bs < be {
            &src[tk[bs].0..tk[be.min(tk.len() - 1)].0]
        } else {
            ""
        };
        if !ansi {
            for c in dir_decl.captures_iter(body) {
                for e in split_top_level(&c[2], ',') {
                    if let Some(n) = last_ident(e)
                        && let Some(p) = m.ports.iter_mut().find(|p| p.name == n)
                    {
                        p.direction = dir_of(&c[1]);
                    }
                }
            }
        }
        for c in sig_decl.captures_iter(body) {
            let range = range_re
                .find(&c[2])
                .map(|r| format!(" {}", r.as_str()))
                .unwrap_or_default();
            for e in split_top_level(&c[2], ',') {
                if let Some(n) = last_ident(e) {
                    m.signals.insert(n, format!("{}{}", &c[1], range));
                }
            }
        }
        parse_body(src, &tk, (bs, be), &mut m);
        mods.push(m);
        i = be + 1;
    }
    mods
}

#[cfg(test)]
pub fn parse_sv(text: &str) -> Design {
    parse_sv_top(text, None)
}

/// Parse with module `want` as the top (default: the first module nobody instantiates).
pub fn parse_sv_top(text: &str, want: Option<&str>) -> Design {
    let mut d = Design::default();
    let stripped = strip_comments_sv(text);
    let directive = Regex::new(r"(?m)^[ \t]*`[^\n]*").unwrap();
    if directive.is_match(&stripped) {
        d.warnings
            .push("preprocessor directives ignored; macros are not expanded".into());
    }
    let src = directive.replace_all(&stripped, "").into_owned();
    let mods = parse_modules(&src);
    if mods.is_empty() {
        d.warnings.push("no module found".into());
        return d;
    }
    d.entities = mods
        .iter()
        .map(|m| (m.name.to_ascii_lowercase(), m.ports.clone()))
        .collect();
    let instantiated = |n: &str| mods.iter().any(|m| m.raws.iter().any(|r| r.module == n));
    let tops: Vec<&Module> = mods.iter().filter(|m| !instantiated(&m.name)).collect();
    let named = want.and_then(|w| mods.iter().find(|m| m.name.eq_ignore_ascii_case(w)));
    if let (Some(w), None) = (want, named) {
        d.warnings
            .push(format!("module {w} not found in this file"));
    }
    let top = named.or(tops.first().copied()).unwrap_or(&mods[0]);
    if want.is_none() && tops.len() > 1 {
        d.warnings.push(format!(
            "{} modules are never instantiated; using {}",
            tops.len(),
            top.name
        ));
    }
    d.warnings.extend(top.warnings.iter().cloned());
    d.top_name = top.name.clone();
    d.top_ports = top.ports.clone();
    d.signals = top.signals.clone();
    for raw in &top.raws {
        let known = mods.iter().find(|m| m.name == raw.module);
        let (mut ports, mut positional, mut acts) = (Vec::new(), false, Vec::new());
        for (idx, c) in raw.conns.iter().enumerate() {
            let c = c.trim();
            if c.is_empty() {
                continue;
            }
            let dir_for = |name: &str| {
                known
                    .and_then(|m| m.ports.iter().find(|p| p.name == name))
                    .map_or_else(|| guess_direction(name), |p| p.direction)
            };
            if c.starts_with(".*") {
                d.warnings.push(format!(
                    "instance {} uses .* wildcard connections; not expanded",
                    raw.label
                ));
            } else if let Some(rest) = c.strip_prefix('.') {
                let name: String = rest
                    .chars()
                    .take_while(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '$')
                    .collect();
                let after = rest[name.len()..].trim();
                let sig = if after.starts_with('(') {
                    balanced_parens(after, 0).map(squash).unwrap_or_default()
                } else {
                    name.clone() // implicit `.name` connects to the same-named signal
                };
                ports.push(Port::new(&name, dir_for(&name), &sig));
            } else if let Some(p) = known.and_then(|m| m.ports.get(idx)) {
                ports.push(Port::new(&p.name, p.direction, &squash(c)));
            } else {
                positional = true;
                acts.push(squash(c));
            }
        }
        if positional {
            d.warnings.push(format!(
                "positional connections in instance {} skipped: module {} not in this file",
                raw.label, raw.module
            ));
            d.positional.insert(raw.label.clone(), acts);
        }
        d.instances
            .push(Instance::new(&raw.label, &raw.module, ports));
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Direction;

    const SAMPLE: &str = include_str!("../../tests/fixtures/sample_top.sv");

    fn inst<'a>(d: &'a Design, n: &str) -> &'a crate::model::Instance {
        d.instances
            .iter()
            .find(|i| i.name == n)
            .unwrap_or_else(|| panic!("no instance {n}"))
    }

    #[test]
    fn top_is_uninstantiated_module() {
        let d = parse_sv(SAMPLE);
        assert_eq!(d.top_name, "top");
    }

    #[test]
    fn ansi_ports_with_ranges() {
        let d = parse_sv(SAMPLE);
        let got: Vec<_> = d
            .top_ports
            .iter()
            .map(|p| (p.name.as_str(), p.direction))
            .collect();
        assert_eq!(
            got,
            vec![
                ("clk", Direction::In),
                ("din", Direction::In),
                ("din2", Direction::In),
                ("dout", Direction::Out),
                ("io", Direction::InOut)
            ]
        );
    }

    #[test]
    fn non_ansi_ports() {
        let d = parse_sv("module m (a, b, q); input a; input [3:0] b; output reg q; endmodule");
        let got: Vec<_> = d
            .top_ports
            .iter()
            .map(|p| (p.name.as_str(), p.direction))
            .collect();
        assert_eq!(
            got,
            vec![
                ("a", Direction::In),
                ("b", Direction::In),
                ("q", Direction::Out)
            ]
        );
    }

    #[test]
    fn named_instance_connections() {
        let d = parse_sv(SAMPLE);
        let u = inst(&d, "u_named");
        assert_eq!(u.entity, "sub_ni");
        let got: Vec<_> = u
            .ports
            .iter()
            .map(|p| (p.name.as_str(), p.signal.as_str(), p.direction))
            .collect();
        assert_eq!(
            got,
            vec![
                ("a", "clk", Direction::In),
                ("b", "din[3:0]", Direction::In),
                ("q", "ready", Direction::Out)
            ]
        );
        let o = inst(&d, "u_other"); // module not in file: parameter override skipped, heuristic directions
        assert_eq!(o.ports.len(), 2);
    }

    #[test]
    fn positional_maps_by_order() {
        let d = parse_sv(SAMPLE);
        let u = inst(&d, "u_pos");
        let got: Vec<_> = u
            .ports
            .iter()
            .map(|p| (p.name.as_str(), p.signal.as_str()))
            .collect();
        assert_eq!(got, vec![("a", "clk"), ("b", "din[7:4]"), ("q", "s1[0]")]);
    }

    #[test]
    fn positional_without_module_warns() {
        let d = parse_sv("module t (input a); mystery u (a, a); endmodule");
        assert!(d.warnings.iter().any(|w| w.contains("positional")));
    }

    #[test]
    fn unconnected_port_keeps_neighbours() {
        let d = parse_sv(SAMPLE);
        let u = inst(&d, "u_unconn");
        let got: Vec<_> = u
            .ports
            .iter()
            .map(|p| (p.name.as_str(), p.signal.as_str()))
            .collect();
        assert_eq!(got, vec![("a", "clk"), ("b", ""), ("q", "ready")]);
    }

    #[test]
    fn comment_module_ignored() {
        let d = parse_sv(SAMPLE);
        assert!(!d.instances.iter().any(|i| i.entity == "ghost_block"));
        assert!(d.top_ports.iter().all(|p| p.name != "x"));
    }

    #[test]
    fn generate_instance_warns_but_listed() {
        let d = parse_sv(SAMPLE);
        inst(&d, "u_gen");
        assert!(d.warnings.iter().any(|w| w.contains("generate")));
    }

    #[test]
    fn wildcard_warns() {
        let d = parse_sv("module t (input a); mystery u (.*); endmodule");
        assert!(d.warnings.iter().any(|w| w.contains(".*")));
    }

    #[test]
    fn signals_collected() {
        let d = parse_sv(SAMPLE);
        assert!(d.signals.contains_key("s1") && d.signals.contains_key("ready"));
    }

    #[test]
    fn empty_input_warns() {
        let d = parse_sv("");
        assert!(d.instances.is_empty() && d.top_ports.is_empty());
        assert_eq!(d.warnings.len(), 1);
    }

    #[test]
    fn garbage_does_not_panic() {
        let d = parse_sv("module \u{0} ( ( ( endmodule module");
        assert!(!d.warnings.is_empty());
    }

    #[test]
    fn instances_after_unusual_statement_ends_are_kept() {
        let cases = [
            "function int f(int a); return a; endfunction\n sub u1 (.a(x));",
            "task t; x = 1; endtask\n sub u1 (.a(x));",
            "always_comb case (s) 1: y = 0; default: y = 1; endcase\n sub u1 (.a(x));",
            "if (P) sub u1 (.a(x));",
            "for (i = 0; i < 2; i++) sub u1 (.a(x));",
        ];
        for c in cases {
            let src =
                format!("module top (input x); {c} endmodule\nmodule sub (input a); endmodule");
            let d = parse_sv(&src);
            assert!(
                d.instances.iter().any(|i| i.name == "u1"),
                "dropped instance after: {c}"
            );
        }
    }
}
