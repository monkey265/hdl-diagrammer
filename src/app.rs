use crate::canvas::{Canvas, Outcome, View};
use crate::config::*;
use crate::edit;
use crate::geometry::Pt;
use crate::group;
use crate::history::History;
use crate::model::{Diagram, Direction, Port};
use crate::net::{Stub, Wire, pin_connected, route_all_opts, top_pin_positions, wire_junctions};
use crate::recent;
use crate::script::{Script, Session};
use crate::ui::{Action, inspector};
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

struct GroupDialog {
    name: String,
    members: Vec<(usize, bool)>,
    ports: Vec<Port>,
}

pub struct App {
    pub diagram: Diagram,
    pub signals: HashMap<String, String>,
    pub warnings: Vec<String>,
    pub path: Option<PathBuf>,
    wires: Vec<Wire>,
    junctions: Vec<(String, Pt)>,
    pin_pos: Vec<Pt>,
    pin_ok: Vec<bool>,
    stubs: Vec<Stub>,
    stub_globals: bool,
    canvas: Canvas,
    grid: i32,
    history: History<Diagram>,
    prompt: Option<Prompt>,
    group_dialog: Option<GroupDialog>,
    recent: Vec<String>,
    script: Script,
    console_open: bool,
    console_input: String,
    console_log: Vec<String>,
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
            pin_ok: vec![],
            stubs: vec![],
            stub_globals: true,
            canvas: Canvas::default(),
            grid: DEFAULT_GRID,
            history: History::default(),
            prompt: None,
            group_dialog: None,
            recent: recent::load(),
            script: Script::new(DEFAULT_GRID),
            console_open: false,
            console_input: String::new(),
            console_log: vec![],
        }
    }
}

impl App {
    /// Load a VHDL/SystemVerilog source (parsed and laid out) or a saved `.json` diagram.
    pub fn open_file(&mut self, path: &Path) {
        match crate::load::load(path, self.grid) {
            Ok(l) => {
                (self.diagram, self.signals, self.warnings) = (l.diagram, l.signals, l.warnings)
            }
            Err(e) => {
                self.warnings = vec![e];
                return;
            }
        }
        let name = path.to_string_lossy().into_owned();
        self.path = Some(path.to_path_buf());
        self.history.clear();
        self.drop_index_state();
        self.canvas.highlight = None;
        recent::add(&mut self.recent, &name);
        recent::save(&self.recent);
        self.refresh();
        self.canvas.fit_requested = true;
    }

    /// Hand the diagram to the Tcl interpreter (it edits the same state and history as the GUI).
    fn lend(&mut self) {
        *self.script.session() = Session {
            diagram: std::mem::take(&mut self.diagram),
            signals: std::mem::take(&mut self.signals),
            warnings: std::mem::take(&mut self.warnings),
            grid: self.grid,
            history: std::mem::take(&mut self.history),
            opened: false,
        };
    }

    /// Take the state back after the interpreter ran, then re-route.
    pub fn reclaim(&mut self) {
        let s = std::mem::take(self.script.session());
        (self.diagram, self.signals, self.warnings, self.history) =
            (s.diagram, s.signals, s.warnings, s.history);
        self.drop_index_state();
        if s.opened {
            self.canvas.highlight = None;
            self.canvas.fit_requested = true;
        }
        self.refresh();
    }

    /// Run a Tcl script on the current diagram; the result or error is returned as text.
    pub fn run_script(&mut self, src: &str) -> Result<String, String> {
        self.lend();
        let r = self.script.eval(src);
        self.reclaim();
        r
    }

    fn console_ui(&mut self, ctx: &egui::Context) {
        let mut run = None;
        let mut open = self.console_open;
        egui::Window::new("Tcl console")
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for l in &self.console_log {
                            ui.monospace(l);
                        }
                    });
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.console_input)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .hint_text("instances, rename u1 cpu, group G u1 u2 …"),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    run = Some(std::mem::take(&mut self.console_input));
                    r.request_focus();
                }
            });
        self.console_open = open;
        if let Some(line) = run.filter(|l| !l.trim().is_empty()) {
            self.console_log.push(format!("% {line}"));
            match self.run_script(&line) {
                Ok(out) if !out.is_empty() => self.console_log.push(out),
                Ok(_) => {}
                Err(e) => self.console_log.push(format!("error: {e}")),
            }
        }
    }

    fn save_as(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .add_filter("Diagram", &["json"])
            .set_file_name("diagram.json")
            .save_file()
        else {
            return;
        };
        if let Err(e) = std::fs::write(&p, self.diagram.to_json()) {
            self.warnings = vec![format!("cannot save {}: {e}", p.display())];
        }
    }

    /// Re-route after any change to the diagram.
    pub fn refresh(&mut self) {
        self.pin_pos = top_pin_positions(&self.diagram);
        let r = route_all_opts(&self.diagram, self.stub_globals);
        (self.wires, self.stubs) = (r.wires, r.stubs);
        self.pin_ok = pin_connected(&self.diagram);
        self.junctions = wire_junctions(&self.diagram, &self.wires);
    }

    /// Apply `f`, record the previous state for undo, re-route.
    fn edit(&mut self, f: impl FnOnce(&mut Diagram)) {
        let before = self.diagram.clone();
        f(&mut self.diagram);
        self.history.push(&before);
        self.refresh();
    }

    /// Forget everything that holds instance indices (prompts, dialogs, selection, menu target).
    fn drop_index_state(&mut self) {
        self.prompt = None;
        self.group_dialog = None;
        self.canvas.forget_indices();
    }

    fn undo(&mut self) {
        if let Some(prev) = self.history.undo(&self.diagram) {
            self.drop_index_state();
            self.diagram = prev;
            self.refresh();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.history.redo(&self.diagram) {
            self.drop_index_state();
            self.diagram = next;
            self.refresh();
        }
    }

    fn apply(&mut self, a: Action) {
        let g = self.grid;
        if !matches!(a, Action::Highlight(_) | Action::Select(_)) {
            // a stale prompt/dialog would index blocks this action may shift or remove
            (self.prompt, self.group_dialog) = (None, None);
        }
        match a {
            Action::RenameInstance(i) => {
                let text = self.diagram.instances[i].name.clone();
                self.prompt = Some(Prompt {
                    kind: PromptKind::RenameInstance(i),
                    title: "Rename instance",
                    text,
                });
            }
            Action::RenamePort(i, p) => {
                let text = self.diagram.instances[i].ports[p].name.clone();
                self.prompt = Some(Prompt {
                    kind: PromptKind::RenamePort(i, p),
                    title: "Rename port",
                    text,
                });
            }
            Action::SetSize(i) => {
                let n = &self.diagram.instances[i];
                let text = if n.custom_width > 0 {
                    format!("{}x{}", n.custom_width, n.custom_height)
                } else {
                    String::new()
                };
                self.prompt = Some(Prompt {
                    kind: PromptKind::SetSize(i),
                    title: "Size WxH (empty = auto)",
                    text,
                });
            }
            Action::Color(i, c) => self.edit(|d| d.instances[i].color_override = c),
            Action::ToggleLock(i) => self.edit(|d| edit::toggle_lock(d, i)),
            Action::SetVisible(i, v) => self.edit(|d| edit::set_visible(d, i, v)),
            Action::ResetPorts(i) => self.edit(|d| {
                edit::reset_ports(d, i);
                edit::refit(d, i, g);
            }),
            Action::DeletePort(i, p) => self.edit(|d| {
                edit::delete_port(d, i, p);
                edit::refit(d, i, g);
            }),
            Action::Highlight(s) => self.canvas.highlight = s,
            Action::Select(i) => self.canvas.selected = vec![i],
            Action::ToggleCollapse(i) => self.edit(|d| group::toggle_collapse(d, i, g)),
            Action::Ungroup(i) => {
                self.edit(|d| group::ungroup(d, i));
                self.canvas.selected.clear();
            }
            Action::NewGroup => {
                let (x, y) = self.canvas.view_center;
                let name = group::unique_group_name(&self.diagram);
                self.edit(|d| {
                    group::create_empty_group(d, &name, x - 100, y - 100);
                });
            }
            Action::GroupSelection => {
                if self.canvas.selected.is_empty() {
                    self.warnings =
                        vec!["select one or more blocks first (click, shift-click)".into()];
                } else {
                    let sel = &self.canvas.selected;
                    self.group_dialog = Some(GroupDialog {
                        name: group::unique_group_name(&self.diagram),
                        members: self
                            .diagram
                            .instances
                            .iter()
                            .enumerate()
                            .filter(|(_, i)| i.visible)
                            .map(|(k, _)| (k, sel.contains(&k)))
                            .collect(),
                        ports: group::suggest_ports(&self.diagram, sel),
                    });
                }
            }
        }
    }

    fn finish_prompt(&mut self, p: Prompt) {
        let g = self.grid;
        let t = p.text.trim().to_string();
        match p.kind {
            PromptKind::RenameInstance(i) if !t.is_empty() => self.edit(|d| {
                if let Some(n) = d.instances.get_mut(i) {
                    n.name = t;
                }
                edit::refit(d, i, g);
            }),
            PromptKind::RenamePort(i, k) if !t.is_empty() => self.edit(|d| {
                if let Some(p) = d.instances.get_mut(i).and_then(|n| n.ports.get_mut(k)) {
                    p.name = t;
                }
            }),
            PromptKind::SetSize(i) => {
                let size = t
                    .split_once(['x', 'X'])
                    .and_then(|(w, h)| Some((w.trim().parse().ok()?, h.trim().parse().ok()?)));
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

/// Ungroup actions for a selection, highest index first so earlier removals do not shift later ones.
fn ungroup_actions(selected: &[usize]) -> Vec<Action> {
    let mut s = selected.to_vec();
    s.sort_unstable_by(|a, b| b.cmp(a));
    s.into_iter().map(Action::Ungroup).collect()
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Ctrl+Z inside a text box belongs to that box, not to the diagram
        let typing = ui.ctx().egui_wants_keyboard_input();
        let (undo, redo) = ui.input_mut(|i| {
            if typing {
                return (false, false);
            }
            let redo = i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                || i.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                );
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
                            .add_filter(
                                "HDL or diagram",
                                &["vhd", "vhdl", "sv", "v", "svh", "json"],
                            )
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
                    ui.separator();
                    if ui.button("Group selection…").clicked() {
                        actions.push(Action::GroupSelection);
                        ui.close();
                    }
                    if ui.button("New empty group").clicked() {
                        actions.push(Action::NewGroup);
                        ui.close();
                    }
                    if ui.button("Ungroup selection").clicked() {
                        actions.extend(ungroup_actions(&self.canvas.selected));
                        ui.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.canvas.show_grid, "Grid");
                    ui.checkbox(&mut self.canvas.show_signal_names, "Signal names");
                    ui.checkbox(&mut self.console_open, "Tcl console");
                    ui.checkbox(&mut self.canvas.show_unconnected, "Unconnected pins");
                    if ui
                        .checkbox(&mut self.stub_globals, "Clock/reset as stubs")
                        .changed()
                    {
                        self.refresh();
                    }
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
                egui::ScrollArea::vertical()
                    .max_height(80.0)
                    .show(ui, |ui| {
                        for w in &self.warnings {
                            ui.colored_label(
                                egui::Color32::from_rgb(0xB0, 0x60, 0x00),
                                format!("⚠ {w}"),
                            );
                        }
                    });
            });
        }
        egui::Panel::right("inspector")
            .default_size(260.0)
            .show(ui, |ui| {
                actions.extend(inspector::show(
                    ui,
                    &self.diagram,
                    &self.wires,
                    self.canvas.highlight.as_deref(),
                ));
            });
        let mut outcomes = Vec::new();
        egui::CentralPanel::default().show(ui, |ui| {
            let mut view = View {
                diagram: &mut self.diagram,
                wires: &self.wires,
                junctions: &self.junctions,
                pin_pos: &self.pin_pos,
                pin_connected: &self.pin_ok,
                stubs: &self.stubs,
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
        if let Some(mut gd) = self.group_dialog.take() {
            let (mut ok, mut cancel) = (false, false);
            egui::Window::new("Create group")
                .collapsible(false)
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut gd.name);
                    });
                    ui.separator();
                    ui.label("Blocks");
                    egui::ScrollArea::vertical()
                        .id_salt("gd_blocks")
                        .max_height(140.0)
                        .show(ui, |ui| {
                            for (k, on) in gd.members.iter_mut() {
                                let name = self.diagram.instances.get(*k).map_or("?", |i| &i.name);
                                ui.checkbox(on, name);
                            }
                        });
                    ui.separator();
                    ui.label("Ports");
                    let mut remove = None;
                    for (k, p) in gd.ports.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(90.0));
                            egui::ComboBox::from_id_salt(("gd_dir", k))
                                .selected_text(format!("{:?}", p.direction))
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut p.direction, Direction::In, "In");
                                    ui.selectable_value(&mut p.direction, Direction::Out, "Out");
                                    ui.selectable_value(
                                        &mut p.direction,
                                        Direction::InOut,
                                        "InOut",
                                    );
                                });
                            ui.add(egui::TextEdit::singleline(&mut p.signal).desired_width(90.0));
                            if ui.small_button("✕").clicked() {
                                remove = Some(k);
                            }
                        });
                    }
                    if let Some(k) = remove {
                        gd.ports.remove(k);
                    }
                    if ui.button("Add port").clicked() {
                        gd.ports.push(Port::new("port", Direction::InOut, ""));
                    }
                    ui.horizontal(|ui| {
                        ok = ui.button("OK").clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            if ok {
                let members: Vec<usize> = gd.members.iter().filter(|m| m.1).map(|m| m.0).collect();
                let name = if gd.name.trim().is_empty() {
                    group::unique_group_name(&self.diagram)
                } else {
                    gd.name.trim().to_string()
                };
                self.edit(|d| {
                    group::create_group(d, &name, &members, gd.ports);
                });
                self.canvas.selected.clear();
            } else if !cancel {
                self.group_dialog = Some(gd);
            }
        }
        self.console_ui(ui.ctx());
        if let Some(mut p) = self.prompt.take() {
            let (mut ok, mut cancel) = (false, false);
            egui::Window::new(p.title)
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    let r = ui.text_edit_singleline(&mut p.text);
                    r.request_focus();
                    ui.horizontal(|ui| {
                        ok = ui.button("OK").clicked()
                            || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Instance;

    fn app_with(n: usize) -> App {
        let mut a = App::default();
        a.diagram.instances = (0..n)
            .map(|k| {
                let mut i = Instance::new(&format!("u{k}"), "e", vec![]);
                (i.x, i.width, i.height) = (k as i32 * 300, 100, 100);
                i
            })
            .collect();
        a
    }

    #[test]
    fn undo_closes_prompts_and_dialogs_holding_indices() {
        let mut a = app_with(1);
        a.apply(Action::NewGroup); // index 1 now exists
        a.apply(Action::RenameInstance(1));
        assert!(a.prompt.is_some());
        a.canvas.selected = vec![1];
        a.undo();
        assert!(a.prompt.is_none() && a.group_dialog.is_none());
        assert!(a.canvas.selected.is_empty());
        a.apply(Action::NewGroup);
        a.canvas.selected = vec![0];
        a.apply(Action::GroupSelection);
        assert!(a.group_dialog.is_some());
        a.undo();
        assert!(a.group_dialog.is_none());
    }

    #[test]
    fn structural_edit_closes_open_prompt() {
        let mut a = app_with(2);
        a.apply(Action::RenameInstance(1));
        a.apply(Action::SetVisible(1, false));
        assert!(a.prompt.is_none());
    }

    #[test]
    fn ungroup_selection_goes_highest_index_first() {
        let acts = ungroup_actions(&[2, 0, 1]);
        assert_eq!(
            acts,
            vec![Action::Ungroup(2), Action::Ungroup(1), Action::Ungroup(0)]
        );
    }

    #[test]
    fn non_utf8_source_still_opens() {
        let p = std::env::temp_dir().join("hdl_diag_latin1_test.vhd");
        let mut bytes =
            b"-- caf\xe9 \xb0\nentity t is port (a : in std_logic); end entity;".to_vec();
        bytes.push(b'\n');
        std::fs::write(&p, bytes).unwrap();
        let mut a = App::default();
        a.open_file(&p);
        std::fs::remove_file(&p).ok();
        assert!(a.warnings.iter().all(|w| !w.contains("cannot read")));
        assert_eq!(a.diagram.instances.len(), 0);
        assert!(!a.diagram.top_level_pins.is_empty() || a.warnings.is_empty());
    }

    #[test]
    fn console_script_edits_share_the_guis_undo_history() {
        let mut a = app_with(2);
        assert_eq!(a.run_script("instances").unwrap(), "u0 u1");
        a.run_script("rename u0 cpu").unwrap();
        assert_eq!(a.diagram.instances[0].name, "cpu");
        a.undo();
        assert_eq!(a.diagram.instances[0].name, "u0");
        assert!(a.run_script("rename zz y").is_err());
        assert_eq!(a.diagram.instances.len(), 2); // state came back after the error
    }

    #[test]
    fn json_extension_is_case_insensitive() {
        let p = std::env::temp_dir().join("hdl_diag_upper_test.JSON");
        std::fs::write(&p, app_with(1).diagram.to_json()).unwrap();
        let mut a = App::default();
        a.open_file(&p);
        std::fs::remove_file(&p).ok();
        assert_eq!(a.diagram.instances.len(), 1);
    }
}
