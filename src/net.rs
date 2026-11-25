use crate::config::GRID_STEP;
use crate::geometry::{Pt, Seg, compress_polyline, junctions, mark_segment, snap};
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

/// Fan-out above which a net is "global" (drawn as stubs, not routed).
pub const GLOBAL_FANOUT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetClass {
    Clock,
    Reset,
    Bus,
    Signal,
    Undeclared,
}

fn name_class(sig: &str) -> Option<NetClass> {
    let l = sig.to_ascii_lowercase();
    // words after the clock/reset word that mark a derived control signal (`clk_en`, `reset_done`)
    const DERIVED: [&str; 16] = [
        "en", "enable", "div", "valid", "done", "ready", "strobe", "stb", "req", "ack", "cnt",
        "count", "tick", "pulse", "ok", "locked",
    ];
    let toks: Vec<&str> = l.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    toks.iter().enumerate().find_map(|(i, t)| {
        let class = match t.trim_end_matches(|c: char| c.is_ascii_digit()) {
            "clk" | "clock" | "clks" => NetClass::Clock,
            "rst" | "rstn" | "reset" | "resetn" | "arst" | "arstn" | "areset" | "aresetn"
            | "srst" | "srstn" => NetClass::Reset,
            _ => return None,
        };
        (!toks[i + 1..].iter().any(|w| DERIVED.contains(w))).then_some(class)
    })
}

/// Clock/reset by name; otherwise bus if its declared type is not a single-bit type.
pub fn net_class(sig: &str, signals: &HashMap<String, String>) -> NetClass {
    if let Some(c) = name_class(sig) {
        return c;
    }
    match signals.get(sig) {
        None => NetClass::Undeclared,
        Some(t) => {
            let t = t.to_ascii_lowercase();
            if matches!(
                t.trim(),
                "std_logic" | "std_ulogic" | "bit" | "boolean" | "logic" | "wire" | "reg"
            ) {
                NetClass::Signal
            } else {
                NetClass::Bus
            }
        }
    }
}

/// A short labelled stub at a pin of a global net; `dir` is -1 (extends left) or +1 (right).
#[derive(Clone, Debug, PartialEq)]
pub struct Stub {
    pub signal: String,
    pub pos: Pt,
    pub dir: i32,
}

pub struct Routing {
    pub wires: Vec<Wire>,
    pub stubs: Vec<Stub>,
}

/// Per top-level pin: does its signal reach at least one other pin?
pub fn pin_connected(d: &Diagram) -> Vec<bool> {
    let mut count: HashMap<&str, usize> = HashMap::new();
    let live = |s: &&str| !s.is_empty() && !s.eq_ignore_ascii_case("open");
    for s in d
        .top_level_pins
        .iter()
        .map(|p| p.signal.as_str())
        .filter(live)
    {
        *count.entry(s).or_default() += 1;
    }
    for i in active_instances(d) {
        for s in i.ports.iter().map(|p| p.signal.as_str()).filter(live) {
            *count.entry(s).or_default() += 1;
        }
    }
    d.top_level_pins
        .iter()
        .map(|p| count.get(p.signal.as_str()).is_some_and(|&n| n >= 2))
        .collect()
}

/// Build a laid-out diagram (block sizes and positions) from a parsed design.
pub fn design_to_diagram(d: &Design, grid: i32) -> Diagram {
    let mut diagram = Diagram {
        instances: d.instances.clone(),
        top_level_pins: d.top_ports.clone(),
    };
    arrange_grid(&mut diagram.instances, grid);
    crate::layout::auto_layout(&mut diagram, grid);
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
/// Each pin sits beside the blocks it connects to (pushed down to stay 30 apart); unconnected pins go last.
pub fn top_pin_positions(d: &Diagram) -> Vec<Pt> {
    let (x0, y0, x1, _) = bbox(d).unwrap_or((0, 0, 0, 0));
    let active = active_instances(d);
    let target_y = |sig: &str| -> Option<i32> {
        let ys: Vec<i32> = active
            .iter()
            .flat_map(|i| {
                i.ports
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| !sig.is_empty() && p.signal == sig)
                    .map(|(k, _)| port_pos(i, k).1)
            })
            .collect();
        (!ys.is_empty()).then(|| ys.iter().sum::<i32>() / ys.len() as i32)
    };
    let mut out = vec![(0, 0); d.top_level_pins.len()];
    for left in [true, false] {
        let mut side: Vec<(usize, Option<i32>)> = d
            .top_level_pins
            .iter()
            .enumerate()
            .filter(|(_, p)| (p.direction == Direction::In) == left)
            .map(|(k, p)| (k, target_y(&p.signal)))
            .collect();
        side.sort_by_key(|&(k, y)| (y.unwrap_or(i32::MAX), k));
        let mut next = y0 + 40;
        for (k, y) in side {
            let yy = snap(y.map_or(next, |y| y.max(next)), GRID_STEP);
            next = yy + 30;
            out[k] = (snap(if left { x0 - 100 } else { x1 + 100 }, GRID_STEP), yy);
        }
    }
    out
}

/// (endpoint, position, drives the net, stub direction, lane or -1)
type Endpoint = (Ep, Pt, bool, i32, i32);

const LANES: i32 = 6;

/// Where a pin's wire enters the routing grid: out along its side, `lane` columns past the block margin.
fn lane_cell(e: &Endpoint, occ: &crate::router::Occupancy) -> Pt {
    if e.4 < 0 {
        return e.1;
    }
    let c = (e.1.0 + e.3 * (40 + e.4 * GRID_STEP), e.1.1);
    if occ.get(&c) == Some(&false) { c } else { e.1 }
}

/// Route every net, global ones included.
pub fn route_all(d: &Diagram) -> Vec<Wire> {
    route_all_opts(d, false).wires
}

/// Route every net (signal with >= 2 endpoints) from its driver to each other endpoint.
/// With `stub_globals`, clock/reset nets and very wide fan-outs become pin stubs instead of wires.
pub fn route_all_opts(d: &Diagram, stub_globals: bool) -> Routing {
    let pins = top_pin_positions(d);
    // (endpoint, position, drives the net?) grouped by signal, in first-seen order
    let mut order: Vec<String> = Vec::new();
    let mut nets: HashMap<String, Vec<Endpoint>> = HashMap::new();
    let mut add = |sig: &str, ep: Ep, pos: Pt, drives: bool, dir: i32, lane: i32| {
        if sig.is_empty() || sig.eq_ignore_ascii_case("open") {
            return;
        }
        nets.entry(sig.to_string())
            .or_insert_with(|| {
                order.push(sig.to_string());
                Vec::new()
            })
            .push((ep, pos, drives, dir, lane));
    };
    for (k, p) in d.top_level_pins.iter().enumerate() {
        add(
            &p.signal,
            Ep::Top(k),
            pins[k],
            p.direction == Direction::In,
            if p.direction == Direction::In { 1 } else { -1 },
            -1,
        );
    }
    let active = active_instances(d);
    for (ii, inst) in active.iter().enumerate() {
        for (pi, p) in inst.ports.iter().enumerate() {
            add(
                &p.signal,
                Ep::Inst { inst: ii, port: pi },
                port_pos(inst, pi),
                p.direction != Direction::In,
                if p.direction == Direction::In { -1 } else { 1 },
                // pins on one side leave on different columns so their vertical runs do not coincide
                inst.ports[..pi]
                    .iter()
                    .filter(|q| (q.direction == Direction::In) == (p.direction == Direction::In))
                    .count() as i32
                    % LANES,
            );
        }
    }
    let is_global = |s: &str| {
        let n = nets[s].len();
        stub_globals
            && (n >= GLOBAL_FANOUT
                || n >= 3 && matches!(name_class(s), Some(NetClass::Clock | NetClass::Reset)))
    };
    let stubs: Vec<Stub> = order
        .iter()
        .filter(|s| nets[*s].len() >= 2 && is_global(s))
        .flat_map(|s| {
            nets[s].iter().map(|e| Stub {
                signal: s.clone(),
                pos: e.1,
                dir: e.3,
            })
        })
        .collect();
    let routed: Vec<&String> = order
        .iter()
        .filter(|s| nets[*s].len() >= 2 && !is_global(s))
        .collect();
    let Some((x0, y0, x1, y1)) = bbox(d) else {
        return Routing {
            wires: vec![],
            stubs,
        };
    };
    if routed.is_empty() {
        return Routing {
            wires: vec![],
            stubs,
        };
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
            let (a, b) = (lane_cell(src, &occ), lane_cell(dst, &occ));
            let mut poly = vec![src.1];
            poly.extend(router.route_net(a, b, &occ, &wire_occ, sig, bounds));
            poly.push(dst.1);
            let poly = compress_polyline(&poly);
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
    Routing { wires, stubs }
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

    /// Grid cells where two different signals run in the same direction (real overlap; crossings do not count).
    fn shared_cells(w: &[Wire]) -> usize {
        let (mut h, mut v) = (WireOcc::new(), WireOcc::new());
        for x in w {
            for &sg in &x.segments {
                let m = if (sg.0).1 == (sg.1).1 { &mut h } else { &mut v };
                mark_segment(sg, &x.signal, GRID_STEP, m);
            }
        }
        h.values().chain(v.values()).filter(|s| s.len() > 1).count()
    }

    #[test]
    fn different_nets_rarely_run_on_top_of_each_other() {
        // before lanes/auto layout (stubs mode): radio 975 cells, rc4 53; keep well below that
        for (src, limit) in [
            (include_str!("../tests/fixtures/radio_top.vhd"), 450),
            (include_str!("../tests/fixtures/RC4_sync_top.vhd"), 20),
        ] {
            let d = design_to_diagram(&crate::parser::vhdl::parse_vhdl(src), 20);
            let w = route_all_opts(&d, true).wires;
            assert!(
                shared_cells(&w) < limit,
                "{} cells overlap",
                shared_cells(&w)
            );
        }
    }

    #[test]
    fn net_class_by_name_and_type() {
        let sig: HashMap<String, String> = [
            ("a", "std_logic"),
            ("v", "std_logic_vector(7 downto 0)"),
            ("axi", "axi_lite_m2s_t"),
            ("first", "std_logic"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .into();
        assert_eq!(net_class("clk_fpga_in", &sig), NetClass::Clock);
        assert_eq!(net_class("s_arst_n", &sig), NetClass::Reset);
        assert_eq!(net_class("rst_n_in", &sig), NetClass::Reset);
        assert_eq!(net_class("first", &sig), NetClass::Signal); // "rst" inside a word is not a reset
        assert_eq!(net_class("clk_en", &sig), NetClass::Undeclared);
        assert_eq!(net_class("reset_done", &sig), NetClass::Undeclared);
        assert_eq!(net_class("inputs.wiz_clk", &sig), NetClass::Clock);
        assert_eq!(net_class("aresetn", &sig), NetClass::Reset);
        assert_eq!(net_class("a", &sig), NetClass::Signal);
        assert_eq!(net_class("v", &sig), NetClass::Bus);
        assert_eq!(net_class("axi", &sig), NetClass::Bus);
        assert_eq!(net_class("mystery", &sig), NetClass::Undeclared);
    }

    fn fanout(n: usize, sig: &str) -> Diagram {
        let insts = (0..n)
            .map(|k| {
                let dir = if k == 0 {
                    Direction::Out
                } else {
                    Direction::In
                };
                let mut i = Instance::new(&format!("u{k}"), "E", vec![Port::new("p", dir, sig)]);
                (i.x, i.y, i.width, i.height) = (k as i32 * 300, 0, 100, 80);
                i
            })
            .collect();
        Diagram {
            instances: insts,
            top_level_pins: vec![],
        }
    }

    #[test]
    fn clock_fanout_becomes_stubs_not_wires() {
        let d = fanout(5, "clk_main");
        let r = route_all_opts(&d, true);
        assert!(r.wires.is_empty());
        assert_eq!(r.stubs.len(), 5);
        assert_eq!(r.stubs[0].dir, 1); // the driver's output pin points right
        assert_eq!(r.stubs[1].dir, -1);
        assert_eq!(route_all_opts(&d, false).wires.len(), 4); // toggle off: routed as before
        // a small plain net is never a stub
        assert_eq!(route_all_opts(&fanout(3, "data"), true).wires.len(), 2);
        // a very wide plain net is
        assert!(
            route_all_opts(&fanout(GLOBAL_FANOUT, "data"), true)
                .wires
                .is_empty()
        );
    }

    #[test]
    fn unconnected_top_pins_are_flagged() {
        let mut d = two_blocks();
        d.top_level_pins.push(Port::new("a", Direction::In, "n1")); // joins n1
        d.top_level_pins
            .push(Port::new("b", Direction::Out, "nowhere"));
        d.top_level_pins
            .push(Port::new("c", Direction::Out, "n_solo")); // only one other end? a.lonely + pin
        assert_eq!(pin_connected(&d), vec![true, false, true]);
    }

    #[test]
    fn top_pins_sit_beside_their_blocks_and_keep_spacing() {
        let mut d = fanout(2, "s");
        d.instances[1].y = 600;
        d.top_level_pins.push(Port::new("lo", Direction::Out, "s2"));
        d.instances[1]
            .ports
            .push(Port::new("q", Direction::Out, "s2"));
        d.top_level_pins.push(Port::new("hi", Direction::Out, "s"));
        let p = top_pin_positions(&d);
        assert!(p[1].1 < p[0].1, "pin of the upper block is above");
        assert!(p[0].1 - p[1].1 >= 30);
        assert!(p[0].1 >= 600, "pin lands at the lower block's height");
    }
}
