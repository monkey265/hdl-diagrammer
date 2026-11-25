use crate::config::*;
use crate::group::active_instances;
use crate::layout::port_pos;
use crate::model::{Diagram, Direction};
use crate::net::{route_all, top_pin_positions, wire_junctions};
use std::collections::HashMap;
use std::fmt::Write;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn port_colour(d: Direction) -> String {
    hex(if d == Direction::In {
        INPUT_PORT
    } else {
        OUTPUT_PORT
    })
}

/// Static picture of the diagram: blocks, ports, routed wires, junctions and top-level pins.
/// Gates are drawn as plain boxes (the ANSI symbols are a GUI-only nicety).
pub fn render(d: &Diagram, signals: &HashMap<String, String>) -> String {
    let wires = route_all(d);
    let pins = top_pin_positions(d);
    let junctions = wire_junctions(d, &wires);
    let blocks = active_instances(d);

    let mut pts: Vec<(i32, i32)> = pins.clone();
    for b in &blocks {
        pts.push((b.x, b.y));
        pts.push((b.x + b.width, b.y + b.height));
    }
    let (x0, y0, x1, y1) = pts.iter().fold((0, 0, 0, 0), |a, p| {
        (a.0.min(p.0), a.1.min(p.1), a.2.max(p.0), a.3.max(p.1))
    });
    let (x0, y0) = (x0 - 150, y0 - 60);
    let (w, h) = (x1 + 150 - x0, y1 + 60 - y0);

    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="{x0} {y0} {w} {h}" font-family="Arial, sans-serif">"#
    );
    let _ = writeln!(
        s,
        r##"<rect x="{x0}" y="{y0}" width="{w}" height="{h}" fill="#ffffff"/>"##
    );
    for wire in &wires {
        let c = hex(if signals.contains_key(&wire.signal) {
            SIGNAL
        } else {
            UNDECLARED
        });
        for ((ax, ay), (bx, by)) in &wire.segments {
            let _ = writeln!(
                s,
                r#"<line x1="{ax}" y1="{ay}" x2="{bx}" y2="{by}" stroke="{c}" stroke-width="2"><title>{}</title></line>"#,
                esc(&wire.signal)
            );
        }
    }
    for (_, (x, y)) in &junctions {
        let _ = writeln!(
            s,
            r#"<circle cx="{x}" cy="{y}" r="3" fill="{}"/>"#,
            hex(SIGNAL)
        );
    }
    for b in &blocks {
        let fill = if b.is_group && !b.collapsed {
            "none".to_string()
        } else {
            b.color_override.clone().unwrap_or_else(|| hex(INSTANCE_BG))
        };
        let dash = if b.is_group {
            r#" stroke-dasharray="6 4""#
        } else {
            ""
        };
        let _ = writeln!(
            s,
            r##"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="#333333" stroke-width="1.5"{dash}/>"##,
            b.x,
            b.y,
            b.width,
            b.height,
            esc(&fill)
        );
        let _ = writeln!(
            s,
            r#"<text x="{}" y="{}" font-size="{}" font-weight="bold" text-anchor="middle">{}</text>"#,
            b.x + b.width / 2,
            b.y + 16,
            b.font_size,
            esc(&b.name)
        );
        let _ = writeln!(
            s,
            r##"<text x="{}" y="{}" font-size="8" fill="#666666" text-anchor="middle">{}</text>"##,
            b.x + b.width / 2,
            b.y + 28,
            esc(&b.entity)
        );
        for (k, p) in b.ports.iter().enumerate() {
            let (px, py) = port_pos(b, k);
            let left = p.direction == Direction::In;
            let (tx, anchor) = if left {
                (px + 6, "start")
            } else {
                (px - 6, "end")
            };
            let _ = writeln!(
                s,
                r#"<circle cx="{px}" cy="{py}" r="3" fill="{}"/><text x="{tx}" y="{}" font-size="{}" text-anchor="{anchor}">{}</text>"#,
                port_colour(p.direction),
                py + 3,
                p.font_size,
                esc(&p.name)
            );
        }
    }
    for (p, &(px, py)) in d.top_level_pins.iter().zip(&pins) {
        let (tx, anchor) = if p.direction == Direction::In {
            (px - 8, "end")
        } else {
            (px + 8, "start")
        };
        let _ = writeln!(
            s,
            r#"<circle cx="{px}" cy="{py}" r="4" fill="{}"/><text x="{tx}" y="{}" font-size="9" text-anchor="{anchor}">{}</text>"#,
            port_colour(p.direction),
            py + 3,
            esc(&p.name)
        );
    }
    s.push_str("</svg>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::design_to_diagram;
    use crate::parser::vhdl::parse_vhdl;

    #[test]
    fn svg_has_block_per_instance_wires_and_escapes_names() {
        let src = "entity t is port (a : in std_logic; q : out std_logic); end entity;
architecture x of t is signal m : std_logic; begin
 u1 : entity work.f port map (i => a, o => m);
 u2 : entity work.g port map (i => m, o => q);
end;";
        let des = parse_vhdl(src);
        let mut d = design_to_diagram(&des, 20);
        d.instances[0].name = "a<&>b".into();
        let svg = render(&d, &des.signals);
        assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
        assert_eq!(svg.matches("stroke-width=\"1.5\"").count(), 2); // one rect per block
        assert!(svg.contains("<line "), "wires drawn");
        assert!(svg.contains("a&lt;&amp;&gt;b") && !svg.contains("a<&>b"));
    }
}
