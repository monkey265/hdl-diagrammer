use crate::canvas::{Canvas, Outcome, View};
use crate::config::*;
use crate::edit;
use crate::geometry::Pt;
use crate::history::History;
use crate::model::Diagram;
use crate::net::{design_to_diagram, route_all, top_pin_positions, wire_junctions, Wire};
use crate::parser::parse_file;
use crate::recent;
use crate::ui::{inspector, Action};
use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

enum PromptKind {
    RenameInstance(usize),
    RenamePort(usize, usize),
    SetSize(usize),
}

struct Prompt {
    kind: PromptKind,
    title: &'static str,
    text: String,
}

pub struct App {
    pub diagram: Diagram,
    pub signals: HashMap<String, String>,
    pub warnings: Vec<String>,
    pub path: Option<PathBuf>,
    wires: Vec<Wire>,
    junctions: Vec<(String, Pt)>,
    pin_pos: Vec<Pt>,
    canvas: Canvas,
    grid: i32,
    history: History<Diagram>,
    prompt: Option<Prompt>,
    recent: Vec<String>,
}

impl Default for App {
    fn default() -> Self {
        App {
            diagram: Diagram::default(),
            signals: HashMap::new(),
            warnings: vec![],
            path: None,
            wires: vec![],
            junctions: vec![],
            pin_pos: vec![],
            canvas: Canvas::default(),
            grid: DEFAULT_GRID,
            history: History::default(),
            prompt: None,
            recent: recent::load(),
        }
    }
}

impl App {
    /// Load a VHDL/SystemVerilog source (parsed and laid out) or a saved `.json` diagram.
    pub fn open_file(&mut self, path: &Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.warnings = vec![format!("cannot read {}: {e}", path.display())];
                return;
            }
        };
        let name = path.to_string_lossy().into_owned();
        if name.ends_with(".json") {
            match Diagram::from_json(&text) {
                Ok(d) => (self.diagram, self.signals, self.warnings) = (d, HashMap::new(), vec![]),
                Err(e) => {
                    self.warnings = vec![format!("bad diagram file: {e}")];
                    return;
                }
            }
        } else {
            let design = parse_file(&name, &text);
            self.diagram = design_to_diagram(&design, self.grid);
            (self.signals, self.warnings) = (design.signals, design.warnings);
        }
        self.path = Some(path.to_path_buf());
        self.history.clear();
        self.canvas.selected.clear();
        self.canvas.highlight = None;
        recent::add(&mut self.recent, &name);
        recent::save(&self.recent);
        self.refresh();
        self.canvas.fit_requested = true;
    }

    fn save_as(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("Diagram", &["json"]).set_file_name("diagram.json").save_file() else { return };
        if let Err(e) = std::fs::write(&p, self.diagram.to_json()) {
            self.warnings = vec![format!("cannot save {}: {e}", p.display())];
        }
    }

    /// Re-route after any change to the diagram.
    pub fn refresh(&mut self) {
        self.pin_pos = top_pin_positions(&self.diagram);
        self.wires = route_all(&self.diagram);
        self.junctions = wire_junctions(&self.diagram, &self.wires);
    }

    /// Apply `f`, record the previous state for undo, re-route.
    fn edit(&mut self, f: impl FnOnce(&mut Diagram)) {
        let before = self.diagram.clone();
        f(&mut self.diagram);
        self.history.push(&before);
        self.refresh();
    }

    fn undo(&mut self) {
        if let Some(prev) = self.history.undo(&self.diagram) {
            self.diagram = prev;
            self.refresh();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.history.redo(&self.diagram) {
            self.diagram = next;
            self.refresh();
        }
    }

    fn apply(&mut self, a: Action) {
        match a {
            Action::RenameInstance(i) => {
                let text = self.diagram.instances[i].name.clone();
                self.prompt = Some(Prompt { kind: PromptKind::RenameInstance(i), title: "Rename instance", text });
            }
            Action::RenamePort(i, p) => {
                let text = self.diagram.instances[i].ports[p].name.clone();
                self.prompt = Some(Prompt { kind: PromptKind::RenamePort(i, p), title: "Rename port", text });
            }
            Action::SetSize(i) => {
                let n = &self.diagram.instances[i];
                let text = if n.custom_width > 0 { format!("{}x{}", n.custom_width, n.custom_height) } else { String::new() };
                self.prompt = Some(Prompt { kind: PromptKind::SetSize(i), title: "Size WxH (empty = auto)", text });
            }
            Action::Color(i, c) => self.edit(|d| d.instances[i].color_override = c),
            Action::ToggleLock(i) => self.edit(|d| edit::toggle_lock(d, i)),
            Action::SetVisible(i, v) => self.edit(|d| edit::set_visible(d, i, v)),
            Action::ResetPorts(i) => self.edit(|d| edit::reset_ports(d, i)),
            Action::DeletePort(i, p) => self.edit(|d| edit::delete_port(d, i, p)),
            Action::Highlight(s) => self.canvas.highlight = s,
            Action::Select(i) => self.canvas.selected = vec![i],
        }
    }

    fn finish_prompt(&mut self, p: Prompt) {
        let g = self.grid;
        let t = p.text.trim().to_string();
        match p.kind {
            PromptKind::RenameInstance(i) if !t.is_empty() => self.edit(|d| d.instances[i].name = t),
            PromptKind::RenamePort(i, k) if !t.is_empty() => self.edit(|d| d.instances[i].ports[k].name = t),
            PromptKind::SetSize(i) => {
                let size = t.split_once(['x', 'X']).and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?)));
                if t.is_empty() || size.is_some() {
                    self.edit(|d| edit::set_size(d, i, size, g));
                } else {
                    self.warnings = vec![format!("size '{t}' is not WxH")];
                }
            }
            _ => {}
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let (undo, redo) = ui.input_mut(|i| {
            let redo = i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                || i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
            (i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z), redo)
        });
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        let mut actions: Vec<Action> = Vec::new();
        let mut open: Option<PathBuf> = None;
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open…").clicked() {
                        ui.close();
                        open = rfd::FileDialog::new()
                            .add_filter("HDL or diagram", &["vhd", "vhdl", "sv", "v", "svh", "json"])
                            .pick_file();
                    }
                    ui.menu_button("Open Recent", |ui| {
                        for r in &self.recent {
                            if ui.button(r).clicked() {
                                open = Some(PathBuf::from(r));
                                ui.close();
                            }
                        }
                    });
                    if ui.button("Save As…").clicked() {
                        ui.close();
                        self.save_as();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Undo (Ctrl+Z)").clicked() {
                        self.undo();
                        ui.close();
                    }
                    if ui.button("Redo (Ctrl+Y)").clicked() {
                        self.redo();
                        ui.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.canvas.show_grid, "Grid");
                    ui.checkbox(&mut self.canvas.show_signal_names, "Signal names");
                    for (label, g) in GRID_OPTIONS {
                        ui.radio_value(&mut self.grid, g, label);
                    }
                    if ui.button("Zoom to fit").clicked() {
                        self.canvas.fit_requested = true;
                        ui.close();
                    }
                });
            });
        });
        if let Some(p) = open {
            self.open_file(&p);
        }
        if !self.warnings.is_empty() {
            egui::Panel::bottom("warnings").show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(80.0).show(ui, |ui| {
                    for w in &self.warnings {
                        ui.colored_label(egui::Color32::from_rgb(0xB0, 0x60, 0x00), format!("⚠ {w}"));
                    }
                });
            });
        }
        egui::Panel::right("inspector").default_size(260.0).show(ui, |ui| {
            actions.extend(inspector::show(ui, &self.diagram, &self.wires, self.canvas.highlight.as_deref()));
        });
        let mut outcomes = Vec::new();
        egui::CentralPanel::default().show(ui, |ui| {
            let mut view = View {
                diagram: &mut self.diagram,
                wires: &self.wires,
                junctions: &self.junctions,
                pin_pos: &self.pin_pos,
                signals: &self.signals,
                grid: self.grid,
            };
            outcomes = self.canvas.show(ui, &mut view);
        });
        for o in outcomes {
            match o {
                Outcome::Edited(before) => {
                    self.history.push(&before);
                    self.refresh();
                }
                Outcome::Do(a) => actions.push(a),
            }
        }
        for a in actions {
            self.apply(a);
        }
        if let Some(mut p) = self.prompt.take() {
            let (mut ok, mut cancel) = (false, false);
            egui::Window::new(p.title).collapsible(false).resizable(false).show(ui.ctx(), |ui| {
                let r = ui.text_edit_singleline(&mut p.text);
                r.request_focus();
                ui.horizontal(|ui| {
                    ok = ui.button("OK").clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    cancel = ui.button("Cancel").clicked();
                });
            });
            if ok {
                self.finish_prompt(p);
            } else if !cancel {
                self.prompt = Some(p);
            }
        }
    }
}
