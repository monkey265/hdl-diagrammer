use super::Action;
use crate::model::{Diagram, Direction};
use crate::net::{Stub, Wire};
use eframe::egui;
use std::collections::BTreeMap;

/// Right-hand panel: every instance with its ports, plus a net list. Returns requested actions.
pub fn show(
    ui: &mut egui::Ui,
    d: &Diagram,
    wires: &[Wire],
    stubs: &[Stub],
    highlight: Option<&str>,
    filter: &mut String,
) -> Vec<Action> {
    let mut out = Vec::new();
    ui.add(egui::TextEdit::singleline(filter).hint_text("🔍 find block or net"));
    let q = filter.to_ascii_lowercase();
    let hit = |s: &str| q.is_empty() || s.to_ascii_lowercase().contains(&q);
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Instances");
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
                egui::CollapsingHeader::new(format!("{} ({})", inst.name, inst.entity))
                    .id_salt(i)
                    .show(ui, |ui| {
                        if ui.small_button("go to").clicked() {
                            out.push(Action::Focus(i));
                        }
                        for p in &inst.ports {
                            let arrow = match p.direction {
                                Direction::In => "→",
                                Direction::Out => "←",
                                Direction::InOut => "↔",
                            };
                            ui.label(format!(
                                "{arrow} {}: {}",
                                p.name,
                                if p.signal.is_empty() {
                                    "—"
                                } else {
                                    &p.signal
                                }
                            ));
                        }
                    });
            });
        }
        ui.separator();
        ui.heading("Nets");
        let mut nets: BTreeMap<&str, usize> = BTreeMap::new();
        for w in wires {
            *nets.entry(&w.signal).or_default() += 1;
        }
        for st in stubs {
            *nets.entry(&st.signal).or_default() += 1;
        }
        for (sig, n) in nets.into_iter().filter(|(s, _)| hit(s)) {
            let on = highlight == Some(sig);
            if ui.selectable_label(on, format!("{sig}  ({n})")).clicked() {
                out.push(Action::Highlight((!on).then(|| sig.to_string())));
            }
        }
    });
    out
}
