//! Interface bundling, like Vivado's AXI ports: ports sharing a name prefix (`S_AXI_AWADDR`,
//! `S_AXI_WDATA`, ...) collapse into one thick port whose signal is `<common signal prefix>*`.
use crate::model::{Diagram, Direction, Instance, Port};
use std::collections::{BTreeSet, HashMap, HashSet};

/// A prefix must be shared by at least this many ports to count as an interface.
const MIN_MEMBERS: usize = 4;

fn tokens(s: &str) -> Vec<&str> {
    s.split('_').filter(|t| !t.is_empty()).collect()
}

fn live(s: &str) -> bool {
    !s.is_empty() && !s.eq_ignore_ascii_case("open") && !s.ends_with('*')
}

/// Longest shared run of `_` tokens, as text ("" if none).
fn common_prefix(sigs: &[&str]) -> String {
    let toks: Vec<Vec<&str>> = sigs.iter().map(|s| tokens(s)).collect();
    let n = (0..toks[0].len())
        .take_while(|&k| toks.iter().all(|t| t.get(k) == toks[0].get(k)))
        .count();
    // a signal that is entirely the prefix leaves nothing to tell the members apart
    if toks.iter().any(|t| t.len() <= n) {
        return String::new();
    }
    toks[0][..n].join("_")
}

/// One interface found in a port list: which ports fold, and the bundle that replaces them.
struct Plan {
    cp: String,
    members: Vec<usize>,
    bundle: Port,
}

/// Interfaces in `ports`, skipping signal prefixes in `rejected`.
fn plan(ports: &[Port], rejected: &HashSet<String>) -> Vec<Plan> {
    let toks: Vec<Vec<&str>> = ports.iter().map(|p| tokens(&p.name)).collect();
    let prefix = |k: usize, n: usize| toks[k][..n].join("_").to_ascii_lowercase();
    let mut count: HashMap<String, usize> = HashMap::new();
    for (k, t) in toks.iter().enumerate() {
        if live(&ports[k].signal) {
            (1..t.len()).for_each(|n| *count.entry(prefix(k, n)).or_default() += 1);
        }
    }
    // each port joins its longest name prefix that enough ports share
    let mut groups: Vec<(String, usize, Vec<usize>)> = Vec::new();
    for k in 0..ports.len() {
        if !live(&ports[k].signal) {
            continue;
        }
        let best = (1..toks[k].len())
            .rev()
            .map(|n| (prefix(k, n), n))
            .find(|(p, _)| count[p] >= MIN_MEMBERS);
        if let Some((p, n)) = best {
            match groups.iter_mut().find(|g| g.0 == p) {
                Some(g) => g.2.push(k),
                None => groups.push((p, n, vec![k])),
            }
        }
    }
    let mut out = Vec::new();
    for (_, n, members) in groups {
        // clock/reset-like members sit on other signals: keep the largest set that shares a first token
        let first_tok = |k: usize| tokens(&ports[k].signal)[0].to_string();
        let mut by_tok: HashMap<String, Vec<usize>> = HashMap::new();
        members
            .iter()
            .for_each(|&k| by_tok.entry(first_tok(k)).or_default().push(k));
        let Some(members) = by_tok
            .into_values()
            .max_by_key(|v| (v.len(), usize::MAX - v[0]))
        else {
            continue;
        };
        let sigs: Vec<&str> = members.iter().map(|&k| ports[k].signal.as_str()).collect();
        let distinct = sigs.iter().collect::<BTreeSet<_>>().len();
        let cp = common_prefix(&sigs);
        if distinct < MIN_MEMBERS || cp.is_empty() || rejected.contains(&cp) {
            continue;
        }
        let first = &ports[members[0]];
        let dir = if members
            .iter()
            .all(|&k| ports[k].direction == first.direction)
        {
            first.direction
        } else {
            Direction::InOut
        };
        let bundle = Port {
            name: format!("{} ({})", tokens(&first.name)[..n].join("_"), members.len()),
            direction: dir,
            signal: format!("{cp}*"),
            ..first.clone()
        };
        out.push(Plan {
            cp,
            members,
            bundle,
        });
    }
    out
}

fn apply(ports: &[Port], plans: Vec<Plan>) -> Vec<Port> {
    let mut at: HashMap<usize, Port> = HashMap::new();
    let mut folded = vec![false; ports.len()];
    for p in plans {
        p.members.iter().for_each(|&k| folded[k] = true);
        at.insert(p.members[0], p.bundle);
    }
    (0..ports.len())
        .filter_map(|k| {
            at.remove(&k)
                .or_else(|| (!folded[k]).then(|| ports[k].clone()))
        })
        .collect()
}

fn each_block(list: &mut [Instance], f: &mut dyn FnMut(&mut Instance)) {
    for i in list {
        f(i);
        each_block(&mut i.children, f);
    }
}

fn is_bundled(i: &Instance) -> bool {
    i.ports.iter().any(|p| p.signal.ends_with('*'))
}

/// Fold interfaces on every block and top pin, then resize the blocks. Returns how many folded.
/// A bundle is only made when no loose port anywhere still uses its signal prefix: otherwise
/// those wires would silently lose their other end.
pub fn collapse_all(d: &mut Diagram, grid: i32) -> usize {
    let top_free = d.original_top_pins.is_empty();
    let mut rejected: HashSet<String> = HashSet::new();
    loop {
        let mut lists: Vec<&[Port]> = Vec::new();
        fn collect<'a>(v: &'a [Instance], out: &mut Vec<&'a [Port]>) {
            for i in v {
                if !i.is_group && !is_bundled(i) {
                    out.push(&i.ports);
                }
                collect(&i.children, out);
            }
        }
        collect(&d.instances, &mut lists);
        if top_free {
            lists.push(&d.top_level_pins);
        }
        let plans: Vec<Vec<Plan>> = lists.iter().map(|l| plan(l, &rejected)).collect();
        let mut loose: Vec<&str> = Vec::new();
        for (l, ps) in lists.iter().zip(&plans) {
            let folded: HashSet<usize> =
                ps.iter().flat_map(|p| p.members.iter().copied()).collect();
            loose.extend(
                l.iter()
                    .enumerate()
                    .filter(|(k, p)| !folded.contains(k) && live(&p.signal))
                    .map(|(_, p)| p.signal.as_str()),
            );
        }
        let bad: Vec<String> = plans
            .iter()
            .flatten()
            .filter(|p| {
                let pre = format!("{}_", p.cp);
                loose.iter().any(|s| s.starts_with(&pre))
            })
            .map(|p| p.cp.clone())
            .collect();
        if bad.is_empty() {
            break;
        }
        rejected.extend(bad);
    }
    let mut n = 0;
    each_block(&mut d.instances, &mut |i| {
        if i.is_group || is_bundled(i) {
            return;
        }
        let ps = plan(&i.ports, &rejected);
        if !ps.is_empty() {
            (i.ports, n) = (apply(&i.ports, ps), n + 1);
            (i.width, i.height) = crate::layout::block_size(i, grid);
        }
    });
    if top_free {
        let ps = plan(&d.top_level_pins, &rejected);
        if !ps.is_empty() {
            let new = apply(&d.top_level_pins, ps);
            d.original_top_pins = std::mem::replace(&mut d.top_level_pins, new);
            n += 1;
        }
    }
    n
}

/// Undo `collapse_all`: blocks get their original ports back, top pins likewise.
pub fn expand_all(d: &mut Diagram, grid: i32) {
    each_block(&mut d.instances, &mut |i| {
        if is_bundled(i) {
            i.ports = i.original_ports.clone();
            (i.width, i.height) = crate::layout::block_size(i, grid);
        }
    });
    if !d.original_top_pins.is_empty() {
        d.top_level_pins = std::mem::take(&mut d.original_top_pins);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collapse_ports(ports: &[Port]) -> Option<Vec<Port>> {
        let ps = plan(ports, &HashSet::new());
        (!ps.is_empty()).then(|| apply(ports, ps))
    }

    fn axi(n: usize, sig_prefix: &str) -> Vec<Port> {
        let mut v = vec![Port::new("clk", Direction::In, "clk")];
        for k in 0..n {
            let dir = if k % 2 == 0 {
                Direction::In
            } else {
                Direction::Out
            };
            v.push(Port::new(
                &format!("S_AXI_SIG{k}"),
                dir,
                &format!("{sig_prefix}_sig{k}"),
            ));
        }
        v
    }

    #[test]
    fn axi_ports_fold_into_one_bundle_and_others_stay() {
        let out = collapse_ports(&axi(6, "bus")).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "clk");
        assert_eq!(out[1].name, "S_AXI (6)");
        assert_eq!(out[1].signal, "bus*");
        assert_eq!(out[1].direction, Direction::InOut); // mixed directions
    }

    #[test]
    fn too_few_members_or_unrelated_signals_do_not_fold() {
        assert!(collapse_ports(&axi(3, "bus")).is_none());
        let mut p = axi(6, "bus");
        p.iter_mut()
            .enumerate()
            .for_each(|(k, p)| p.signal = format!("net{k}"));
        assert!(collapse_ports(&p).is_none());
    }

    #[test]
    fn both_ends_agree_and_expand_restores_everything() {
        let mk = |n: &str| Instance::new(n, "E", axi(5, "bus"));
        let mut d = Diagram {
            instances: vec![mk("a"), mk("b")],
            top_level_pins: axi(5, "bus"),
            ..Default::default()
        };
        let before = d.clone();
        assert_eq!(collapse_all(&mut d, 20), 3);
        assert_eq!(d.instances[0].ports[1].signal, "bus*");
        assert_eq!(d.top_level_pins[1].signal, "bus*");
        assert_eq!(
            collapse_all(&mut d, 20),
            0,
            "second collapse changes nothing"
        );
        expand_all(&mut d, 20);
        assert_eq!(d.instances[0].ports, before.instances[0].ports);
        assert_eq!(d.top_level_pins, before.top_level_pins);
    }

    #[test]
    fn clock_on_another_signal_does_not_block_the_rest_of_the_interface() {
        let mut p = axi(6, "bus");
        p[1].signal = "inputs.clk".into(); // one member on an unrelated signal
        let out = collapse_ports(&p).unwrap();
        assert!(
            out.iter()
                .any(|x| x.name == "S_AXI (5)" && x.signal == "bus*")
        );
        assert!(out.iter().any(|x| x.signal == "inputs.clk"));
    }

    #[test]
    fn nothing_folds_if_a_loose_port_would_lose_its_partner() {
        // `a` has an interface on bus_*, but `b` has only two ports on it: they would vanish
        let b = vec![
            Port::new("x", Direction::In, "bus_sig0"),
            Port::new("y", Direction::In, "bus_sig1"),
        ];
        let mut d = Diagram {
            instances: vec![
                Instance::new("a", "E", axi(5, "bus")),
                Instance::new("b", "E", b),
            ],
            ..Default::default()
        };
        assert_eq!(collapse_all(&mut d, 20), 0);
        assert!(
            d.instances[0]
                .ports
                .iter()
                .all(|p| !p.signal.ends_with('*'))
        );
    }
}
