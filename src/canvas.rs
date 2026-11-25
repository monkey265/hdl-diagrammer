use crate::config::*;
use crate::geometry::Pt;
use crate::layout::port_pos;
use crate::model::{Diagram, Direction};
use crate::net::Wire;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::collections::HashMap;

/// Everything the canvas draws; owned by the app.
pub struct Scene<'a> {
    pub diagram: &'a Diagram,
    pub wires: &'a [Wire],
    pub junctions: &'a [(String, Pt)],
    pub pin_pos: &'a [Pt],
    pub signals: &'a HashMap<String, String>,
    pub grid: i32,
}

pub struct Canvas {
    pub zoom: f32,
    pub pan: Vec2,
    pub show_grid: bool,
    pub show_signal_names: bool,
    pub fit_requested: bool,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas { zoom: 1.0, pan: Vec2::ZERO, show_grid: true, show_signal_names: true, fit_requested: true }
    }
}

fn rgb(c: (u8, u8, u8)) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// `#rrggbb` (as written by the Python app's colour chooser) to a colour.
pub fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn signal_color(sig: &str, signals: &HashMap<String, String>) -> Color32 {
    if sig.eq_ignore_ascii_case("open") {
        rgb(UNDECLARED)
    } else if signals.contains_key(sig) {
        rgb(SIGNAL)
    } else {
        rgb(UNDECLARED)
    }
}

impl Canvas {
    fn world_bounds(scene: &Scene) -> Option<Rect> {
        let mut pts: Vec<Pos2> = scene.pin_pos.iter().map(|p| Pos2::new(p.0 as f32, p.1 as f32)).collect();
        for i in scene.diagram.instances.iter().filter(|i| i.visible) {
            pts.push(Pos2::new(i.x as f32, i.y as f32));
            pts.push(Pos2::new((i.x + i.width) as f32, (i.y + i.height) as f32));
        }
        (!pts.is_empty()).then(|| Rect::from_points(&pts).expand2(Vec2::new(140.0, 40.0)))
    }

    fn fit(&mut self, view: Rect, scene: &Scene) {
        let Some(b) = Self::world_bounds(scene) else { return };
        self.zoom = (view.width() / b.width()).min(view.height() / b.height()).clamp(0.1, 4.0);
        self.pan = view.size() / 2.0 - b.center().to_vec2() * self.zoom;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, scene: &Scene) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let view = resp.rect;
        if self.fit_requested {
            self.fit(Rect::from_min_size(Pos2::ZERO, view.size()), scene);
            self.fit_requested = false;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                if let Some(ptr) = ui.input(|i| i.pointer.hover_pos()) {
                    let local = ptr - view.min;
                    let world = (local - self.pan) / self.zoom;
                    self.zoom = (self.zoom * (scroll * 0.002).exp()).clamp(0.1, 8.0);
                    self.pan = local - world * self.zoom;
                }
            }
        }
        if resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Primary) {
            self.pan += resp.drag_delta();
        }
        let (z, origin) = (self.zoom, view.min + self.pan);
        let to_s = |p: Pt| origin + Vec2::new(p.0 as f32, p.1 as f32) * z;
        let font = |pt: f32| FontId::proportional((pt * z).max(1.0));
        let text_ok = |pt: f32| pt * z >= 5.0;

        painter.rect_filled(view, 0.0, Color32::WHITE);
        if self.show_grid {
            let step = scene.grid as f32 * z;
            if step >= 6.0 {
                let faint = Stroke::new(1.0, Color32::from_gray(235));
                let mut x = view.min.x + (self.pan.x).rem_euclid(step);
                while x < view.max.x {
                    painter.line_segment([Pos2::new(x, view.min.y), Pos2::new(x, view.max.y)], faint);
                    x += step;
                }
                let mut y = view.min.y + (self.pan.y).rem_euclid(step);
                while y < view.max.y {
                    painter.line_segment([Pos2::new(view.min.x, y), Pos2::new(view.max.x, y)], faint);
                    y += step;
                }
            }
        }

        // wires under blocks' labels but over the grid
        for w in scene.wires {
            let color = signal_color(&w.signal, scene.signals);
            for &(a, b) in &w.segments {
                painter.line_segment([to_s(a), to_s(b)], Stroke::new(1.5, color));
            }
            if self.show_signal_names && text_ok(8.0) {
                if let Some(&(a, b)) = w.segments.iter().max_by_key(|((x1, y1), (x2, y2))| (x2 - x1).abs() + (y2 - y1).abs()) {
                    let mid = ((a.0 + b.0) / 2, (a.1 + b.1) / 2);
                    painter.text(to_s(mid) - Vec2::new(0.0, 2.0 * z), Align2::CENTER_BOTTOM, &w.signal, font(8.0), color);
                }
            }
        }
        for (sig, p) in scene.junctions {
            painter.circle_filled(to_s(*p), 3.0 * z, signal_color(sig, scene.signals));
        }

        for inst in scene.diagram.instances.iter().filter(|i| i.visible) {
            let r = Rect::from_min_max(to_s((inst.x, inst.y)), to_s((inst.x + inst.width, inst.y + inst.height)));
            let fill = inst.color_override.as_deref().and_then(parse_hex).unwrap_or(rgb(INSTANCE_BG));
            painter.rect_filled(r, 4.0 * z, fill);
            painter.rect_stroke(r, 4.0 * z, Stroke::new(1.5, Color32::from_gray(60)), StrokeKind::Inside);
            if text_ok(inst.font_size as f32) {
                painter.text(r.center_top() + Vec2::new(0.0, 6.0 * z), Align2::CENTER_TOP, &inst.name, font(inst.font_size as f32), Color32::BLACK);
                painter.text(r.center_top() + Vec2::new(0.0, (inst.font_size as f32 + 10.0) * z), Align2::CENTER_TOP, format!("({})", inst.entity), font(8.0), Color32::DARK_GRAY);
            }
            for (pi, p) in inst.ports.iter().enumerate() {
                let pos = to_s(port_pos(inst, pi));
                let left = p.direction == Direction::In;
                let c = if left { rgb(INPUT_PORT) } else { rgb(OUTPUT_PORT) };
                painter.circle_filled(pos, 3.5 * z, c);
                if text_ok(p.font_size as f32) {
                    let (anchor, dx) = if left { (Align2::LEFT_CENTER, 6.0) } else { (Align2::RIGHT_CENTER, -6.0) };
                    painter.text(pos + Vec2::new(dx * z, 0.0), anchor, &p.name, font(p.font_size as f32), Color32::BLACK);
                }
            }
        }

        for (pin, pos) in scene.diagram.top_level_pins.iter().zip(scene.pin_pos) {
            let c = to_s(*pos);
            let col = if pin.direction == Direction::In { rgb(INPUT_PORT) } else { rgb(OUTPUT_PORT) };
            painter.rect_filled(Rect::from_center_size(c, Vec2::splat(8.0 * z)), 1.0, col);
            if text_ok(8.0) {
                let (anchor, dx) = if pin.direction == Direction::In { (Align2::RIGHT_CENTER, -8.0) } else { (Align2::LEFT_CENTER, 8.0) };
                painter.text(c + Vec2::new(dx * z, 0.0), anchor, &pin.name, font(9.0), Color32::BLACK);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_accepts_python_colours() {
        assert_eq!(parse_hex("#ff0000"), Some(Color32::from_rgb(255, 0, 0)));
        assert_eq!(parse_hex("#e8f4f8"), Some(Color32::from_rgb(0xe8, 0xf4, 0xf8)));
        assert_eq!(parse_hex("red"), None);
        assert_eq!(parse_hex("#12"), None);
    }
}
