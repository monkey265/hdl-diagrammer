use std::f32::consts::FRAC_PI_2;

/// ANSI logic-gate symbols, chosen by entity name (letters only, case-insensitive: `AND2` -> AND).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateKind {
    And,
    Or,
    Nand,
    Nor,
    Xor,
    Xnor,
    Not,
    Buf,
}

pub fn gate_kind(entity: &str) -> Option<GateKind> {
    let letters: String = entity
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    Some(match letters.as_str() {
        "AND" => GateKind::And,
        "OR" => GateKind::Or,
        "NAND" => GateKind::Nand,
        "NOR" => GateKind::Nor,
        "XOR" => GateKind::Xor,
        "XNOR" => GateKind::Xnor,
        "NOT" => GateKind::Not,
        "BUF" => GateKind::Buf,
        _ => return None,
    })
}

pub struct GateShape {
    /// Outline in a unit box (x: 0 = left, 1 = body right edge; y: 0 = top, 1 = bottom).
    pub body: Vec<(f32, f32)>,
    /// Extra open curve (the XOR/XNOR input arc), same coordinates.
    pub extra: Option<Vec<(f32, f32)>>,
}

fn bezier(p0: (f32, f32), c: (f32, f32), p1: (f32, f32), n: usize) -> Vec<(f32, f32)> {
    (0..=n)
        .map(|k| {
            let t = k as f32 / n as f32;
            let u = 1.0 - t;
            (
                u * u * p0.0 + 2.0 * u * t * c.0 + t * t * p1.0,
                u * u * p0.1 + 2.0 * u * t * c.1 + t * t * p1.1,
            )
        })
        .collect()
}

fn or_body(back_shift: f32) -> Vec<(f32, f32)> {
    let mut pts = bezier((0.0, 0.0), (0.55, 0.0), (1.0, 0.5), 8);
    pts.extend(bezier((1.0, 0.5), (0.55, 1.0), (0.0, 1.0), 8));
    pts.extend(bezier((0.0, 1.0), (0.3, 0.5), (0.0, 0.0), 8));
    pts.iter()
        .map(|&(x, y)| (x + if x < 0.05 { back_shift } else { 0.0 }, y))
        .collect()
}

impl GateKind {
    /// Inverting gates get a small output bubble drawn after the body.
    pub fn bubble(self) -> bool {
        matches!(
            self,
            GateKind::Nand | GateKind::Nor | GateKind::Xnor | GateKind::Not
        )
    }

    pub fn shape(self) -> GateShape {
        match self {
            GateKind::And | GateKind::Nand => {
                let mut body = vec![(0.0, 0.0), (0.5, 0.0)];
                body.extend((1..9).map(|k| {
                    let t = -FRAC_PI_2 + std::f32::consts::PI * k as f32 / 9.0;
                    (0.5 + 0.5 * t.cos(), 0.5 + 0.5 * t.sin())
                }));
                body.extend([(0.5, 1.0), (0.0, 1.0)]);
                GateShape { body, extra: None }
            }
            GateKind::Or | GateKind::Nor => GateShape {
                body: or_body(0.0),
                extra: None,
            },
            GateKind::Xor | GateKind::Xnor => {
                let extra: Vec<_> = bezier((0.0, 1.0), (0.3, 0.5), (0.0, 0.0), 8)
                    .iter()
                    .map(|&(x, y)| (x - 0.12, y))
                    .collect();
                GateShape {
                    body: or_body(0.0),
                    extra: Some(extra),
                }
            }
            GateKind::Not | GateKind::Buf => GateShape {
                body: vec![(0.0, 0.0), (1.0, 0.5), (0.0, 1.0)],
                extra: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_kind_recognises_names() {
        assert_eq!(gate_kind("AND2"), Some(GateKind::And));
        assert_eq!(gate_kind("nand_3"), Some(GateKind::Nand));
        assert_eq!(gate_kind("Xnor"), Some(GateKind::Xnor));
        assert_eq!(gate_kind("BUF"), Some(GateKind::Buf));
        assert_eq!(gate_kind("uart_tx"), None);
        assert_eq!(gate_kind(""), None);
        assert_eq!(gate_kind("GROUP"), None);
    }

    #[test]
    fn shapes_stay_in_unit_box_and_bubbles_match() {
        for k in [
            GateKind::And,
            GateKind::Or,
            GateKind::Nand,
            GateKind::Nor,
            GateKind::Xor,
            GateKind::Xnor,
            GateKind::Not,
            GateKind::Buf,
        ] {
            let s = k.shape();
            assert!(s.body.len() >= 3);
            let pts = s.body.iter().chain(s.extra.iter().flatten());
            for &(x, y) in pts {
                assert!(
                    (-0.2..=1.0001).contains(&x) && (0.0..=1.0001).contains(&y),
                    "{k:?} ({x},{y})"
                );
            }
        }
        assert!(
            GateKind::Nand.bubble()
                && GateKind::Not.bubble()
                && !GateKind::And.bubble()
                && !GateKind::Buf.bubble()
        );
        assert!(GateKind::Xor.shape().extra.is_some() && GateKind::And.shape().extra.is_none());
    }
}
