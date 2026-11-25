use crate::canvas::{Canvas, Scene};
use crate::config::*;
use crate::geometry::{junctions, Pt};
use crate::model::Diagram;
use crate::net::{design_to_diagram, route_all, top_pin_positions, wire_junctions, Wire};
use crate::parser::parse_file;
use eframe::egui;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
        let name = path.to_string_lossy();
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
        self.refresh();
        self.canvas.fit_requested = true;
    }

    /// Re-route after any change to the diagram.
    pub fn refresh(&mut self) {
        self.pin_pos = top_pin_positions(&self.diagram);
        self.wires = route_all(&self.diagram);
        self.junctions = wire_junctions(&self.diagram, &self.wires);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open…").clicked() {
                        ui.close();
                        let pick = rfd::FileDialog::new()
                            .add_filter("HDL or diagram", &["vhd", "vhdl", "sv", "v", "svh", "json"])
                            .pick_file();
                        if let Some(p) = pick {
                            self.open_file(&p);
                        }
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
        if !self.warnings.is_empty() {
            egui::Panel::bottom("warnings").show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(80.0).show(ui, |ui| {
                    for w in &self.warnings {
                        ui.colored_label(egui::Color32::from_rgb(0xB0, 0x60, 0x00), format!("⚠ {w}"));
                    }
                });
            });
        }
        egui::CentralPanel::default().show(ui, |ui| {
            let scene = Scene {
                diagram: &self.diagram,
                wires: &self.wires,
                junctions: &self.junctions,
                pin_pos: &self.pin_pos,
                signals: &self.signals,
                grid: self.grid,
            };
            self.canvas.show(ui, &scene);
        });
    }
}
