use crate::config::*;
use crate::geometry::Pt;
use crate::layout::port_pos;
use crate::edit;
use crate::model::{Diagram, Direction};
use crate::net::Wire;
use crate::ui::Action;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::collections::HashMap;

/// What the canvas draws; the diagram is mutable because dragging moves blocks.
pub struct View<'a> {
    pub diagram: &'a mut Diagram,
    pub wires: &'a [Wire],
    pub junctions: &'a [(String, Pt)],
    pub pin_pos: &'a [Pt],
    pub signals: &'a HashMap<String, String>,
    pub grid: i32,
}

/// What a frame of canvas interaction asks the app to do.
pub enum Outcome {
    /// A drag finished; carries the diagram as it was before the drag (for undo).
    Edited(Diagram),
    Do(Action),
}

enum Target {
    Block(usize),
    Port(usize, usize),
}

struct Drag {
    before: Diagram,
    start: (f32, f32),
    origins: Vec<(usize, i32, i32)>,
    moved: bool,
}

pub struct Canvas {
    pub zoom: f32,
    pub pan: Vec2,
    pub show_grid: bool,
    pub show_signal_names: bool,
    pub fit_requested: bool,
    pub selected: Vec<usize>,
    pub highlight: Option<String>,
    drag: Option<Drag>,
    panning: bool,
    target: Option<Target>,
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas {
            zoom: 1.0, pan: Vec2::ZERO, show_grid: true, show_signal_names: true, fit_requested: true,
            selected: vec![], highlight: None, drag: None, panning: false, target: None,
        }
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
    fn world_bounds(scene: &View) -> Option<Rect> {
        let mut pts: Vec<Pos2> = scene.pin_pos.iter().map(|p| Pos2::new(p.0 as f32, p.1 as f32)).collect();
        for i in scene.diagram.instances.iter().filter(|i| i.visible) {
            pts.push(Pos2::new(i.x as f32, i.y as f32));
            pts.push(Pos2::new((i.x + i.width) as f32, (i.y + i.height) as f32));
        }
        (!pts.is_empty()).then(|| Rect::from_points(&pts).expand2(Vec2::new(140.0, 40.0)))
    }

    fn fit(&mut self, view: Rect, scene: &View) {
        let Some(b) = Self::world_bounds(scene) else { return };
        self.zoom = (view.width() / b.width()).min(view.height() / b.height()).clamp(0.1, 4.0);
        self.pan = view.size() / 2.0 - b.center().to_vec2() * self.zoom;
    }

    pub fn show(&mut self, ui: &mut egui::Ui, scene: &mut View) -> Vec<Outcome> {
        let mut out = Vec::new();
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let view = resp.rect;
        let n = scene.diagram.instances.len();
        self.selected.retain(|&i| i < n);
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
        let to_w = |p: Pos2, pan: Vec2, zoom: f32| -> (f32, f32) {
            let v = (p - view.min - pan) / zoom;
            (v.x, v.y)
        };
        let shift = ui.input(|i| i.modifiers.shift);
        let ptr_world = resp.interact_pointer_pos().map(|p| to_w(p, self.pan, self.zoom));

        if resp.drag_started_by(egui::PointerButton::Primary) {
            let hit = ptr_world.and_then(|w| edit::hit_block(scene.diagram, w));
            match (hit, ptr_world) {
                (Some(i), Some(w)) => {
                    if !self.selected.contains(&i) {
                        if !shift {
                            self.selected.clear();
                        }
                        self.selected.push(i);
                    }
                    let origins = self.selected.iter().map(|&k| (k, scene.diagram.instances[k].x, scene.diagram.instances[k].y)).collect();
                    self.drag = Some(Drag { before: scene.diagram.clone(), start: w, origins, moved: false });
                }
                _ => self.panning = true,
            }
        }
        if resp.dragged_by(egui::PointerButton::Middle) || (self.panning && resp.dragged_by(egui::PointerButton::Primary)) {
            self.pan += resp.drag_delta();
        }
        if let (Some(drag), Some(w)) = (self.drag.as_mut(), ptr_world) {
            if resp.dragged_by(egui::PointerButton::Primary) {
                let (dx, dy) = ((w.0 - drag.start.0) as i32, (w.1 - drag.start.1) as i32);
                for &(k, ox, oy) in &drag.origins {
                    edit::move_block(scene.diagram, k, ox + dx, oy + dy, scene.grid);
                }
                drag.moved |= dx != 0 || dy != 0;
            }
        }
        if resp.drag_stopped() {
            self.panning = false;
            if let Some(drag) = self.drag.take().filter(|d| d.moved) {
                for &(k, _, _) in &drag.origins {
                    edit::resolve_collision(scene.diagram, k, scene.grid);
                }
                out.push(Outcome::Edited(drag.before));
            }
        }
        if resp.clicked() {
            if let Some(w) = ptr_world {
                if let Some(i) = edit::hit_block(scene.diagram, w) {
                    if shift {
                        match self.selected.iter().position(|&k| k == i) {
                            Some(at) => drop(self.selected.remove(at)),
                            None => self.selected.push(i),
                        }
                    } else {
                        self.selected = vec![i];
                    }
                } else {
                    let hit = edit::hit_wire(scene.wires, w, 4.0 / self.zoom).map(|k| scene.wires[k].signal.clone());
                    self.highlight = hit;
                    self.selected.clear();
                }
            }
        }
        if resp.secondary_clicked() {
            self.target = ptr_world.and_then(|w| {
                edit::hit_port(scene.diagram, w, 8.0 / self.zoom.min(1.0))
                    .map(|(i, p)| Target::Port(i, p))
                    .or_else(|| edit::hit_block(scene.diagram, w).map(Target::Block))
            });
        }
        resp.context_menu(|ui| match &self.target {
            Some(Target::Port(i, p)) => {
                if ui.button("Rename port…").clicked() {
                    out.push(Outcome::Do(Action::RenamePort(*i, *p)));
                    ui.close();
                }
                if ui.button("Delete port").clicked() {
                    out.push(Outcome::Do(Action::DeletePort(*i, *p)));
                    ui.close();
                }
            }
            Some(Target::Block(i)) => {
                let i = *i;
                let inst = &scene.diagram.instances[i];
                if ui.button("Rename…").clicked() {
                    out.push(Outcome::Do(Action::RenameInstance(i)));
                    ui.close();
                }
                ui.menu_button("Colour", |ui| {
                    for hex in ["#ffcdd2", "#fff9c4", "#c8e6c9", "#bbdefb", "#e1bee7", "#ffe0b2", "#cfd8dc"] {
                        let c = parse_hex(hex).unwrap_or(Color32::WHITE);
                        if ui.add(egui::Button::new("      ").fill(c)).clicked() {
                            out.push(Outcome::Do(Action::Color(i, Some(hex.to_string()))));
                            ui.close();
                        }
                    }
                    if ui.button("Reset").clicked() {
                        out.push(Outcome::Do(Action::Color(i, None)));
                        ui.close();
                    }
                });
                if ui.button("Set size…").clicked() {
                    out.push(Outcome::Do(Action::SetSize(i)));
                    ui.close();
                }
                if ui.button(if inst.locked { "Unlock" } else { "Lock" }).clicked() {
                    out.push(Outcome::Do(Action::ToggleLock(i)));
                    ui.close();
                }
                if ui.button("Reset ports").clicked() {
                    out.push(Outcome::Do(Action::ResetPorts(i)));
                    ui.close();
                }
                if ui.button("Delete").clicked() {
                    out.push(Outcome::Do(Action::SetVisible(i, false)));
                    ui.close();
                }
            }
            None => {
                if ui.button("Zoom to fit").clicked() {
                    self.fit_requested = true;
                    ui.close();
                }
            }
        });

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
            let hl = self.highlight.as_deref() == Some(w.signal.as_str());
            let color = if hl { rgb(HIGHLIGHT) } else { signal_color(&w.signal, scene.signals) };
            for &(a, b) in &w.segments {
                painter.line_segment([to_s(a), to_s(b)], Stroke::new(if hl { 3.0 } else { 1.5 }, color));
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

        for (ii, inst) in scene.diagram.instances.iter().enumerate().filter(|(_, i)| i.visible) {
            let r = Rect::from_min_max(to_s((inst.x, inst.y)), to_s((inst.x + inst.width, inst.y + inst.height)));
            let fill = inst.color_override.as_deref().and_then(parse_hex).unwrap_or(rgb(INSTANCE_BG));
            painter.rect_filled(r, 4.0 * z, fill);
            let sel = self.selected.contains(&ii);
            painter.rect_stroke(r, 4.0 * z, Stroke::new(if sel { 3.0 } else { 1.5 }, if sel { rgb(HIGHLIGHT) } else { Color32::from_gray(60) }), StrokeKind::Inside);
            if inst.locked && text_ok(8.0) {
                painter.text(r.right_top() + Vec2::new(-4.0 * z, 3.0 * z), Align2::RIGHT_TOP, "locked", font(7.0), Color32::GRAY);
            }
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
        out
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
