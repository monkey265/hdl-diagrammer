use std::collections::{BTreeMap, HashMap, HashSet};

pub type Pt = (i32, i32);
pub type Seg = (Pt, Pt);

pub fn snap(v: i32, grid: i32) -> i32 {
    (v as f64 / grid as f64).round() as i32 * grid
}

/// Drop points that lie on a straight horizontal/vertical run and consecutive duplicates.
pub fn compress_polyline(pts: &[Pt]) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::new();
    for &p in pts {
        if out.last() == Some(&p) {
            continue;
        }
        if out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            if (a.0 == b.0 && b.0 == p.0) || (a.1 == b.1 && b.1 == p.1) {
                *out.last_mut().unwrap() = p;
                continue;
            }
        }
        out.push(p);
    }
    out
}

/// Add every grid cell on an axis-aligned segment to `occ` under `signal`.
pub fn mark_segment(s: Seg, signal: &str, grid: i32, occ: &mut HashMap<Pt, HashSet<String>>) {
    let ((x1, y1), (x2, y2)) = s;
    let mut add = |c: Pt| {
        occ.entry(c).or_default().insert(signal.to_string());
    };
    if y1 == y2 {
        (x1.min(x2)..=x1.max(x2)).step_by(grid as usize).for_each(|x| add((x, y1)));
    } else if x1 == x2 {
        (y1.min(y2)..=y1.max(y2)).step_by(grid as usize).for_each(|y| add((x1, y)));
    }
}

/// Junction dots: grid points where one signal's unit segments meet more than twice, excluding ports.
pub fn junctions(wires: &[(String, Vec<Seg>)], ports: &HashSet<Pt>, grid: i32) -> Vec<(String, Pt)> {
    let mut out = Vec::new();
    for (sig, segs) in wires {
        let mut units: HashSet<Seg> = HashSet::new();
        for &((x1, y1), (x2, y2)) in segs {
            if x1 == x2 {
                for y in (y1.min(y2)..y1.max(y2)).step_by(grid as usize) {
                    units.insert(((x1, y), (x1, y + grid)));
                }
            } else if y1 == y2 {
                for x in (x1.min(x2)..x1.max(x2)).step_by(grid as usize) {
                    units.insert(((x, y1), (x + grid, y1)));
                }
            }
        }
        let mut counts: BTreeMap<Pt, u32> = BTreeMap::new();
        for (a, b) in units {
            *counts.entry(a).or_default() += 1;
            *counts.entry(b).or_default() += 1;
        }
        out.extend(counts.into_iter().filter(|(p, n)| *n > 2 && !ports.contains(p)).map(|(p, _)| (sig.clone(), p)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_rounds_to_grid() {
        assert_eq!(snap(14, 20), 20);
        assert_eq!(snap(9, 20), 0);
        assert_eq!(snap(-14, 20), -20);
        assert_eq!(snap(40, 40), 40);
    }

    #[test]
    fn compress_removes_collinear() {
        assert_eq!(compress_polyline(&[(0, 0), (10, 0), (20, 0), (20, 10), (20, 20)]), vec![(0, 0), (20, 0), (20, 20)]);
        assert_eq!(compress_polyline(&[]), vec![]);
        assert_eq!(compress_polyline(&[(0, 0), (0, 0)]), vec![(0, 0)]);
    }

    #[test]
    fn mark_segment_covers_cells_both_directions() {
        let mut occ = HashMap::new();
        mark_segment(((20, 0), (0, 0)), "a", 10, &mut occ);
        mark_segment(((0, 0), (0, 20)), "b", 10, &mut occ);
        assert_eq!(occ.len(), 5);
        assert!(occ[&(0, 0)].contains("a") && occ[&(0, 0)].contains("b"));
    }

    #[test]
    fn junction_at_t_split() {
        // horizontal line 0..20 at y=0 with a branch going down from x=10
        let w = vec![("s".to_string(), vec![((0, 0), (20, 0)), ((10, 0), (10, 10))])];
        let j = junctions(&w, &HashSet::new(), 10);
        assert_eq!(j, vec![("s".to_string(), (10, 0))]);
        let ports: HashSet<Pt> = [(10, 0)].into();
        assert!(junctions(&w, &ports, 10).is_empty());
    }
}
