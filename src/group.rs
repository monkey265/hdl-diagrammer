use crate::layout::block_size;
use crate::model::{Diagram, Direction, Instance, Port};
use std::collections::BTreeSet;

/// Visible blocks that are drawn and routed: top-level blocks, plus the children of expanded groups.
pub fn active_instances(d: &Diagram) -> Vec<&Instance> {
    fn walk<'a>(i: &'a Instance, out: &mut Vec<&'a Instance>) {
        if !i.visible {
            return;
        }
        out.push(i);
        if i.is_group && !i.collapsed {
            i.children.iter().for_each(|c| walk(c, out));
        }
    }
    let mut out = Vec::new();
    d.instances.iter().for_each(|i| walk(i, &mut out));
    out
}

/// Signals shared between the members and anything outside them (other blocks, top pins), as INOUT ports.
pub fn suggest_ports(d: &Diagram, members: &[usize]) -> Vec<Port> {
    let sigs = |i: &Instance| {
        i.ports
            .iter()
            .map(|p| p.signal.clone())
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("open"))
            .collect::<Vec<_>>()
    };
    let inner: BTreeSet<String> = members
        .iter()
        .filter_map(|&m| d.instances.get(m))
        .flat_map(sigs)
        .collect();
    let mut external: BTreeSet<String> = d
        .instances
        .iter()
        .enumerate()
        .filter(|(k, i)| i.visible && !members.contains(k))
        .flat_map(|(_, i)| sigs(i))
        .collect();
    external.extend(d.top_level_pins.iter().map(|p| p.name.clone()));
    inner
        .intersection(&external)
        .map(|s| Port::new(s, Direction::InOut, s))
        .collect()
}

fn frame_around(blocks: &[Instance]) -> (i32, i32, i32, i32) {
    let x1 = blocks.iter().map(|i| i.x).min().unwrap_or(0) - 20;
    let y1 = blocks.iter().map(|i| i.y).min().unwrap_or(0) - 40;
    let x2 = blocks.iter().map(|i| i.x + i.width).max().unwrap_or(200) + 20;
    let y2 = blocks.iter().map(|i| i.y + i.height).max().unwrap_or(200) + 20;
    (x1, y1, x2 - x1, y2 - y1)
}

/// Move `members` (top-level indices) into a new expanded group around them; returns its index.
pub fn create_group(d: &mut Diagram, name: &str, members: &[usize], ports: Vec<Port>) -> usize {
    let idxs: BTreeSet<usize> = members
        .iter()
        .copied()
        .filter(|&m| m < d.instances.len())
        .collect();
    let children: Vec<Instance> = idxs.iter().map(|&m| d.instances[m].clone()).collect();
    for &m in idxs.iter().rev() {
        d.instances.remove(m);
    }
    let (x, y, w, h) = frame_around(&children);
    let mut g = Instance::new(name, "GROUP", ports);
    (g.x, g.y, g.width, g.height) = (x, y, w, h);
    g.is_group = true;
    g.children = children;
    d.instances.push(g);
    d.instances.len() - 1
}

/// Empty 200x200 group at (x, y).
pub fn create_empty_group(d: &mut Diagram, name: &str, x: i32, y: i32) -> usize {
    let mut g = Instance::new(name, "GROUP", vec![]);
    (g.x, g.y, g.width, g.height) = (x, y, 200, 200);
    g.is_group = true;
    d.instances.push(g);
    d.instances.len() - 1
}

/// Put a group's children back at top level and remove the group. No-op if `idx` is not a group.
pub fn ungroup(d: &mut Diagram, idx: usize) {
    if !d.instances.get(idx).is_some_and(|i| i.is_group) {
        return;
    }
    let g = d.instances.remove(idx);
    d.instances.extend(g.children);
}

pub fn toggle_collapse(d: &mut Diagram, idx: usize, grid: i32) {
    let Some(g) = d.instances.get_mut(idx).filter(|i| i.is_group) else {
        return;
    };
    g.collapsed = !g.collapsed;
    if g.collapsed {
        (g.width, g.height) = block_size(g, grid);
    } else if !g.children.is_empty() {
        (g.x, g.y, g.width, g.height) = frame_around(&g.children);
    }
}

/// A name not used by any top-level block: Group_1, Group_2, ...
pub fn unique_group_name(d: &Diagram) -> String {
    (1..)
        .map(|n| format!("Group_{n}"))
        .find(|n| !d.instances.iter().any(|i| &i.name == n))
        .expect("infinite range")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blk(n: &str, x: i32, y: i32, sig: &str) -> Instance {
        let mut i = Instance::new(n, "E", vec![Port::new("p", Direction::Out, sig)]);
        (i.x, i.y, i.width, i.height) = (x, y, 100, 80);
        i
    }
    fn three() -> Diagram {
        Diagram {
            instances: vec![
                blk("a", 0, 0, "s1"),
                blk("b", 200, 0, "s1"),
                blk("c", 400, 0, "s2"),
            ],
            top_level_pins: vec![Port::new("s2", Direction::Out, "s2")],
            ..Default::default()
        }
    }

    #[test]
    fn group_then_ungroup_restores() {
        let mut d = three();
        let before: Vec<_> = d
            .instances
            .iter()
            .map(|i| (i.name.clone(), i.x, i.y))
            .collect();
        let g = create_group(&mut d, "G", &[0, 1], vec![]);
        assert_eq!(d.instances.len(), 2); // c + group
        assert_eq!(d.instances[g].children.len(), 2);
        assert!(d.instances[g].is_group && !d.instances[g].collapsed);
        ungroup(&mut d, g);
        let mut after: Vec<_> = d
            .instances
            .iter()
            .map(|i| (i.name.clone(), i.x, i.y))
            .collect();
        let mut before = before;
        after.sort();
        before.sort();
        assert_eq!(after, before);
    }

    #[test]
    fn group_frame_surrounds_members() {
        let mut d = three();
        let g = create_group(&mut d, "G", &[0, 1], vec![]);
        let gi = &d.instances[g];
        assert!(
            gi.x <= 0 - 20
                && gi.y <= 0 - 40
                && gi.x + gi.width >= 300 + 20
                && gi.y + gi.height >= 80 + 20
        );
    }

    #[test]
    fn group_ports_follow_dialog_list() {
        let mut d = three();
        let ports = vec![
            Port::new("x", Direction::In, "s1"),
            Port::new("y", Direction::Out, "s9"),
        ];
        let g = create_group(&mut d, "G", &[0], ports.clone());
        assert_eq!(d.instances[g].ports, ports);
        assert_eq!(d.instances[g].entity, "GROUP");
    }

    #[test]
    fn suggest_ports_finds_external_signals() {
        let d = three();
        // a,b share s1 internally; c uses s2 which is also a top pin -> only s2... for members [0]: s1 is shared with b (outside)
        let names: Vec<_> = suggest_ports(&d, &[0])
            .iter()
            .map(|p| p.signal.clone())
            .collect();
        assert_eq!(names, vec!["s1"]);
        assert!(suggest_ports(&d, &[0, 1]).is_empty());
        let names: Vec<_> = suggest_ports(&d, &[2])
            .iter()
            .map(|p| p.signal.clone())
            .collect();
        assert_eq!(names, vec!["s2"]); // top pin
    }

    #[test]
    fn collapse_hides_children() {
        let mut d = three();
        let g = create_group(&mut d, "G", &[0, 1], vec![]);
        assert_eq!(active_instances(&d).len(), 4); // c, G, a, b
        toggle_collapse(&mut d, g, 20);
        assert!(d.instances[g].collapsed);
        assert_eq!(active_instances(&d).len(), 2); // c, G
        toggle_collapse(&mut d, g, 20);
        assert_eq!(active_instances(&d).len(), 4);
    }

    #[test]
    fn ungroup_non_group_is_noop() {
        let mut d = three();
        ungroup(&mut d, 0);
        ungroup(&mut d, 99);
        assert_eq!(d.instances.len(), 3);
    }

    #[test]
    fn empty_group_and_unique_names() {
        let mut d = three();
        assert_eq!(unique_group_name(&d), "Group_1");
        create_empty_group(&mut d, "Group_1", 10, 10);
        assert_eq!(unique_group_name(&d), "Group_2");
        let _ = block_size(&d.instances[3], 20);
    }
}
