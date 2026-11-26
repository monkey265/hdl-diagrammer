//! Expand `for ... generate` loops with literal bounds by repeating the body text, so each copy
//! of an instance shows up as its own block. `if`/`case` generates and symbolic bounds stay as-is.
use super::Design;
use regex::Regex;
use std::collections::HashMap;

const MAX_COPIES: i64 = 256;

/// Replace the whole word `var` by `val`.
fn subst(body: &str, var: &str, val: &str) -> String {
    let re = Regex::new(&format!(r"\b{}\b", regex::escape(var))).unwrap();
    re.replace_all(body, val).into_owned()
}

fn values(from: i64, to: i64, step: i64) -> Option<Vec<i64>> {
    let n = if step > 0 { to - from } else { from - to } + 1;
    (n <= MAX_COPIES).then(|| (0..n.max(0)).map(|k| from + k * step).collect())
}

/// Text after a match start with nesting: the end of the block opened just before `from`.
/// `open`/`close` are regex alternatives; returns (start of closer, end of closer).
fn block_end(text: &str, from: usize, open: &Regex, close: &Regex) -> Option<(usize, usize)> {
    let mut depth = 1;
    let mut at = from;
    loop {
        let o = open.find_at(text, at);
        let c = close.find_at(text, at)?;
        match o {
            Some(o) if o.start() < c.start() => {
                depth += 1;
                at = o.end();
            }
            _ => {
                depth -= 1;
                if depth == 0 {
                    return Some((c.start(), c.end()));
                }
                at = c.end();
            }
        }
    }
}

/// Expand every simple VHDL for-generate; returns the new text and the loop value lists used.
pub fn expand_vhdl(src: &str) -> (String, Vec<Vec<String>>) {
    let head =
        Regex::new(r"(?i)\b\w+\s*:\s*for\s+(\w+)\s+in\s+(\d+)\s+(to|downto)\s+(\d+)\s+generate\b")
            .unwrap();
    // `end generate` closes; a bare `generate` (not preceded by end) opens
    let close = Regex::new(r"(?i)\bend\s+generate\b").unwrap();
    // an `end generate` also contains the word `generate`: only count real openers
    let open = Regex::new(r"(?i)\b(?:for|if|case)\b[^;]*?\bgenerate\b").unwrap();
    let (mut text, mut lists, mut from) = (src.to_string(), Vec::new(), 0);
    for _ in 0..64 {
        let Some(c) = head.captures_at(&text, from) else {
            break;
        };
        let m = c.get(0).unwrap();
        let (var, a, dir, b) = (
            c[1].to_string(),
            c[2].parse::<i64>(),
            c[3].to_ascii_lowercase(),
            c[4].parse::<i64>(),
        );
        let (Ok(a), Ok(b)) = (a, b) else {
            from = m.end();
            continue;
        };
        let Some((cs, ce)) = block_end(&text, m.end(), &open, &close) else {
            from = m.end();
            continue;
        };
        let Some(vals) = values(a, b, if dir == "to" { 1 } else { -1 }) else {
            from = m.end();
            continue;
        };
        let after = text[ce..].find(';').map_or(text.len(), |p| ce + p + 1);
        let body = &text[m.end()..cs];
        let copies: Vec<String> = vals
            .iter()
            .map(|v| subst(body, &var, &v.to_string()))
            .collect();
        lists.push(vals.iter().map(i64::to_string).collect());
        let start = m.start();
        text = format!(
            "{}{}{}",
            &text[..m.start()],
            copies.join("\n"),
            &text[after..]
        );
        from = start;
    }
    (text, lists)
}

/// Expand simple SystemVerilog `for (genvar i = A; i < B; i = i + 1) begin ... end` loops.
pub fn expand_sv(src: &str) -> (String, Vec<Vec<String>>) {
    let head = Regex::new(
        r"for\s*\(\s*(?:genvar\s+)?(\w+)\s*=\s*(\d+)\s*;\s*(\w+)\s*(<=|<)\s*(\d+)\s*;\s*(\w+)\s*(?:\+\+|=\s*(\w+)\s*\+\s*1|\+=\s*1)\s*\)\s*begin(?:\s*:\s*\w+)?",
    )
    .unwrap();
    let open = Regex::new(r"\bbegin\b").unwrap();
    let close = Regex::new(r"\bend\b").unwrap();
    let (mut text, mut lists, mut from) = (src.to_string(), Vec::new(), 0);
    for _ in 0..64 {
        let Some(c) = head.captures_at(&text, from) else {
            break;
        };
        let m = c.get(0).unwrap();
        let var = c[1].to_string();
        let simple = c[3] == var && c[6] == var && c.get(7).is_none_or(|x| x.as_str() == var);
        let (a, b) = (c[2].parse::<i64>(), c[5].parse::<i64>());
        let (true, Ok(a), Ok(mut b)) = (simple, a, b) else {
            from = m.end();
            continue;
        };
        if &c[4] == "<" {
            b -= 1;
        }
        let (Some((cs, ce)), Some(vals)) =
            (block_end(&text, m.end(), &open, &close), values(a, b, 1))
        else {
            from = m.end();
            continue;
        };
        let body = &text[m.end()..cs];
        let copies: Vec<String> = vals
            .iter()
            .map(|v| subst(body, &var, &v.to_string()))
            .collect();
        lists.push(vals.iter().map(i64::to_string).collect());
        let start = m.start();
        text = format!("{}{}{}", &text[..start], copies.join("\n"), &text[ce..]);
        from = start;
    }
    if !lists.is_empty() {
        // the keywords are optional, and left in they would make every instance "inside a generate"
        let kw = Regex::new(r"\b(?:generate|endgenerate)\b").unwrap();
        text = kw.replace_all(&text, "").into_owned();
    }
    (text, lists)
}

/// Repeated instance labels came from loop copies: number them `label(v)` or `label[v]`.
pub fn rename_copies(d: &mut Design, lists: &[Vec<String>], open: char, close: char) {
    if lists.is_empty() {
        return;
    }
    let mut total: HashMap<String, usize> = HashMap::new();
    d.instances
        .iter()
        .for_each(|i| *total.entry(i.name.clone()).or_default() += 1);
    let mut seen: HashMap<String, usize> = HashMap::new();
    for inst in &mut d.instances {
        let n = total[&inst.name];
        if n < 2 {
            continue;
        }
        let k = seen.entry(inst.name.clone()).or_default();
        // loop values when exactly one loop has this many copies, else just 0, 1, 2...
        let mut cands = lists.iter().filter(|l| l.len() == n);
        let val = match (cands.next(), cands.next()) {
            (Some(l), None) => l[*k].clone(),
            _ => k.to_string(),
        };
        *k += 1;
        let new = format!("{}{open}{val}{close}", inst.name);
        let old = std::mem::replace(&mut inst.name, new);
        if let Some(acts) = d.positional.get(&old).cloned() {
            d.positional.insert(inst.name.clone(), acts); // approximate: the last copy's actuals
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::{sv::parse_sv, vhdl::parse_vhdl};

    #[test]
    fn vhdl_for_generate_expands_with_loop_values() {
        let d = parse_vhdl(
            "entity t is port (a : in std_logic_vector(3 downto 0); q : out std_logic_vector(3 downto 0)); end entity;
architecture x of t is begin
 g : for i in 0 to 3 generate
   u : entity work.f port map (i => a(i), o => q(i));
 end generate g;
 w : entity work.single port map (i => a(0), o => q(0));
end;",
        );
        let names: Vec<_> = d.instances.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["u(0)", "u(1)", "u(2)", "u(3)", "w"]);
        assert_eq!(d.instances[2].ports[0].signal, "a(2)");
        assert!(
            d.warnings.iter().all(|w| !w.contains("generate")),
            "{:?}",
            d.warnings
        );
    }

    #[test]
    fn vhdl_downto_and_symbolic_bounds() {
        let d = parse_vhdl(
            "entity t is port (a : in std_logic); end entity;
architecture x of t is begin
 g : for i in 2 downto 1 generate u : entity work.f port map (i => a); end generate;
 h : for j in 0 to N generate v : entity work.f port map (i => a); end generate;
end;",
        );
        let names: Vec<_> = d.instances.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["u(2)", "u(1)", "v"]); // N is symbolic: v stays once, with the old warning
        assert!(d.warnings.iter().any(|w| w.contains("generate")));
    }

    #[test]
    fn sv_for_generate_expands() {
        let d = parse_sv(
            "module top(input [3:0] a, output [3:0] q);
 genvar i;
 generate for (i = 0; i < 4; i = i + 1) begin : g
   sub u (.i(a[i]), .o(q[i]));
 end endgenerate
endmodule",
        );
        let names: Vec<_> = d.instances.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["u[0]", "u[1]", "u[2]", "u[3]"]);
        assert_eq!(d.instances[3].ports[0].signal, "a[3]");
    }
}
