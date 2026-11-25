use crate::geometry::snap;
use crate::layout::port_pos;
use crate::model::Diagram;

fn shift(i: &mut crate::model::Instance, dx: i32, dy: i32) {
    i.x += dx;
    i.y += dy;
    i.children.iter_mut().for_each(|c| shift(c, dx, dy));
}

/// Move a block to (x, y) snapped to `grid`; locked blocks do not move.
pub fn move_block(d: &mut Diagram, idx: usize, x: i32, y: i32, grid: i32) {
    if let Some(i) = d.instances.get_mut(idx).filter(|i| !i.locked) {
        let (dx, dy) = (snap(x, grid) - i.x, snap(y, grid) - i.y);
        shift(i, dx, dy); // a group carries its children (absolute coordinates)
    }
}

/// Recompute a block's size after its ports or custom size changed.
pub fn refit(d: &mut Diagram, idx: usize, grid: i32) {
    if let Some(i) = d.instances.get_mut(idx) {
        (i.width, i.height) = crate::layout::block_size(i, grid);
    }
}

fn overlaps(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// If block `idx` overlaps another visible block, move it to the nearest free grid spot.
pub fn resolve_collision(d: &mut Diagram, idx: usize, grid: i32) {
    let Some(me) = d.instances.get(idx).filter(|i| !i.locked) else {
        return;
    };
    let others: Vec<_> = d
        .instances
        .iter()
        .enumerate()
        .filter(|(k, i)| *k != idx && i.visible)
        .map(|(_, i)| (i.x, i.y, i.width, i.height))
        .collect();
    let free = |x: i32, y: i32| {
        !others
            .iter()
            .any(|&o| overlaps((x, y, me.width, me.height), o))
    };
    if free(me.x, me.y) {
        return;
    }
    const R: i32 = 30; // search radius in grid cells
    let mut ring: Vec<(i32, i32)> = (-R..=R)
        .flat_map(|dx| (-R..=R).map(move |dy| (dx, dy)))
        .collect();
    ring.sort_by_key(|(dx, dy)| dx * dx + dy * dy);
    let (x0, y0) = (me.x, me.y);
    if let Some((dx, dy)) = ring
        .into_iter()
        .find(|&(dx, dy)| free(x0 + dx * grid, y0 + dy * grid))
    {
        shift(&mut d.instances[idx], dx * grid, dy * grid); // children follow a group
    }
}

/// Topmost (last drawn) visible block containing the world point.
pub fn hit_block(d: &Diagram, p: (f32, f32)) -> Option<usize> {
    d.instances
        .iter()
        .enumerate()
        .rev()
        .find(|(_, i)| {
            i.visible
                && p.0 >= i.x as f32
                && p.0 <= (i.x + i.width) as f32
                && p.1 >= i.y as f32
                && p.1 <= (i.y + i.height) as f32
        })
        .map(|(k, _)| k)
}

/// Nearest port of a visible block within `radius` of the world point.
pub fn hit_port(d: &Diagram, p: (f32, f32), radius: f32) -> Option<(usize, usize)> {
    let mut best: Option<(f32, (usize, usize))> = None;
    for (ii, inst) in d.instances.iter().enumerate().filter(|(_, i)| i.visible) {
        for pi in 0..inst.ports.len() {
            let (x, y) = port_pos(inst, pi);
            let dist = ((x as f32 - p.0).powi(2) + (y as f32 - p.1).powi(2)).sqrt();
            if dist <= radius && best.is_none_or(|(b, _)| dist < b) {
                best = Some((dist, (ii, pi)));
            }
        }
    }
    best.map(|(_, k)| k)
}

pub fn set_visible(d: &mut Diagram, idx: usize, visible: bool) {
    if let Some(i) = d.instances.get_mut(idx) {
        i.visible = visible;
    }
}

pub fn toggle_lock(d: &mut Diagram, idx: usize) {
    if let Some(i) = d.instances.get_mut(idx) {
        i.locked = !i.locked;
    }
}

pub fn delete_port(d: &mut Diagram, idx: usize, port: usize) {
    if let Some(i) = d.instances.get_mut(idx).filter(|i| port < i.ports.len()) {
        i.ports.remove(port);
    }
}

pub fn reset_ports(d: &mut Diagram, idx: usize) {
    if let Some(i) = d.instances.get_mut(idx) {
        i.ports = i.original_ports.clone();
    }
}

/// Set a custom size (grid-snapped upward, at least 50x50); `None` clears it back to auto-size.
pub fn set_size(d: &mut Diagram, idx: usize, size: Option<(i32, i32)>, grid: i32) {
    let Some(i) = d.instances.get_mut(idx) else {
        return;
    };
    let up = |v: i32| (v.max(50) + grid - 1).div_euclid(grid) * grid;
    (i.custom_width, i.custom_height) = size.map_or((0, 0), |(w, h)| (up(w), up(h)));
    refit(d, idx, grid);
}

/// Index of the wire whose polyline passes within `tol` world units of the point.
pub fn hit_wire(wires: &[crate::net::Wire], p: (f32, f32), tol: f32) -> Option<usize> {
    wires.iter().position(|w| {
        w.segments.iter().any(|&((x1, y1), (x2, y2))| {
            let (cx, cy) = (
                p.0.clamp(x1.min(x2) as f32, x1.max(x2) as f32),
                p.1.clamp(y1.min(y2) as f32, y1.max(y2) as f32),
            );
            ((cx - p.0).powi(2) + (cy - p.1).powi(2)).sqrt() <= tol
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Direction, Instance, Port};

    fn blk(x: i32, y: i32) -> Instance {
        let mut i = Instance::new(
            "u",
            "E",
            vec![
                Port::new("a", Direction::In, "s"),
                Port::new("b", Direction::Out, "s"),
            ],
        );
        (i.x, i.y, i.width, i.height) = (x, y, 100, 100);
        i
    }
    fn dia(v: Vec<Instance>) -> Diagram {
        Diagram {
            instances: v,
            top_level_pins: vec![],
        }
    }

    #[test]
    fn drag_snaps_to_grid() {
        let mut d = dia(vec![blk(0, 0)]);
        move_block(&mut d, 0, 113, 87, 20);
        assert_eq!((d.instances[0].x, d.instances[0].y), (120, 80));
    }

    #[test]
    fn locked_block_does_not_move() {
        let mut d = dia(vec![blk(40, 40)]);
        toggle_lock(&mut d, 0);
        move_block(&mut d, 0, 300, 300, 20);
        assert_eq!((d.instances[0].x, d.instances[0].y), (40, 40));
        toggle_lock(&mut d, 0);
        move_block(&mut d, 0, 300, 300, 20);
        assert_eq!(d.instances[0].x, 300);
    }

    #[test]
    fn collision_pushes_to_nearby_free_spot() {
        let mut d = dia(vec![blk(100, 100), blk(100, 100)]);
        resolve_collision(&mut d, 1, 20);
        let (a, b) = (&d.instances[0], &d.instances[1]);
        assert!((b.x, b.y) != (100, 100));
        let apart = b.x >= a.x + a.width
            || b.x + b.width <= a.x
            || b.y >= a.y + a.height
            || b.y + b.height <= a.y;
        assert!(apart);
        assert!((b.x - 100).abs() <= 160 && (b.y - 100).abs() <= 160);
        assert_eq!((a.x, a.y), (100, 100)); // the other block never moves
    }

    #[test]
    fn collision_noop_when_free() {
        let mut d = dia(vec![blk(0, 0), blk(300, 0)]);
        resolve_collision(&mut d, 1, 20);
        assert_eq!(d.instances[1].x, 300);
    }

    #[test]
    fn hit_block_prefers_topmost_and_skips_hidden() {
        let mut d = dia(vec![blk(0, 0), blk(50, 50)]);
        assert_eq!(hit_block(&d, (60.0, 60.0)), Some(1));
        assert_eq!(hit_block(&d, (10.0, 10.0)), Some(0));
        assert_eq!(hit_block(&d, (500.0, 500.0)), None);
        set_visible(&mut d, 1, false);
        assert_eq!(hit_block(&d, (60.0, 60.0)), Some(0));
    }

    #[test]
    fn hit_port_finds_pin_within_radius() {
        let d = dia(vec![blk(0, 0)]);
        let (px, py) = port_pos(&d.instances[0], 1);
        assert_eq!(
            hit_port(&d, (px as f32 + 2.0, py as f32), 6.0),
            Some((0, 1))
        );
        assert_eq!(hit_port(&d, (px as f32 + 40.0, py as f32), 6.0), None);
    }

    #[test]
    fn delete_port_and_reset() {
        let mut d = dia(vec![blk(0, 0)]);
        delete_port(&mut d, 0, 0);
        assert_eq!(d.instances[0].ports.len(), 1);
        assert_eq!(d.instances[0].ports[0].name, "b");
        reset_ports(&mut d, 0);
        assert_eq!(d.instances[0].ports.len(), 2);
        delete_port(&mut d, 0, 9); // out of range: no panic
    }

    #[test]
    fn set_size_snaps_and_clamps() {
        let mut d = dia(vec![blk(0, 0)]);
        set_size(&mut d, 0, Some((333, 10)), 20);
        assert_eq!(
            (d.instances[0].custom_width, d.instances[0].custom_height),
            (340, 60)
        );
        set_size(&mut d, 0, None, 20);
        assert_eq!(
            (d.instances[0].custom_width, d.instances[0].custom_height),
            (0, 0)
        );
    }

    #[test]
    fn hit_wire_near_segment() {
        use crate::net::{Ep, Wire};
        let w = Wire {
            signal: "s".into(),
            src: Ep::Top(0),
            dst: Ep::Top(1),
            segments: vec![((0, 0), (100, 0)), ((100, 0), (100, 50))],
        };
        assert_eq!(
            hit_wire(std::slice::from_ref(&w), (50.0, 3.0), 4.0),
            Some(0)
        );
        assert_eq!(
            hit_wire(std::slice::from_ref(&w), (103.0, 30.0), 4.0),
            Some(0)
        );
        assert_eq!(hit_wire(std::slice::from_ref(&w), (50.0, 20.0), 4.0), None);
    }

    #[test]
    fn moving_group_moves_children() {
        let mut d = dia(vec![blk(0, 0), blk(200, 0)]);
        let g = crate::group::create_group(&mut d, "G", &[0, 1], vec![]);
        let (cx, cy) = (d.instances[g].children[1].x, d.instances[g].children[1].y);
        let (gx, gy) = (d.instances[g].x, d.instances[g].y);
        move_block(&mut d, g, gx + 100, gy + 60, 20);
        assert_eq!(
            (d.instances[g].children[1].x, d.instances[g].children[1].y),
            (cx + 100, cy + 60)
        );
    }

    #[test]
    fn collision_push_carries_group_children() {
        let mut d = dia(vec![blk(0, 0), blk(200, 0), blk(600, 0)]);
        let g = crate::group::create_group(&mut d, "G", &[0, 1], vec![]);
        let o = d.instances.iter().position(|i| !i.is_group).unwrap();
        let (gx, gy) = (d.instances[g].x, d.instances[g].y);
        (d.instances[o].x, d.instances[o].y) = (gx + 20, gy + 20); // lands on the group
        resolve_collision(&mut d, g, 20);
        let gr = &d.instances[g];
        assert!((gr.x, gr.y) != (gx, gy));
        for c in &gr.children {
            assert!(c.x >= gr.x && c.y >= gr.y, "child left behind by push-out");
        }
    }

    #[test]
    fn collision_never_moves_locked_block() {
        let mut d = dia(vec![blk(100, 100), blk(100, 100)]);
        toggle_lock(&mut d, 1);
        resolve_collision(&mut d, 1, 20);
        assert_eq!((d.instances[1].x, d.instances[1].y), (100, 100));
    }

    #[test]
    fn set_size_and_port_edits_resize_block() {
        let mut d = dia(vec![blk(0, 0)]);
        set_size(&mut d, 0, Some((333, 10)), 20);
        assert_eq!((d.instances[0].width, d.instances[0].height), (340, 60));
        set_size(&mut d, 0, None, 20);
        let auto = (d.instances[0].width, d.instances[0].height);
        for _ in 0..8 {
            let p = d.instances[0].ports[0].clone();
            d.instances[0].ports.push(p);
        }
        refit(&mut d, 0, 20);
        assert!(d.instances[0].height > auto.1);
    }
}
