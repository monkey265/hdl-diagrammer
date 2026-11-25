use crate::geometry::{Pt, compress_polyline};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

pub type Bounds = (i32, i32, i32, i32); // xmin, xmax, ymin, ymax
pub type Occupancy = HashMap<Pt, bool>;
pub type WireOcc = HashMap<Pt, HashSet<String>>;

const BLOCKED: i64 = 1_000_000;

pub struct Router {
    pub grid_step: i32,
}

impl Router {
    pub fn build_occupancy(
        &self,
        blocks: &[(i32, i32, i32, i32)],
        b: Bounds,
        margin: i32,
    ) -> Occupancy {
        let (xmin, xmax, ymin, ymax) = b;
        let mut occ = Occupancy::new();
        for gx in (xmin..=xmax).step_by(self.grid_step as usize) {
            for gy in (ymin..=ymax).step_by(self.grid_step as usize) {
                let blocked = blocks.iter().any(|&(bx, by, bw, bh)| {
                    bx - margin <= gx
                        && gx <= bx + bw + margin
                        && by - margin <= gy
                        && gy <= by + bh + margin
                });
                occ.insert((gx, gy), blocked);
            }
        }
        occ
    }

    /// A* over grid cells; cells missing from `occ` count as blocked.
    pub fn find_path(
        &self,
        start: Pt,
        goal: Pt,
        occ: &Occupancy,
        wires: &WireOcc,
        sig: &str,
        b: Bounds,
    ) -> Option<Vec<Pt>> {
        let (xmin, xmax, ymin, ymax) = b;
        let h =
            |a: Pt| ((a.0 - goal.0).abs() + (a.1 - goal.1).abs()) as i64 / self.grid_step as i64; // cells, so A* stays admissible
        let cost = |c: Pt| -> i64 {
            if occ.get(&c).copied().unwrap_or(true) {
                return BLOCKED;
            }
            match wires.get(&c) {
                Some(s) if s.contains(sig) => 1,
                Some(s) => 1 + s.len() as i64 * 40,
                None => 1,
            }
        };
        let mut open = BinaryHeap::new();
        open.push(Reverse((h(start), 0i64, start)));
        let mut came: HashMap<Pt, Pt> = HashMap::new();
        let mut g_score: HashMap<Pt, i64> = HashMap::from([(start, 0)]);
        let mut closed: HashSet<Pt> = HashSet::new();
        while let Some(Reverse((_, g, cur))) = open.pop() {
            if !closed.insert(cur) {
                continue;
            }
            if cur == goal {
                let mut path = vec![cur];
                let mut c = cur;
                while let Some(&p) = came.get(&c) {
                    path.push(p);
                    c = p;
                }
                path.reverse();
                return Some(path);
            }
            let st = self.grid_step;
            for n in [
                (cur.0 + st, cur.1),
                (cur.0 - st, cur.1),
                (cur.0, cur.1 + st),
                (cur.0, cur.1 - st),
            ] {
                if n.0 < xmin || n.0 > xmax || n.1 < ymin || n.1 > ymax || closed.contains(&n) {
                    continue;
                }
                let c = cost(n);
                if c >= BLOCKED {
                    continue;
                }
                let t = g + c;
                if g_score.get(&n).is_none_or(|&old| t < old) {
                    came.insert(n, cur);
                    g_score.insert(n, t);
                    open.push(Reverse((t + h(n), t, n)));
                }
            }
        }
        None
    }

    /// Nearest unblocked cell to `p` (within 5 cells), or `p` itself if none.
    pub fn free_cell(&self, p: Pt, occ: &Occupancy) -> Pt {
        let free = |c: Pt| !occ.get(&c).copied().unwrap_or(true);
        if free(p) {
            return p;
        }
        let st = self.grid_step;
        for dist in 1..=5i32 {
            for dx in -dist..=dist {
                for dy in -dist..=dist {
                    if dx.abs().max(dy.abs()) != dist {
                        continue;
                    }
                    let c = (p.0 + dx * st, p.1 + dy * st);
                    if free(c) {
                        return c;
                    }
                }
            }
        }
        p
    }

    /// Full pin-to-pin polyline; never fails (falls back to an L-shape).
    pub fn route_net(
        &self,
        start: Pt,
        goal: Pt,
        occ: &Occupancy,
        wires: &WireOcc,
        sig: &str,
        b: Bounds,
    ) -> Vec<Pt> {
        let (s, g) = (self.free_cell(start, occ), self.free_cell(goal, occ));
        let mid = self
            .find_path(s, g, occ, wires, sig, b)
            .unwrap_or_else(|| vec![s, g]);
        let mut seq = vec![start];
        seq.extend(mid);
        seq.push(goal);
        let mut out = vec![seq[0]];
        for w in seq.windows(2) {
            let (a, c) = (w[0], w[1]);
            if a.0 != c.0 && a.1 != c.1 {
                out.push((c.0, a.1)); // elbow keeps every segment axis-aligned
            }
            out.push(c);
        }
        compress_polyline(&out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Bounds = (0, 200, 0, 100);
    fn r() -> Router {
        Router { grid_step: 10 }
    }

    #[test]
    fn find_path_around_block() {
        let occ = r().build_occupancy(&[(80, 20, 20, 60)], B, 0);
        let p = r()
            .find_path((20, 50), (160, 50), &occ, &WireOcc::new(), "a", B)
            .unwrap();
        assert_eq!((p[0], *p.last().unwrap()), ((20, 50), (160, 50)));
        assert!(p.iter().all(|c| !occ[c]));
    }

    #[test]
    fn find_path_reuses_same_signal() {
        let occ = r().build_occupancy(&[], B, 0);
        let mut w = WireOcc::new();
        for x in (0..=100).step_by(10) {
            w.entry((x, 0)).or_default().insert("a".into());
        }
        // from (0,50) to (100,50): detour via own wire at y=0 costs more than straight (1/cell), so straight wins
        let p = r().find_path((0, 50), (100, 50), &occ, &w, "a", B).unwrap();
        assert!(p.iter().all(|c| c.1 == 50));
        // but another signal's wire in the straight line is avoided
        let mut w2 = WireOcc::new();
        for x in (0..=100).step_by(10) {
            w2.entry((x, 50)).or_default().insert("other".into());
        }
        let p2 = r()
            .find_path((0, 50), (100, 50), &occ, &w2, "a", B)
            .unwrap();
        assert!(p2.iter().filter(|c| c.1 == 50).count() < 11);
    }

    #[test]
    fn no_path_returns_none() {
        // goal walled in by blocks
        let occ = r().build_occupancy(
            &[
                (60, 0, 10, 100),
                (60, 0, 140, 10),
                (60, 90, 140, 10),
                (150, 0, 10, 100),
            ],
            B,
            0,
        );
        let g = r().free_cell((100, 50), &occ);
        assert!(
            r().find_path((10, 50), g, &occ, &WireOcc::new(), "a", B)
                .is_none()
        );
    }

    #[test]
    fn start_inside_margin_still_routes() {
        let occ = r().build_occupancy(&[(80, 20, 40, 60)], B, 30);
        assert!(occ[&(80, 50)]); // pin on block edge is blocked by the margin
        let poly = r().route_net((80, 50), (180, 50), &occ, &WireOcc::new(), "a", B);
        assert_eq!((poly[0], *poly.last().unwrap()), ((80, 50), (180, 50)));
        // unreachable goal still yields a polyline instead of looping or panicking
        let poly2 = r().route_net((80, 50), (900, 900), &occ, &WireOcc::new(), "a", B);
        assert!(poly2.len() >= 2);
    }
}
