use super::Action;
use crate::model::{Diagram, Direction};
use crate::net::Wire;
use eframe::egui;
use std::collections::BTreeMap;

/// Right-hand panel: every instance with its ports, plus a net list. Returns requested actions.
pub fn show(
    ui: &mut egui::Ui,
    d: &Diagram,
    wires: &[Wire],
    highlight: Option<&str>,
) -> Vec<Action> {
    let mut out = Vec::new();
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Instances");
        for (i, inst) in d.instances.iter().enumerate() {
            ui.horizontal(|ui| {
                let mut vis = inst.visible;
                if ui.checkbox(&mut vis, "").changed() {
                    out.push(Action::SetVisible(i, vis));
                }
                egui::CollapsingHeader::new(format!("{} ({})", inst.name, inst.entity))
                    .id_salt(i)
                    .show(ui, |ui| {
                        if ui.small_button("select").clicked() {
                            out.push(Action::Select(i));
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
        for (sig, n) in nets {
            let on = highlight == Some(sig);
            if ui.selectable_label(on, format!("{sig}  ({n})")).clicked() {
                out.push(Action::Highlight((!on).then(|| sig.to_string())));
            }
        }
    });
    out
}
