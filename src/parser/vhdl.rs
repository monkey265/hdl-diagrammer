use super::lex::{balanced_parens, split_top_level, strip_comments_vhdl};
use super::Design;
use crate::model::{Direction, Instance, Port};
use regex::Regex;
use std::collections::HashMap;

/// Collapse runs of whitespace so multi-line expressions compare and print cleanly.
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Token-based fallback when the instantiated entity is not in the file.
fn guess_direction(port: &str) -> Direction {
    let lower = port.to_ascii_lowercase();
    let toks: Vec<&str> = lower.split('_').collect();
    let has = |set: &[&str]| toks.iter().any(|t| set.contains(t));
    if has(&["o", "out", "dout", "result", "do", "q"]) {
        Direction::Out
    } else if has(&["i", "in", "din", "di", "clk", "clock", "rst", "rstn", "aresetn", "reset"]) {
        Direction::In
    } else {
        Direction::InOut
    }
}

fn parse_ports(body: &str) -> Vec<Port> {
    let mut out = Vec::new();
    for decl in split_top_level(body, ';') {
        let Some((names, rest)) = decl.split_once(':') else { continue };
        let first = rest.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        let dir = match first.as_str() {
            "in" => Direction::In,
            "out" | "buffer" => Direction::Out,
            _ => Direction::InOut,
        };
        for n in names.split(',') {
            let n = n.trim();
            if !n.is_empty() {
                out.push(Port::new(n, dir, n));
            }
        }
    }
    out
}

pub fn parse_vhdl(text: &str) -> Design {
    let src = strip_comments_vhdl(text);
    let mut d = Design::default();

    // Entity declarations: name -> ports (first one is the fallback top).
    let ent_re = Regex::new(r"(?i)\bentity\s+(\w+)\s+is\b").unwrap();
    let port_re = Regex::new(r"(?i)\bport\s*\(").unwrap();
    let end_re = Regex::new(r"(?i)\bend\b").unwrap();
    let mut entities: Vec<(String, Vec<Port>)> = Vec::new();
    for m in ent_re.captures_iter(&src) {
        let after = m.get(0).unwrap().end();
        let rest = &src[after..];
        let end_at = end_re.find(rest).map_or(rest.len(), |e| e.start());
        let ports = match port_re.find(rest) {
            Some(p) if p.start() < end_at => {
                let open = after + p.end() - 1;
                balanced_parens(&src, open).map(parse_ports).unwrap_or_default()
            }
            _ => Vec::new(),
        };
        entities.push((m[1].to_string(), ports));
    }
    if entities.is_empty() {
        d.warnings.push("no entity found".into());
    }
    let ent_ports: HashMap<String, &Vec<Port>> =
        entities.iter().map(|(n, p)| (n.to_ascii_lowercase(), p)).collect();

    let sig_re = Regex::new(r"(?is)\bsignal\s+(\w+(?:\s*,\s*\w+)*)\s*:\s*([^;:=]+?)(?::=.*?)?;").unwrap();
    for m in sig_re.captures_iter(&src) {
        for n in m[1].split(',') {
            d.signals.insert(n.trim().to_string(), squash(&m[2]));
        }
    }

    // Instances: anchor on `port map (`, look back to the previous `;` for the header.
    let pm_re = Regex::new(r"(?i)\bport\s+map\s*\(").unwrap();
    let head_re = Regex::new(r"(?is)(\w+)\s*:\s*(?:entity\s+|component\s+|configuration\s+)?([\w.]+)").unwrap();
    for m in pm_re.find_iter(&src) {
        let start = src[..m.start()].rfind(';').map_or(0, |i| i + 1);
        let header = &src[start..m.start()];
        let Some(h) = head_re.captures(header) else {
            d.warnings.push("port map without a recognisable instance label skipped".into());
            continue;
        };
        let (label, ent) = (h[1].to_string(), h[2].rsplit('.').next().unwrap_or("").to_string());
        let Some(body) = balanced_parens(&src, m.end() - 1) else {
            d.warnings.push(format!("unbalanced port map in instance {label}"));
            continue;
        };
        let known = ent_ports.get(&ent.to_ascii_lowercase());
        let mut ports = Vec::new();
        let mut positional = false;
        for assoc in split_top_level(body, ',') {
            if assoc.is_empty() {
                continue;
            }
            let Some((formal, actual)) = assoc.split_once("=>") else {
                positional = true;
                continue;
            };
            let formal = formal.trim();
            let dir = known
                .and_then(|ps| ps.iter().find(|p| p.name.eq_ignore_ascii_case(formal)))
                .map_or_else(|| guess_direction(formal), |p| p.direction);
            ports.push(Port::new(formal, dir, &squash(actual)));
        }
        if positional {
            d.warnings.push(format!("positional associations in instance {label} skipped"));
        }
        d.instances.push(Instance::new(&label, &ent, ports));
    }

    // Top = first entity nobody instantiates, else first entity.
    let top = entities
        .iter()
        .find(|(n, _)| !d.instances.iter().any(|i| i.entity.eq_ignore_ascii_case(n)))
        .or(entities.first());
    if let Some((n, p)) = top {
        d.top_name = n.clone();
        d.top_ports = p.clone();
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    const RADIO: &str = include_str!("../../tests/fixtures/radio_top.vhd");
    const RC4: &str = include_str!("../../tests/fixtures/RC4_sync_top.vhd");

    const SAMPLE: &str = "entity top is port ( clk : in std_logic; a, b : in std_logic_vector(7 downto 0); q : out std_logic; io : inout std_logic ); end entity top;
architecture rtl of top is signal s1, s2 : std_logic; begin
 u1 : entity work.sub generic map (W => 8) port map ( x => a, y => s1, z => open );
end architecture;";

    #[test]
    fn parses_entity_ports() {
        let d = parse_vhdl(SAMPLE);
        assert_eq!(d.top_name, "top");
        let got: Vec<_> = d.top_ports.iter().map(|p| (p.name.as_str(), p.direction)).collect();
        assert_eq!(got, vec![("clk", Direction::In), ("a", Direction::In), ("b", Direction::In), ("q", Direction::Out), ("io", Direction::InOut)]);
        assert!(d.signals.contains_key("s1") && d.signals.contains_key("s2"));
    }

    #[test]
    fn instance_count_radio_top() {
        let d = parse_vhdl(RADIO);
        assert_eq!(d.instances.len(), 11);
        assert_eq!(d.top_ports.len(), 7);
    }

    #[test]
    fn instance_count_rc4() {
        let d = parse_vhdl(RC4);
        assert_eq!(d.instances.len(), 5);
        assert_eq!(d.top_ports.len(), 6);
    }

    #[test]
    fn keyword_in_comment_ignored() {
        let d = parse_vhdl("-- u9 : entity work.ghost port map (a => b);\n");
        assert!(d.instances.is_empty());
    }

    #[test]
    fn mixed_case_keywords() {
        let d = parse_vhdl("ENTITY T Is Port (a : IN std_logic); End T;\nBegin\n u : Entity WORK.m Port Map (p => a);");
        assert_eq!(d.instances.len(), 1);
        assert_eq!(d.instances[0].entity, "m");
    }

    #[test]
    fn open_association_kept() {
        let d = parse_vhdl(SAMPLE);
        let z = d.instances[0].ports.iter().find(|p| p.name == "z").unwrap();
        assert_eq!(z.signal, "open");
        assert_eq!(d.instances[0].ports.len(), 3);
    }

    #[test]
    fn direction_from_entity_in_same_file() {
        let d = parse_vhdl("entity sub is port (x : in bit; y : out bit); end sub;\nentity top is port (a : in bit); end top;\narchitecture r of top is begin u : entity work.sub port map (x => a, y => w); end;");
        assert_eq!(d.top_name, "top");
        assert_eq!(d.instances[0].ports[1].direction, Direction::Out);
    }

    #[test]
    fn positional_association_warns() {
        let d = parse_vhdl("architecture r of t is begin u : entity work.m port map (a, b); end;");
        assert!(d.warnings.iter().any(|w| w.contains("positional")));
    }

    #[test]
    fn empty_input_warns() {
        let d = parse_vhdl("");
        assert!(d.instances.is_empty() && d.top_ports.is_empty());
        assert_eq!(d.warnings.len(), 1);
    }

    #[test]
    fn no_entity_warns() {
        let d = parse_vhdl("library ieee; use ieee.std_logic_1164.all;");
        assert!(d.warnings.iter().any(|w| w.contains("no entity")));
    }
}
