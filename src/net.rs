use crate::config::GRID_STEP;
use crate::geometry::{Pt, Seg, junctions, mark_segment, snap};
use crate::group::active_instances;
use crate::layout::{arrange_grid, port_pos};
use crate::model::{Diagram, Direction};
use crate::parser::Design;
use crate::router::{Bounds, Router, WireOcc};
use std::collections::{HashMap, HashSet};

/// One end of a wire: an instance port or a top-level pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ep {
    Inst { inst: usize, port: usize },
    Top(usize),
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // src/dst identify the net ends for upcoming per-wire actions; tests read them
pub struct Wire {
    pub signal: String,
    pub src: Ep,
    pub dst: Ep,
    pub segments: Vec<Seg>,
}

/// Build a laid-out diagram (block sizes and positions) from a parsed design.
pub fn design_to_diagram(d: &Design, grid: i32) -> Diagram {
    let mut diagram = Diagram {
        instances: d.instances.clone(),
        top_level_pins: d.top_ports.clone(),
    };
    arrange_grid(&mut diagram.instances, grid);
    diagram
}

fn bbox(d: &Diagram) -> Option<(i32, i32, i32, i32)> {
    let vis = d.instances.iter().filter(|i| i.visible);
    vis.fold(None, |acc, i| {
        let (x0, y0, x1, y1) = (i.x, i.y, i.x + i.width, i.y + i.height);
        Some(acc.map_or((x0, y0, x1, y1), |(a, b, c, e)| {
            (a.min(x0), b.min(y0), c.max(x1), e.max(y1))
        }))
    })
}

/// Positions of `diagram.top_level_pins`, aligned by index: IN left of the blocks, OUT/INOUT right.
pub fn top_pin_positions(d: &Diagram) -> Vec<Pt> {
    let (x0, y0, x1, _) = bbox(d).unwrap_or((0, 0, 0, 0));
    let (mut n_in, mut n_out) = (0, 0);
    d.top_level_pins
        .iter()
        .map(|p| {
            let k = if p.direction == Direction::In {
                &mut n_in
            } else {
                &mut n_out
            };
            let y = snap(y0 + 40 + *k * 40, GRID_STEP);
            *k += 1;
            if p.direction == Direction::In {
                (snap(x0 - 100, GRID_STEP), y)
            } else {
                (snap(x1 + 100, GRID_STEP), y)
            }
        })
        .collect()
}

/// Route every net (signal with >= 2 endpoints) from its driver to each other endpoint.
pub fn route_all(d: &Diagram) -> Vec<Wire> {
    let pins = top_pin_positions(d);
    // (endpoint, position, drives the net?) grouped by signal, in first-seen order
    let mut order: Vec<String> = Vec::new();
    let mut nets: HashMap<String, Vec<(Ep, Pt, bool)>> = HashMap::new();
    let mut add = |sig: &str, ep: Ep, pos: Pt, drives: bool| {
        if sig.is_empty() || sig.eq_ignore_ascii_case("open") {
            return;
        }
        nets.entry(sig.to_string())
            .or_insert_with(|| {
                order.push(sig.to_string());
                Vec::new()
            })
            .push((ep, pos, drives));
    };
    for (k, p) in d.top_level_pins.iter().enumerate() {
        add(&p.signal, Ep::Top(k), pins[k], p.direction == Direction::In);
    }
    let active = active_instances(d);
    for (ii, inst) in active.iter().enumerate() {
        for (pi, p) in inst.ports.iter().enumerate() {
            add(
                &p.signal,
                Ep::Inst { inst: ii, port: pi },
                port_pos(inst, pi),
                p.direction != Direction::In,
            );
        }
    }
    let routed: Vec<&String> = order.iter().filter(|s| nets[*s].len() >= 2).collect();
    let Some((x0, y0, x1, y1)) = bbox(d) else {
        return Vec::new();
    };
    if routed.is_empty() {
        return Vec::new();
    }
    let all_pts = routed.iter().flat_map(|s| nets[*s].iter().map(|n| n.1));
    let (mut bx0, mut by0, mut bx1, mut by1) = (x0, y0, x1, y1);
    for (x, y) in all_pts {
        (bx0, by0, bx1, by1) = (bx0.min(x), by0.min(y), bx1.max(x), by1.max(y));
    }
    let floor = |v: i32| v.div_euclid(GRID_STEP) * GRID_STEP;
    let bounds: Bounds = (
        floor(bx0 - 200),
        floor(bx1 + 200),
        floor(by0 - 150),
        floor(by1 + 150),
    );
    let router = Router {
        grid_step: GRID_STEP,
    };
    // an expanded group is only a frame: wires may cross it to reach its children
    let blocks: Vec<_> = active
        .iter()
        .filter(|i| !i.is_group || i.collapsed)
        .map(|i| (i.x, i.y, i.width, i.height))
        .collect();
    let occ = router.build_occupancy(&blocks, bounds, 30);
    let mut wire_occ = WireOcc::new();
    let mut wires = Vec::new();
    for sig in routed {
        let eps = &nets[sig];
        let src = eps.iter().find(|e| e.2).unwrap_or(&eps[0]);
        for dst in eps.iter().filter(|e| e.0 != src.0) {
            let poly = router.route_net(src.1, dst.1, &occ, &wire_occ, sig, bounds);
            let segments: Vec<Seg> = poly.windows(2).map(|w| (w[0], w[1])).collect();
            segments
                .iter()
                .for_each(|&s| mark_segment(s, sig, GRID_STEP, &mut wire_occ));
            wires.push(Wire {
                signal: sig.clone(),
                src: src.0,
                dst: dst.0,
                segments,
            });
        }
    }
    wires
}

/// Junction dots for a routed diagram (per-signal T-splits, not on pins).
pub fn wire_junctions(d: &Diagram, wires: &[Wire]) -> Vec<(String, Pt)> {
    let mut by_sig: Vec<(String, Vec<Seg>)> = Vec::new();
    for w in wires {
        match by_sig.iter_mut().find(|(s, _)| *s == w.signal) {
            Some((_, v)) => v.extend(&w.segments),
            None => by_sig.push((w.signal.clone(), w.segments.clone())),
        }
    }
    let mut ports: HashSet<Pt> = top_pin_positions(d).into_iter().collect();
    for inst in active_instances(d) {
        ports.extend((0..inst.ports.len()).map(|k| port_pos(inst, k)));
    }
    junctions(&by_sig, &ports, GRID_STEP)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Instance, Port};

    fn two_blocks() -> Diagram {
        let a = Instance::new(
            "a",
            "A",
            vec![
                Port::new("o", Direction::Out, "n1"),
                Port::new("lonely", Direction::Out, "n_solo"),
            ],
        );
        let b = Instance::new(
            "b",
            "B",
            vec![
                Port::new("i", Direction::In, "n1"),
                Port::new("z", Direction::In, "open"),
            ],
        );
        let mut d = Diagram {
            instances: vec![a, b],
            top_level_pins: vec![],
        };
        crate::layout::arrange_grid(&mut d.instances, 20);
        d
    }

    #[test]
    fn route_all_connects_shared_signal() {
        let w = route_all(&two_blocks());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].signal, "n1");
        assert_eq!(w[0].src, Ep::Inst { inst: 0, port: 0 });
        assert_eq!(w[0].dst, Ep::Inst { inst: 1, port: 0 });
        assert!(!w[0].segments.is_empty());
        // every segment is axis-aligned
        assert!(
            w[0].segments
                .iter()
                .all(|((x1, y1), (x2, y2))| x1 == x2 || y1 == y2)
        );
    }

    #[test]
    fn route_all_skips_single_port_and_open_signals() {
        let w = route_all(&two_blocks());
        assert!(w.iter().all(|w| w.signal != "n_solo" && w.signal != "open"));
    }

    #[test]
    fn top_pin_joins_net_as_driver() {
        let mut d = two_blocks();
        d.instances[0].ports[0].signal = "x".into();
        d.instances[1].ports[0].signal = "x".into();
        d.top_level_pins.push(Port::new("x", Direction::In, "x"));
        let w = route_all(&d);
        assert_eq!(w.len(), 2);
        assert!(w.iter().all(|w| w.src == Ep::Top(0)));
    }

    #[test]
    fn hidden_instances_are_not_routed() {
        let mut d = two_blocks();
        d.instances[1].visible = false;
        assert!(route_all(&d).is_empty());
    }

    #[test]
    fn design_to_diagram_lays_out_and_keeps_original_ports() {
        let des =
            crate::parser::vhdl::parse_vhdl(include_str!("../tests/fixtures/RC4_sync_top.vhd"));
        let d = design_to_diagram(&des, 20);
        assert_eq!(d.instances.len(), 5);
        assert_eq!(d.top_level_pins.len(), 6);
        assert!(
            d.instances
                .iter()
                .all(|i| i.width > 0 && i.height > 0 && i.original_ports == i.ports)
        );
    }

    #[test]
    fn empty_diagram_routes_nothing() {
        assert!(route_all(&Diagram::default()).is_empty());
        assert!(top_pin_positions(&Diagram::default()).is_empty());
    }

    #[test]
    fn radio_top_routes_every_wire_axis_aligned() {
        let des = crate::parser::vhdl::parse_vhdl(include_str!("../tests/fixtures/radio_top.vhd"));
        let w = route_all(&design_to_diagram(&des, 20));
        assert!(w.len() > 20);
        assert!(
            w.iter()
                .flat_map(|w| &w.segments)
                .all(|((x1, y1), (x2, y2))| x1 == x2 || y1 == y2)
        );
    }

    fn grouped(collapsed: bool) -> Diagram {
        let mk = |n: &str, x: i32, dir: Direction| {
            let mut i = Instance::new(n, "E", vec![Port::new("p", dir, "n")]);
            (i.x, i.y, i.width, i.height) = (x, 0, 100, 80);
            i
        };
        let mut d = Diagram {
            instances: vec![mk("a", 0, Direction::Out), mk("b", 300, Direction::In)],
            top_level_pins: vec![],
        };
        crate::group::create_group(&mut d, "G", &[0, 1], vec![]);
        if collapsed {
            crate::group::toggle_collapse(&mut d, 0, 20);
        }
        d
    }

    #[test]
    fn expanded_group_routes_to_children() {
        let w = route_all(&grouped(false));
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn collapsed_group_hides_children_nets() {
        assert!(route_all(&grouped(true)).is_empty());
    }
}
