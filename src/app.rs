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
    SaveView,
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
    filter: String,
    console_input: String,
    console_log: Vec<(LogKind, String)>,
    console_hist: Vec<String>,
    hist_pos: Option<usize>,
    show_structure: bool,
    show_objects: bool,
    views: Vec<crate::views::SavedView>,
    files: Vec<PathBuf>,
    nav: Vec<crate::script::Frame>,
    top: String,
    /// GUI only: route on a worker thread so big diagrams do not freeze the window.
    pub async_route: bool,
    route_rx: Option<std::sync::mpsc::Receiver<Routed>>,
    route_dirty: bool,
    /// After opening a design, fit the view again once its wires arrive (they extend past the blocks).
    refit_after_route: bool,
}

type Routed = (Vec<Wire>, Vec<Stub>, Vec<(String, Pt)>);

#[derive(Clone, Copy)]
enum LogKind {
    Echo,
    Out,
    Err,
}

fn questa_visuals() -> egui::Visuals {
    let c = |r, g, b| egui::Color32::from_rgb(r, g, b);
    let mut v = egui::Visuals::dark();
    v.panel_fill = c(0x12, 0x17, 0x1E);
    v.window_fill = c(0x16, 0x1C, 0x24);
    v.extreme_bg_color = c(0x05, 0x08, 0x0B);
    v.selection.bg_fill = c(0x1F, 0x4E, 0x79);
    v
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
            console_open: true,
            filter: String::new(),
            console_input: String::new(),
            console_log: vec![],
            console_hist: vec![],
            hist_pos: None,
            show_structure: true,
            show_objects: true,
            views: vec![],
            files: vec![],
            nav: vec![],
            top: String::new(),
            async_route: false,
            route_rx: None,
            route_dirty: false,
            refit_after_route: false,
        }
    }
}

impl App {
    /// Load a VHDL/SystemVerilog source (parsed and laid out) or a saved `.json` diagram.
    pub fn open_file(&mut self, path: &Path) {
        match crate::load::load(path, self.grid) {
            Ok(l) => {
                (self.diagram, self.signals, self.warnings) = (l.diagram, l.signals, l.warnings);
                (self.files, self.top) = (l.files, l.top);
                self.nav.clear();
            }
            Err(e) => {
                self.warnings = vec![e];
                return;
            }
        }
        let name = path.to_string_lossy().into_owned();
        self.path = Some(path.to_path_buf());
        self.views = crate::views::load(path);
        self.history.clear();
        self.drop_index_state();
        self.canvas.highlight = None;
        recent::add(&mut self.recent, &name);
        recent::save(&self.recent);
        self.refresh();
        self.canvas.fit_requested = true;
        self.refit_after_route = true;
    }

    /// Hand the diagram to the Tcl interpreter (it edits the same state and history as the GUI).
    fn lend(&mut self) {
        *self.script.session() = Session {
            diagram: std::mem::take(&mut self.diagram),
            signals: std::mem::take(&mut self.signals),
            warnings: std::mem::take(&mut self.warnings),
            grid: self.grid,
            history: std::mem::take(&mut self.history),
            views: std::mem::take(&mut self.views),
            view_now: (self.canvas.pan.x, self.canvas.pan.y, self.canvas.zoom),
            files: std::mem::take(&mut self.files),
            nav: std::mem::take(&mut self.nav),
            top: std::mem::take(&mut self.top),
            opened: false,
            ..Default::default()
        };
    }

    /// Take the state back after the interpreter ran, then re-route.
    pub fn reclaim(&mut self) {
        let s = std::mem::take(self.script.session());
        (self.diagram, self.signals, self.warnings, self.history) =
            (s.diagram, s.signals, s.warnings, s.history);
        (self.files, self.nav, self.top) = (s.files, s.nav, s.top);
        if s.views != self.views {
            self.views = s.views;
            self.persist_views();
        }
        if let Some((x, y, z)) = s.goto {
            (self.canvas.pan, self.canvas.zoom) = (egui::vec2(x, y), z);
            self.canvas.fit_requested = false;
        }
        self.drop_index_state();
        if s.opened {
            self.canvas.highlight = None;
            self.canvas.fit_requested = true;
            self.refit_after_route = true;
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

    /// Questa-style transcript docked at the bottom: warnings, echoed commands, results, prompt.
    fn transcript_ui(&mut self, ui: &mut egui::Ui) {
        if !self.console_open {
            return;
        }
        let mut run = None;
        let c = |r, g, b| egui::Color32::from_rgb(r, g, b);
        egui::Panel::bottom("transcript")
            .resizable(true)
            .default_size(170.0)
            .min_size(70.0)
            .show(ui, |ui| {
                inspector::title(ui, "Transcript");
                let log_h = (ui.available_height() - 28.0).max(20.0);
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .max_height(log_h)
                    .show(ui, |ui| {
                        for w in &self.warnings {
                            let t = format!("# ** Warning: {w}");
                            ui.label(
                                egui::RichText::new(t)
                                    .monospace()
                                    .color(c(0xFF, 0xB7, 0x4D)),
                            );
                        }
                        for (k, l) in &self.console_log {
                            let col = match k {
                                LogKind::Echo => c(0x9C, 0xDC, 0xFE),
                                LogKind::Out => c(0xB5, 0xF5, 0xB5),
                                LogKind::Err => c(0xFF, 0x6B, 0x6B),
                            };
                            ui.label(egui::RichText::new(l).monospace().color(col));
                        }
                    });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("VSIM>")
                            .monospace()
                            .strong()
                            .color(c(0x9C, 0xDC, 0xFE)),
                    );
                    let id = egui::Id::new("transcript_input");
                    if ui.memory(|m| m.has_focus(id)) {
                        let key = |ui: &egui::Ui, k| {
                            ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, k))
                        };
                        if key(ui, egui::Key::ArrowUp) && !self.console_hist.is_empty() {
                            let at = self
                                .hist_pos
                                .map_or(self.console_hist.len() - 1, |p| p.saturating_sub(1));
                            self.hist_pos = Some(at);
                            self.console_input = self.console_hist[at].clone();
                        }
                        if key(ui, egui::Key::ArrowDown) {
                            self.hist_pos = self
                                .hist_pos
                                .map(|p| p + 1)
                                .filter(|&p| p < self.console_hist.len());
                            self.console_input = self
                                .hist_pos
                                .map_or(String::new(), |p| self.console_hist[p].clone());
                        }
                        if key(ui, egui::Key::Tab) {
                            self.complete();
                        }
                    }
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.console_input)
                            .id(id)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .hint_text("instances, rename u1 cpu, group G u1 u2 …"),
                    );
                    if r.has_focus() {
                        // the TextEdit resets this every frame, so set ours after it: Tab and
                        // Up/Down stay in the box (history, completion) instead of moving focus
                        ui.memory_mut(|m| {
                            m.set_focus_lock_filter(
                                r.id,
                                egui::EventFilter {
                                    tab: true,
                                    horizontal_arrows: false,
                                    vertical_arrows: true,
                                    escape: false,
                                },
                            )
                        });
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        run = Some(std::mem::take(&mut self.console_input));
                        r.request_focus();
                    }
                });
            });
        if let Some(line) = run.filter(|l| !l.trim().is_empty()) {
            self.console_log
                .push((LogKind::Echo, format!("VSIM> {line}")));
            self.console_hist.push(line.clone());
            self.hist_pos = None;
            match self.run_script(&line) {
                Ok(out) if !out.is_empty() => out
                    .lines()
                    .for_each(|l| self.console_log.push((LogKind::Out, l.to_string()))),
                Ok(_) => {}
                Err(e) => e.lines().for_each(|l| {
                    self.console_log
                        .push((LogKind::Err, format!("# ** Error: {l}")))
                }),
            }
        }
    }

    /// Tab: complete the command name being typed; list the choices when ambiguous.
    fn complete(&mut self) {
        let word = self.console_input.trim_start().to_string();
        if word.contains(' ') {
            return;
        }
        let names: Vec<&str> = crate::script::command_names()
            .filter(|n| n.starts_with(&word))
            .collect();
        match names.as_slice() {
            [] => {}
            [one] => self.console_input = format!("{one} "),
            many => {
                let first = many[0];
                let common = (0..=first.len())
                    .rev()
                    .find(|&n| {
                        first.is_char_boundary(n) && many.iter().all(|m| m.starts_with(&first[..n]))
                    })
                    .unwrap_or(0);
                self.console_input = first[..common].to_string();
                self.console_log.push((LogKind::Out, many.join("  ")));
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

    fn export_hdl(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .add_filter("VHDL", &["vhd"])
            .add_filter("SystemVerilog", &["sv"])
            .set_file_name("top.vhd")
            .save_file()
        else {
            return;
        };
        let name = if self.top.is_empty() {
            "top"
        } else {
            &self.top
        };
        let text = if p.extension().is_some_and(|e| e == "sv" || e == "v") {
            crate::hdlgen::to_sv(&self.diagram, &self.signals, name)
        } else {
            crate::hdlgen::to_vhdl(&self.diagram, &self.signals, name)
        };
        if let Err(e) = std::fs::write(&p, text) {
            self.warnings = vec![format!("cannot write {}: {e}", p.display())];
        }
    }

    fn export_image(&mut self) {
        let Some(p) = rfd::FileDialog::new()
            .add_filter("SVG", &["svg"])
            .add_filter("PNG", &["png"])
            .set_file_name("diagram.svg")
            .save_file()
        else {
            return;
        };
        if let Err(e) = crate::svg::write(&p, &self.diagram, &self.signals) {
            self.warnings = vec![e];
        }
    }

    fn persist_views(&self) {
        if let Some(p) = &self.path {
            crate::views::save(p, &self.views);
        }
    }

    /// Run a navigation command; a failure is shown as a warning.
    fn navigate(&mut self, cmd: &str) {
        match self.run_script(cmd) {
            Ok(_) => {
                self.canvas.highlight = None;
                self.path = None;
            }
            Err(e) => self.warnings = vec![e],
        }
    }

    /// `top / sub / subsub` for the Structure pane title.
    fn crumb(&self) -> String {
        let mut parts: Vec<&str> = self.nav.iter().map(|f| f.top.as_str()).collect();
        parts.push(&self.top);
        parts.retain(|p| !p.is_empty());
        parts.join(" / ")
    }

    /// Re-route after any change to the diagram.
    pub fn refresh(&mut self) {
        self.pin_pos = top_pin_positions(&self.diagram);
        self.pin_ok = pin_connected(&self.diagram);
        if self.async_route {
            self.route_dirty = true; // `poll_route` starts the job; old wires stay until it ends
        } else {
            let r = route_all_opts(&self.diagram, self.stub_globals);
            self.junctions = wire_junctions(&self.diagram, &r.wires);
            (self.wires, self.stubs) = (r.wires, r.stubs);
        }
    }

    /// Collect a finished routing job and start the next one if the diagram changed meanwhile.
    fn poll_route(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.route_rx {
            match rx.try_recv() {
                Ok((w, st, j)) => {
                    (self.wires, self.stubs, self.junctions) = (w, st, j);
                    self.route_rx = None;
                    if self.refit_after_route && !self.route_dirty {
                        self.refit_after_route = false;
                        self.canvas.fit_requested = true;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(40));
                    return;
                }
                Err(_) => self.route_rx = None,
            }
        }
        if self.route_rx.is_none() && self.route_dirty {
            self.route_dirty = false;
            let (d, stubs) = (self.diagram.clone(), self.stub_globals);
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let r = route_all_opts(&d, stubs);
                let j = wire_junctions(&d, &r.wires);
                let _ = tx.send((r.wires, r.stubs, j));
            });
            self.route_rx = Some(rx);
            ctx.request_repaint_after(std::time::Duration::from_millis(40));
        }
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
        if !matches!(a, Action::Highlight(_) | Action::Focus(_)) {
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
            Action::ToggleCollapse(i) => self.edit(|d| group::toggle_collapse(d, i, g)),
            Action::AddToGroup(i) => {
                let sel = self.canvas.selected.clone();
                self.edit(|d| {
                    group::add_to_group(d, i, &sel);
                });
                self.canvas.forget_indices();
            }
            Action::ReleaseChild(i, c) => {
                self.edit(|d| group::release_child(d, i, c));
                self.canvas.forget_indices();
            }
            Action::DeleteGroup(i) => {
                self.edit(|d| group::delete_group(d, i));
                self.canvas.forget_indices();
            }
            Action::Ungroup(i) => {
                self.edit(|d| group::ungroup(d, i));
                self.canvas.selected.clear();
            }
            Action::AutoLayout => self.edit(|d| crate::layout::auto_layout(d, g)),
            Action::AutoLayoutSelection => {
                let sel = self.canvas.selected.clone();
                self.edit(|d| crate::layout::auto_layout_subset(d, &sel, g));
            }
            Action::Focus(i) => {
                if let Some(b) = self.diagram.instances.get(i) {
                    let (cx, cy) = (b.x + b.width / 2, b.y + b.height / 2);
                    let vc = self.canvas.view_center;
                    let z = self.canvas.zoom;
                    self.canvas.pan +=
                        eframe::egui::Vec2::new((vc.0 - cx) as f32, (vc.1 - cy) as f32) * z;
                    self.canvas.view_center = (cx, cy);
                    self.canvas.selected = vec![i];
                }
            }
            Action::Descend(i) => {
                if let Some(name) = self.diagram.instances.get(i).map(|b| b.name.clone()) {
                    self.navigate(&format!("descend {name}"));
                }
            }
            Action::Ascend => self.navigate("ascend"),
            Action::SaveView => {
                let name = (1..)
                    .map(|n| format!("View {n}"))
                    .find(|n| self.views.iter().all(|v| &v.name != n))
                    .expect("infinite range");
                self.prompt = Some(Prompt {
                    kind: PromptKind::SaveView,
                    title: "Save view as",
                    text: name,
                });
            }
            Action::GotoView(i) => {
                if let Some(v) = self.views.get(i) {
                    (self.canvas.pan, self.canvas.zoom) = (egui::vec2(v.pan.0, v.pan.1), v.zoom);
                    self.canvas.fit_requested = false;
                }
            }
            Action::DeleteView(i) => {
                if i < self.views.len() {
                    self.views.remove(i);
                    self.persist_views();
                }
            }
            Action::Connect(a, b) => {
                let mut result = Ok(String::new());
                self.edit(|d| result = edit::connect(d, a, b));
                if let Err(e) = result {
                    self.warnings = vec![e];
                }
            }
            Action::Disconnect(e) => self.edit(|d| edit::disconnect(d, e)),
            Action::CollapseInterfaces => self.edit(|d| {
                crate::interface::collapse_all(d, g);
            }),
            Action::ExpandInterfaces => self.edit(|d| crate::interface::expand_all(d, g)),
            Action::Validate => {
                let found = crate::validate::validate(&self.diagram);
                self.warnings = if found.is_empty() {
                    vec!["validate: no problems found".into()]
                } else {
                    found
                };
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
            PromptKind::SaveView if !t.is_empty() => {
                let v = crate::views::SavedView {
                    name: t,
                    pan: (self.canvas.pan.x, self.canvas.pan.y),
                    zoom: self.canvas.zoom,
                };
                crate::views::put(&mut self.views, v);
                self.persist_views();
            }
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
        self.poll_route(ui.ctx());
        ui.ctx().set_visuals(if self.canvas.dark {
            questa_visuals()
        } else {
            egui::Visuals::light()
        });
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
                    if ui.button("Export SVG/PNG…").clicked() {
                        ui.close();
                        self.export_image();
                    }
                    if ui.button("Export HDL…").clicked() {
                        ui.close();
                        self.export_hdl();
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
                ui.menu_button("Tools", |ui| {
                    if ui.button("Auto layout").clicked() {
                        actions.push(Action::AutoLayout);
                        ui.close();
                    }
                    if ui.button("Auto layout selection").clicked() {
                        actions.push(Action::AutoLayoutSelection);
                        ui.close();
                    }
                    ui.separator();
                    if ui
                        .add_enabled(!self.nav.is_empty(), egui::Button::new("Go up one level"))
                        .clicked()
                    {
                        actions.push(Action::Ascend);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Collapse interfaces").clicked() {
                        actions.push(Action::CollapseInterfaces);
                        ui.close();
                    }
                    if ui.button("Expand interfaces").clicked() {
                        actions.push(Action::ExpandInterfaces);
                        ui.close();
                    }
                    if ui.button("Validate design").clicked() {
                        actions.push(Action::Validate);
                        ui.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.canvas.show_grid, "Grid");
                    ui.checkbox(&mut self.canvas.show_signal_names, "Signal names");
                    ui.separator();
                    ui.checkbox(&mut self.show_structure, "Structure");
                    ui.checkbox(&mut self.show_objects, "Objects");
                    ui.checkbox(&mut self.console_open, "Transcript");
                    ui.checkbox(&mut self.canvas.dark, "Dark theme");
                    ui.separator();
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
                    ui.menu_button("Bookmarks", |ui| {
                        if ui.button("Save view…").clicked() {
                            actions.push(Action::SaveView);
                            ui.close();
                        }
                        for (i, v) in self.views.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui.button(&v.name).clicked() {
                                    actions.push(Action::GotoView(i));
                                    ui.close();
                                }
                                if ui.small_button("✕").clicked() {
                                    actions.push(Action::DeleteView(i));
                                    ui.close();
                                }
                            });
                        }
                    });
                    if ui.button("Zoom to fit").clicked() {
                        self.canvas.fit_requested = true;
                        ui.close();
                    }
                });
                if self.route_rx.is_some() || self.route_dirty {
                    ui.weak("⟳ routing…");
                }
            });
        });
        if let Some(p) = open {
            self.open_file(&p);
        }
        self.transcript_ui(ui);
        let crumb = self.crumb();
        if self.show_structure {
            egui::Panel::left("structure")
                .default_size(220.0)
                .show(ui, |ui| {
                    actions.extend(inspector::structure(
                        ui,
                        &crumb,
                        &self.diagram,
                        &self.canvas.selected,
                        &mut self.filter,
                    ));
                });
        }
        if self.show_objects {
            egui::Panel::right("objects")
                .default_size(240.0)
                .show(ui, |ui| {
                    actions.extend(inspector::objects(
                        ui,
                        &self.diagram,
                        &self.canvas.selected,
                        &self.wires,
                        &self.stubs,
                        self.canvas.highlight.as_deref(),
                        &self.filter,
                    ));
                });
        }
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

    #[test]
    fn async_routing_delivers_the_same_wires_in_the_end() {
        let mut sync = App::default();
        sync.open_file(Path::new("tests/fixtures/RC4_sync_top.vhd"));
        let mut a = App {
            async_route: true,
            ..Default::default()
        };
        a.open_file(Path::new("tests/fixtures/RC4_sync_top.vhd"));
        assert!(a.wires.is_empty(), "nothing routed until the job is polled");
        let ctx = egui::Context::default();
        for _ in 0..500 {
            a.poll_route(&ctx);
            if a.route_rx.is_none() && !a.route_dirty {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(a.wires.len(), sync.wires.len());
        assert!(!a.wires.is_empty());
    }
}
