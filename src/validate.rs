//! "Validate design": unconnected inputs, undriven nets, multiply-driven nets, dangling nets.
use crate::group::active_instances;
use crate::model::{Diagram, Direction};
use std::collections::BTreeMap;

fn live(s: &str) -> bool {
    !s.is_empty() && !s.eq_ignore_ascii_case("open")
}

/// Plain identifiers only; constants, slices and expressions are not nets we can judge.
fn plain(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

pub fn validate(d: &Diagram) -> Vec<String> {
    let mut out = Vec::new();
    // net -> (drivers, loads, strict-Out drivers, endpoints)
    let mut nets: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    let mut add = |sig: &str, dir: Direction, top: bool| {
        if !live(sig) || !plain(sig) {
            return;
        }
        let e = nets.entry(sig.to_string()).or_default();
        // a top-level input drives the net, a top-level output loads it
        let dir = match (dir, top) {
            (Direction::In, true) => Direction::Out,
            (Direction::Out, true) => Direction::In,
            (d, _) => d,
        };
        e[3] += 1;
        match dir {
            Direction::Out => {
                e[0] += 1;
                e[2] += 1;
            }
            Direction::In => e[1] += 1,
            Direction::InOut => {
                e[0] += 1;
                e[1] += 1;
            }
        }
    };
    for i in active_instances(d)
        .into_iter()
        .filter(|i| !i.is_group || i.collapsed)
    {
        for p in &i.ports {
            if p.direction == Direction::In && !live(&p.signal) {
                out.push(format!("{}.{}: input is not connected", i.name, p.name));
            }
            add(&p.signal, p.direction, false);
        }
    }
    for p in &d.top_level_pins {
        add(&p.signal, p.direction, true);
    }
    for (sig, [drivers, loads, outs, ends]) in nets {
        if loads > 0 && drivers == 0 {
            out.push(format!("net {sig}: no driver"));
        } else if outs > 1 {
            out.push(format!("net {sig}: {outs} drivers"));
        } else if ends == 1 {
            out.push(format!("net {sig}: connects to one pin only"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Instance, Port};

    #[test]
    fn finds_each_kind_of_problem_and_stays_quiet_on_a_clean_net() {
        let mk = |n: &str, ports: Vec<Port>| Instance::new(n, "E", ports);
        let d = Diagram {
            instances: vec![
                mk(
                    "a",
                    vec![
                        Port::new("o", Direction::Out, "ok"),
                        Port::new("o2", Direction::Out, "twice"),
                        Port::new("i", Direction::In, "floating"),
                        Port::new("nc", Direction::In, ""),
                        Port::new("k", Direction::In, "'0'"),
                    ],
                ),
                mk(
                    "b",
                    vec![
                        Port::new("i", Direction::In, "ok"),
                        Port::new("o", Direction::Out, "twice"),
                        Port::new("lonely", Direction::Out, "alone"),
                    ],
                ),
            ],
            top_level_pins: vec![Port::new("q", Direction::Out, "twice")],
            ..Default::default()
        };
        let r = validate(&d);
        assert!(r.contains(&"a.nc: input is not connected".to_string()));
        assert!(r.contains(&"net floating: no driver".to_string()));
        assert!(r.contains(&"net twice: 2 drivers".to_string()));
        assert!(r.contains(&"net alone: connects to one pin only".to_string()));
        assert!(!r.iter().any(|m| m.contains("net ok") || m.contains("'0'")));
        assert_eq!(r.len(), 4, "{r:?}");
    }
}
