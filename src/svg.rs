use crate::config::*;
use crate::gates::GateKind;
use crate::gates::gate_kind;
use crate::group::active_instances;
use crate::layout::port_pos;
use crate::model::{Diagram, Direction, Instance};
use crate::net::{net_class, pin_connected, route_all_opts, top_pin_positions, wire_junctions};
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

fn gate_svg(s: &mut String, b: &Instance, kind: GateKind, fill: &str) {
    let body_w = b.width as f32 - if kind.bubble() { 10.0 } else { 0.0 };
    let pts = |v: &[(f32, f32)]| {
        v.iter()
            .map(|&(px, py)| {
                format!(
                    "{:.1},{:.1}",
                    b.x as f32 + px * body_w,
                    b.y as f32 + py * b.height as f32
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let shape = kind.shape();
    let _ = writeln!(
        s,
        r##"<polygon points="{}" fill="{}" stroke="#333333" stroke-width="1.5"/>"##,
        pts(&shape.body),
        esc(fill)
    );
    if let Some(extra) = &shape.extra {
        let _ = writeln!(
            s,
            r##"<polyline points="{}" fill="none" stroke="#333333" stroke-width="1.5"/>"##,
            pts(extra)
        );
    }
    if kind.bubble() {
        let _ = writeln!(
            s,
            r##"<circle cx="{}" cy="{}" r="5" fill="#ffffff" stroke="#333333" stroke-width="1.5"/>"##,
            b.x + b.width - 5,
            b.y + b.height / 2
        );
    }
}

/// Write `d` to `path`: PNG (2x) when the extension is `.png`, otherwise SVG.
pub fn write(
    path: &std::path::Path,
    d: &Diagram,
    signals: &HashMap<String, String>,
) -> Result<(), String> {
    let svg = render(d, signals);
    let err = |e: &dyn std::fmt::Display| format!("cannot write {}: {e}", path.display());
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    {
        return std::fs::write(path, svg).map_err(|e| err(&e));
    }
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(&svg, &opt).map_err(|e| err(&e))?;
    let size = tree.size();
    let mut pix =
        resvg::tiny_skia::Pixmap::new((size.width() * 2.0) as u32, (size.height() * 2.0) as u32)
            .ok_or_else(|| err(&"diagram too large"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(2.0, 2.0),
        &mut pix.as_mut(),
    );
    pix.save_png(path).map_err(|e| err(&e))
}

/// Static picture of the diagram: blocks, ports, routed wires, junctions and top-level pins.
/// Gates (AND, OR, NAND, …) get their ANSI symbol.
pub fn render(d: &Diagram, signals: &HashMap<String, String>) -> String {
    let routing = route_all_opts(d, true);
    let (wires, stubs) = (routing.wires, routing.stubs);
    let linked = pin_connected(d);
    let pins = top_pin_positions(d);
    let junctions = wire_junctions(d, &wires);
    let blocks = active_instances(d);

    let mut pts: Vec<(i32, i32)> = pins.clone();
    for b in &blocks {
        pts.push((b.x, b.y));
        pts.push((b.x + b.width, b.y + b.height));
    }
    for w in &wires {
        for &(a, b) in &w.segments {
            pts.extend([a, b]);
        }
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
        let (rgb, width) = class_style(net_class(&wire.signal, signals), false);
        let c = hex(rgb);
        for ((ax, ay), (bx, by)) in &wire.segments {
            let _ = writeln!(
                s,
                r#"<line x1="{ax}" y1="{ay}" x2="{bx}" y2="{by}" stroke="{c}" stroke-width="{width}"><title>{}</title></line>"#,
                esc(&wire.signal)
            );
        }
    }
    for (sig, (x, y)) in &junctions {
        let _ = writeln!(
            s,
            r#"<circle cx="{x}" cy="{y}" r="3" fill="{}"/>"#,
            hex(class_style(net_class(sig, signals), false).0)
        );
    }
    for st in &stubs {
        let c = hex(class_style(net_class(&st.signal, signals), false).0);
        let (x, y) = st.pos;
        let e = x + st.dir * 24;
        let anchor = if st.dir > 0 { "start" } else { "end" };
        let _ = writeln!(
            s,
            r#"<line x1="{x}" y1="{y}" x2="{e}" y2="{y}" stroke="{c}" stroke-width="1.5"/><text x="{}" y="{}" font-size="8" fill="{c}" text-anchor="{anchor}">{}</text>"#,
            e + st.dir * 2,
            y + 3,
            esc(&st.signal)
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
        if let Some(kind) = gate_kind(&b.entity) {
            gate_svg(&mut s, b, kind, &fill);
        } else {
            let _ = writeln!(
                s,
                r##"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="#333333" stroke-width="1.5"{dash}/>"##,
                b.x,
                b.y,
                b.width,
                b.height,
                esc(&fill)
            );
        }
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
    for (k, (p, &(px, py))) in d.top_level_pins.iter().zip(&pins).enumerate() {
        let (tx, anchor) = if p.direction == Direction::In {
            (px - 8, "end")
        } else {
            (px + 8, "start")
        };
        let _ = writeln!(
            s,
            r#"<circle cx="{px}" cy="{py}" r="4" fill="{}" stroke="{}" opacity="{}"/><text x="{tx}" y="{}" font-size="9" text-anchor="{anchor}" opacity="{}">{}</text>"#,
            if linked[k] {
                port_colour(p.direction)
            } else {
                "#ffffff".into()
            },
            port_colour(p.direction),
            if linked[k] { 1.0 } else { 0.5 },
            py + 3,
            if linked[k] { 1.0 } else { 0.5 },
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
        assert_eq!(svg.matches("stroke=\"#333333\"").count(), 2); // one rect per block
        assert!(svg.contains("<line "), "wires drawn");
        let vb: Vec<i32> = svg
            .split("viewBox=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .split(' ')
            .map(|v| v.parse().unwrap())
            .collect();
        let w = route_all_opts(&d, true).wires;
        assert!(
            w.iter()
                .flat_map(|w| &w.segments)
                .all(|&((ax, ay), (bx, by))| {
                    [ax, bx].iter().all(|x| (vb[0]..=vb[0] + vb[2]).contains(x))
                        && [ay, by].iter().all(|y| (vb[1]..=vb[1] + vb[3]).contains(y))
                }),
            "viewBox contains every wire"
        );
        assert!(svg.contains("a&lt;&amp;&gt;b") && !svg.contains("a<&>b"));
    }

    #[test]
    fn gates_are_polygons_and_png_export_writes_a_png() {
        let src = "entity t is port (a, b : in std_logic; q : out std_logic); end entity;
architecture x of t is begin
 g : entity work.NAND2 port map (i0 => a, i1 => b, o => q);
end;";
        let des = parse_vhdl(src);
        let d = design_to_diagram(&des, 20);
        let svg = render(&d, &des.signals);
        assert!(svg.contains("<polygon") && svg.contains("r=\"5\""));
        let f = std::env::temp_dir().join(format!("hdl_{}.png", std::process::id()));
        write(&f, &d, &des.signals).unwrap();
        assert_eq!(&std::fs::read(&f).unwrap()[1..4], b"PNG");
        std::fs::remove_file(f).ok();
    }
}
