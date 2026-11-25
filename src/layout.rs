use crate::config::*;
use crate::geometry::Pt;
use crate::model::{Direction, Instance};
use std::collections::BTreeMap;

fn ceil_to(v: i32, grid: i32) -> i32 {
    (v + grid - 1).div_euclid(grid) * grid
}

pub fn block_size(inst: &Instance, grid: i32) -> (i32, i32) {
    if inst.custom_width > 0 && inst.custom_height > 0 {
        return (inst.custom_width, inst.custom_height);
    }
    let n_in = inst.ports.iter().filter(|p| p.direction != Direction::Out).count();
    let n_out = inst.ports.iter().filter(|p| p.direction != Direction::In).count();
    let mut h = ((n_in.max(n_out)) as i32 * PORT_HEIGHT + 40).max(MIN_BLOCK_HEIGHT);
    // no font metrics outside egui: estimate the name's width from its length
    let text_w = (inst.name.chars().count() as f32 * inst.font_size as f32 * 0.7) as i32 + 20;
    let mut w = MIN_BLOCK_WIDTH.max(text_w);
    if inst.is_group && !inst.collapsed {
        if inst.children.is_empty() {
            (w, h) = (w.max(200), h.max(200));
        } else {
            let max_x = inst.children.iter().map(|c| c.x + c.width).max().unwrap_or(0);
            let max_y = inst.children.iter().map(|c| c.y + c.height).max().unwrap_or(0);
            w = w.max(max_x + 20 - inst.x);
            h = h.max(max_y + 20 - inst.y);
        }
    }
    (ceil_to(w, grid), ceil_to(h, grid))
}

/// Size every visible block; if all sit at (0,0) (fresh load), place them in a grid of rows/columns.
pub fn arrange_grid(instances: &mut [Instance], grid: i32) {
    let vis: Vec<usize> = (0..instances.len()).filter(|&i| instances[i].visible).collect();
    for &i in &vis {
        (instances[i].width, instances[i].height) = block_size(&instances[i], grid);
    }
    if vis.is_empty() || !vis.iter().all(|&i| instances[i].x == 0 && instances[i].y == 0) {
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

/// Absolute position of `inst.ports[port_idx]` on a standard (non-gate) block.
pub fn port_pos(inst: &Instance, port_idx: usize) -> Pt {
    let Some(port) = inst.ports.get(port_idx) else { return (inst.x, inst.y) };
    let left = port.direction == Direction::In;
    let idx = inst.ports[..port_idx].iter().filter(|p| (p.direction == Direction::In) == left).count() as i32;
    (if left { inst.x } else { inst.x + inst.width }, inst.y + 40 + idx * PORT_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Port;

    fn inst(n: &str, ports: Vec<Port>) -> Instance { Instance::new(n, "E", ports) }
    fn ins(k: usize) -> Vec<Port> { (0..k).map(|i| Port::new(&format!("i{i}"), Direction::In, "s")).collect() }

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
        let mut v: Vec<Instance> = (0..7).map(|k| inst(&format!("u{k}"), ins(k * 2 + 1))).collect();
        arrange_grid(&mut v, 20);
        for (a, x) in v.iter().enumerate() {
            assert_eq!((x.x % 20, x.y % 20), (0, 0));
            for y in &v[a + 1..] {
                let apart = x.x + x.width <= y.x || y.x + y.width <= x.x || x.y + x.height <= y.y || y.y + y.height <= x.y;
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
        let ports = vec![Port::new("in1", Direction::In, ""), Port::new("out1", Direction::Out, ""), Port::new("io", Direction::InOut, "")];
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
}
