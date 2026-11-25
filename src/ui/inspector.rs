use super::Action;
use crate::model::{Diagram, Direction};
use crate::net::{Stub, Wire};
use eframe::egui::{self, Color32, RichText};
use std::collections::BTreeMap;

/// Blue title strip on top of a pane, like Questa's window headers.
pub fn title(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgb(0x24, 0x3B, 0x5A))
        .inner_margin(egui::Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).strong().color(Color32::WHITE));
        });
}

fn arrow(d: Direction) -> &'static str {
    match d {
        Direction::In => "→",
        Direction::Out => "←",
        Direction::InOut => "↔",
    }
}

/// Left pane: the design hierarchy. Click a block to select it and pan to it.
pub fn structure(
    ui: &mut egui::Ui,
    d: &Diagram,
    selected: &[usize],
    filter: &mut String,
) -> Vec<Action> {
    let mut out = Vec::new();
    title(ui, "Structure");
    ui.add(egui::TextEdit::singleline(filter).hint_text("🔍 find block or net"));
    let q = filter.to_ascii_lowercase();
    let hit = |s: &str| q.is_empty() || s.to_ascii_lowercase().contains(&q);
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, inst) in d.instances.iter().enumerate() {
            let in_ports = inst.ports.iter().any(|p| hit(&p.name) || hit(&p.signal));
            if !(hit(&inst.name) || hit(&inst.entity) || in_ports) {
                continue;
            }
            ui.horizontal(|ui| {
                let mut vis = inst.visible;
                if ui.checkbox(&mut vis, "").changed() {
                    out.push(Action::SetVisible(i, vis));
                }
                let label = format!("{} ({})", inst.name, inst.entity);
                if ui.selectable_label(selected.contains(&i), label).clicked() {
                    out.push(Action::Focus(i));
                }
            });
            for (c, child) in inst.children.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    ui.label(format!("▪ {} ({})", child.name, child.entity));
                    if ui.small_button("release").clicked() {
                        out.push(Action::ReleaseChild(i, c));
                    }
                });
            }
        }
    });
    out
}

/// Right pane: ports of the selected blocks, then the net list.
pub fn objects(
    ui: &mut egui::Ui,
    d: &Diagram,
    selected: &[usize],
    wires: &[Wire],
    stubs: &[Stub],
    highlight: Option<&str>,
    filter: &str,
) -> Vec<Action> {
    let mut out = Vec::new();
    title(ui, "Objects");
    let q = filter.to_ascii_lowercase();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for inst in selected.iter().filter_map(|&i| d.instances.get(i)) {
            ui.label(RichText::new(&inst.name).strong());
            egui::Grid::new(("ports", &inst.name))
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    for p in &inst.ports {
                        ui.monospace(arrow(p.direction));
                        ui.monospace(&p.name);
                        let sig = if p.signal.is_empty() {
                            "—"
                        } else {
                            &p.signal
                        };
                        ui.monospace(sig);
                        ui.end_row();
                    }
                });
            ui.separator();
        }
        if selected.is_empty() {
            ui.weak("Select a block to list its ports.");
            ui.separator();
        }
        ui.label(RichText::new("Nets").strong());
        let mut nets: BTreeMap<&str, usize> = BTreeMap::new();
        for w in wires {
            *nets.entry(&w.signal).or_default() += 1;
        }
        for st in stubs {
            *nets.entry(&st.signal).or_default() += 1;
        }
        for (sig, n) in nets
            .into_iter()
            .filter(|(s, _)| q.is_empty() || s.to_ascii_lowercase().contains(&q))
        {
            let on = highlight == Some(sig);
            if ui.selectable_label(on, format!("{sig}  ({n})")).clicked() {
                out.push(Action::Highlight((!on).then(|| sig.to_string())));
            }
        }
    });
    out
}
