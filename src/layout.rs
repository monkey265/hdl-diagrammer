use crate::config::*;
use crate::gates::{GateKind, gate_kind};
use crate::geometry::{Pt, snap};
use crate::model::{Direction, Instance};
use std::collections::{BTreeMap, BTreeSet};

fn ceil_to(v: i32, grid: i32) -> i32 {
    (v + grid - 1).div_euclid(grid) * grid
}

pub fn block_size(inst: &Instance, grid: i32) -> (i32, i32) {
    if inst.custom_width > 0 && inst.custom_height > 0 {
        return (inst.custom_width, inst.custom_height);
    }
    let n_in = inst
        .ports
        .iter()
        .filter(|p| p.direction != Direction::Out)
        .count();
    let n_out = inst
        .ports
        .iter()
        .filter(|p| p.direction != Direction::In)
        .count();
    let mut h = ((n_in.max(n_out)) as i32 * PORT_HEIGHT + 40).max(MIN_BLOCK_HEIGHT);
    // no font metrics outside egui: estimate the name's width from its length
    let text_w = (inst.name.chars().count() as f32 * inst.font_size as f32 * 0.7) as i32 + 20;
    let mut w = MIN_BLOCK_WIDTH.max(text_w);
    if inst.is_group && !inst.collapsed {
        if inst.children.is_empty() {
            (w, h) = (w.max(200), h.max(200));
        } else {
            let max_x = inst
                .children
                .iter()
                .map(|c| c.x + c.width)
                .max()
                .unwrap_or(0);
            let max_y = inst
                .children
                .iter()
                .map(|c| c.y + c.height)
                .max()
                .unwrap_or(0);
            w = w.max(max_x + 20 - inst.x);
            h = h.max(max_y + 20 - inst.y);
        }
    }
    (ceil_to(w, grid), ceil_to(h, grid))
}

/// Size every visible block; if all sit at (0,0) (fresh load), place them in a grid of rows/columns.
pub fn arrange_grid(instances: &mut [Instance], grid: i32) {
    let vis: Vec<usize> = (0..instances.len())
        .filter(|&i| instances[i].visible)
        .collect();
    for &i in &vis {
        (instances[i].width, instances[i].height) = block_size(&instances[i], grid);
    }
    if vis.is_empty()
        || !vis
            .iter()
            .all(|&i| instances[i].x == 0 && instances[i].y == 0)
    {
        return;
    }
    let cols = (vis.len() as f64).sqrt().ceil() as usize;
    let mut row_h: BTreeMap<usize, i32> = BTreeMap::new();
    let mut max_w = 0;
    for (n, &i) in vis.iter().enumerate() {
        let e = row_h.entry(n / cols).or_insert(0);
        *e = (*e).max(instances[i].height);
        max_w = max_w.max(instances[i].width);
    }
    for (n, &i) in vis.iter().enumerate() {
        let (row, col) = (n / cols, n % cols);
        let above: i32 = (0..row).map(|r| row_h[&r] + 100).sum();
        instances[i].x = ceil_to(col as i32 * (max_w + 150) + 50, grid);
        instances[i].y = ceil_to(40 + above, grid);
    }
}

/// Left-to-right layered layout: drivers left of their loads, rows ordered to cut crossings,
/// channels widened with the number of nets crossing them. Locked blocks stay where they are.
pub fn auto_layout(d: &mut crate::model::Diagram, grid: i32) {
    let nodes: Vec<usize> = (0..d.instances.len())
        .filter(|&i| d.instances[i].visible)
        .collect();
    let n = nodes.len();
    if n == 0 {
        return;
    }
    // edges driver -> load between nodes (positions in `nodes`), one per ordered pair
    let mut edges: BTreeSet<(usize, usize)> = BTreeSet::new();
    let mut by_sig: BTreeMap<&str, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for (k, &i) in nodes.iter().enumerate() {
        for p in &d.instances[i].ports {
            if p.signal.is_empty() || p.signal.eq_ignore_ascii_case("open") {
                continue;
            }
            let e = by_sig.entry(p.signal.as_str()).or_default();
            if p.direction == Direction::In {
                e.1.push(k);
            } else {
                e.0.push(k);
            }
        }
    }
    for (src, dst) in by_sig.values() {
        for &a in src {
            for &b in dst {
                if a != b {
                    edges.insert((a, b));
                }
            }
        }
    }
    // break cycles: DFS, edges to a node still on the stack are dropped
    let mut succ = vec![vec![]; n];
    edges.iter().for_each(|&(a, b)| succ[a].push(b));
    let (mut state, mut order) = (vec![0u8; n], Vec::new());
    let mut fwd: Vec<(usize, usize)> = Vec::new();
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        let mut stack = vec![(root, 0usize)];
        state[root] = 1;
        while let Some(&mut (u, ref mut k)) = stack.last_mut() {
            if *k < succ[u].len() {
                let v = succ[u][*k];
                *k += 1;
                match state[v] {
                    0 => {
                        fwd.push((u, v));
                        state[v] = 1;
                        stack.push((v, 0));
                    }
                    2 => fwd.push((u, v)),
                    _ => {} // back edge
                }
            } else {
                state[u] = 2;
                order.push(u);
                stack.pop();
            }
        }
    }
    let mut layer = vec![0usize; n];
    for &u in order.iter().rev() {
        for &(a, b) in fwd.iter().filter(|e| e.0 == u) {
            layer[b] = layer[b].max(layer[a] + 1);
        }
    }
    let n_layers = layer.iter().max().copied().unwrap_or(0) + 1;
    let mut rows: Vec<Vec<usize>> = vec![vec![]; n_layers];
    (0..n).for_each(|k| rows[layer[k]].push(k));
    // barycentre sweeps
    let mut pos = vec![0f32; n];
    let refresh = |rows: &Vec<Vec<usize>>, pos: &mut Vec<f32>| {
        rows.iter()
            .flat_map(|r| r.iter().enumerate())
            .for_each(|(i, &k)| pos[k] = i as f32);
    };
    refresh(&rows, &mut pos);
    for sweep in 0..4 {
        let down = sweep % 2 == 0;
        let range: Vec<usize> = if down {
            (1..n_layers).collect()
        } else {
            (0..n_layers.saturating_sub(1)).rev().collect()
        };
        for l in range {
            let key = |k: usize| -> f32 {
                let nb: Vec<f32> = fwd
                    .iter()
                    .filter_map(|&(a, b)| match (down, a == k, b == k) {
                        (true, false, true) => Some(pos[a]),
                        (false, true, false) => Some(pos[b]),
                        _ => None,
                    })
                    .collect();
                if nb.is_empty() {
                    pos[k]
                } else {
                    nb.iter().sum::<f32>() / nb.len() as f32
                }
            };
            let mut r = rows[l].clone();
            r.sort_by(|&a, &b| key(a).total_cmp(&key(b)));
            rows[l] = r;
            refresh(&rows, &mut pos);
        }
    }
    // coordinates
    let w = |k: usize| d.instances[nodes[k]].width;
    let h = |k: usize| d.instances[nodes[k]].height;
    let heights: Vec<i32> = rows
        .iter()
        .map(|r| r.iter().map(|&k| h(k)).sum::<i32>() + 60 * r.len().saturating_sub(1) as i32)
        .collect();
    let tallest = heights.iter().copied().max().unwrap_or(0);
    let mut x = 50;
    let mut place: Vec<(usize, i32, i32)> = Vec::new();
    for (l, r) in rows.iter().enumerate() {
        let mut y = 40 + (tallest - heights[l]) / 2;
        for &k in r {
            place.push((nodes[k], x, y));
            y += h(k) + 60;
        }
        let crossing = fwd
            .iter()
            .filter(|&&(a, b)| layer[a] <= l && layer[b] > l)
            .count() as i32;
        let colw = r.iter().map(|&k| w(k)).max().unwrap_or(0);
        x += colw + 180 + 10 * crossing.min(24);
    }
    for (i, px, py) in place {
        crate::edit::move_block(d, i, px, py, grid);
    }
}

/// Absolute position of `inst.ports[port_idx]`: ANSI gate symbols and standard blocks differ.
pub fn port_pos(inst: &Instance, port_idx: usize) -> Pt {
    let Some(port) = inst.ports.get(port_idx) else {
        return (inst.x, inst.y);
    };
    if let Some(kind) = gate_kind(&inst.entity) {
        let center = snap(inst.y + inst.height / 2, 20);
        if port.direction == Direction::Out {
            return (inst.x + inst.width, center);
        }
        let n_in = inst
            .ports
            .iter()
            .filter(|p| p.direction != Direction::Out)
            .count() as i32;
        let idx = inst.ports[..port_idx]
            .iter()
            .filter(|p| p.direction != Direction::Out)
            .count() as i32;
        let start = snap(center - (n_in - 1) * 20 / 2, 20);
        let dx = if matches!(kind, GateKind::Xor | GateKind::Xnor) {
            10
        } else {
            0
        };
        return (inst.x + dx, start + idx * 20);
    }
    let left = port.direction == Direction::In;
    let idx = inst.ports[..port_idx]
        .iter()
        .filter(|p| (p.direction == Direction::In) == left)
        .count() as i32;
    (
        if left { inst.x } else { inst.x + inst.width },
        inst.y + 40 + idx * PORT_HEIGHT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Diagram, Port};

    fn inst(n: &str, ports: Vec<Port>) -> Instance {
        Instance::new(n, "E", ports)
    }
    fn ins(k: usize) -> Vec<Port> {
        (0..k)
            .map(|i| Port::new(&format!("i{i}"), Direction::In, "s"))
            .collect()
    }

    #[test]
    fn block_size_respects_minimum() {
        let (w, h) = block_size(&inst("u", ins(1)), 20);
        assert!(w >= MIN_BLOCK_WIDTH && h >= MIN_BLOCK_HEIGHT);
        assert_eq!((w % 20, h % 20), (0, 0));
        let (_, h10) = block_size(&inst("u", ins(10)), 20);
        assert_eq!(h10, 240); // 10*20+40
    }

    #[test]
    fn custom_size_wins() {
        let mut i = inst("u", ins(10));
        i.custom_width = 333;
        i.custom_height = 77;
        assert_eq!(block_size(&i, 20), (333, 77));
    }

    #[test]
    fn group_grows_to_cover_children() {
        let mut g = inst("g", vec![]);
        g.is_group = true;
        g.x = 100;
        g.y = 100;
        let mut c = inst("c", vec![]);
        c.x = 300;
        c.y = 300;
        c.width = 100;
        c.height = 100;
        g.children.push(c);
        let (w, h) = block_size(&g, 20);
        assert!(100 + w >= 420 && 100 + h >= 420);
    }

    #[test]
    fn arrange_grid_no_overlap() {
        let mut v: Vec<Instance> = (0..7)
            .map(|k| inst(&format!("u{k}"), ins(k * 2 + 1)))
            .collect();
        arrange_grid(&mut v, 20);
        for (a, x) in v.iter().enumerate() {
            assert_eq!((x.x % 20, x.y % 20), (0, 0));
            for y in &v[a + 1..] {
                let apart = x.x + x.width <= y.x
                    || y.x + y.width <= x.x
                    || x.y + x.height <= y.y
                    || y.y + y.height <= x.y;
                assert!(apart, "{} overlaps {}", x.name, y.name);
            }
        }
    }

    #[test]
    fn arrange_grid_keeps_existing_positions() {
        let mut v = vec![inst("a", vec![]), inst("b", vec![])];
        v[1].x = 500;
        arrange_grid(&mut v, 20);
        assert_eq!((v[0].x, v[0].y, v[1].x), (0, 0, 500));
        assert!(v[0].width > 0);
    }

    #[test]
    fn out_port_on_right_edge() {
        let ports = vec![
            Port::new("in1", Direction::In, ""),
            Port::new("out1", Direction::Out, ""),
            Port::new("io", Direction::InOut, ""),
        ];
        let mut i = inst("U1", ports);
        (i.x, i.y, i.width, i.height) = (100, 100, 100, 100);
        assert_eq!(port_pos(&i, 0).0, 100);
        assert_eq!(port_pos(&i, 1).0, 200);
        assert_eq!(port_pos(&i, 2).0, 200);
        assert_eq!(port_pos(&i, 2).1, 100 + 40 + 20); // INOUT is second on the right
    }

    #[test]
    fn ports_snap_to_grid() {
        let mut i = inst("U", ins(3));
        (i.x, i.y, i.width, i.height) = (40, 60, 160, 120);
        for k in 0..3 {
            assert_eq!(port_pos(&i, k).1 % 20, 0);
        }
    }

    #[test]
    fn gate_port_snaps_to_grid() {
        let mut g = inst(
            "U1",
            vec![
                Port::new("A", Direction::In, ""),
                Port::new("B", Direction::In, ""),
                Port::new("Z", Direction::Out, ""),
            ],
        );
        g.entity = "AND2".into();
        (g.x, g.y, g.width, g.height) = (100, 100, 60, 50);
        for k in 0..3 {
            assert_eq!(port_pos(&g, k).1 % 20, 0, "port {k}");
        }
        assert_eq!(port_pos(&g, 0).0, 100);
        assert_eq!(port_pos(&g, 2).0, 160);
        g.entity = "XOR".into();
        assert_eq!(port_pos(&g, 0).0, 110); // XOR inputs sit on the curved edge
    }

    fn chain(n: usize, cyclic: bool) -> crate::model::Diagram {
        let insts = (0..n)
            .map(|k| {
                let mut ports = vec![Port::new("o", Direction::Out, &format!("s{k}"))];
                if k > 0 {
                    ports.push(Port::new("i", Direction::In, &format!("s{}", k - 1)));
                } else if cyclic {
                    ports.push(Port::new("i", Direction::In, &format!("s{}", n - 1)));
                }
                let mut i = Instance::new(&format!("u{k}"), "E", ports);
                (i.width, i.height) = (100, 80);
                i
            })
            .collect();
        crate::model::Diagram {
            instances: insts,
            top_level_pins: vec![],
        }
    }

    fn no_overlap(d: &crate::model::Diagram) -> bool {
        let v = &d.instances;
        (0..v.len()).all(|a| {
            (a + 1..v.len()).all(|b| {
                let (p, q) = (&v[a], &v[b]);
                p.x + p.width <= q.x
                    || q.x + q.width <= p.x
                    || p.y + p.height <= q.y
                    || q.y + q.height <= p.y
            })
        })
    }

    #[test]
    fn auto_layout_puts_drivers_left_of_loads() {
        let mut d = chain(4, false);
        auto_layout(&mut d, 20);
        let xs: Vec<i32> = d.instances.iter().map(|i| i.x).collect();
        assert!(xs.windows(2).all(|w| w[0] < w[1]), "{xs:?}");
        assert!(no_overlap(&d));
    }

    #[test]
    fn auto_layout_survives_cycles_and_keeps_locked_blocks() {
        let mut d = chain(5, true);
        d.instances[2].locked = true;
        (d.instances[2].x, d.instances[2].y) = (7000, 7000);
        auto_layout(&mut d, 20);
        assert_eq!((d.instances[2].x, d.instances[2].y), (7000, 7000));
        let mut e = chain(5, true);
        auto_layout(&mut e, 20);
        assert!(no_overlap(&e));
        auto_layout(&mut Diagram::default(), 20);
    }

    #[test]
    fn auto_layout_radio_top_has_no_overlap_and_is_compact() {
        let des = crate::parser::vhdl::parse_vhdl(include_str!("../tests/fixtures/radio_top.vhd"));
        let mut d = crate::net::design_to_diagram(&des, 20);
        auto_layout(&mut d, 20);
        assert!(no_overlap(&d));
        let w = d.instances.iter().map(|i| i.x + i.width).max().unwrap();
        let h = d.instances.iter().map(|i| i.y + i.height).max().unwrap();
        assert!(w > 0 && h > 0);
    }
}
