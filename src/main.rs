mod app;
mod canvas;
mod config;
mod edit;
mod history;
mod recent;
mod gates;
mod geometry;
mod group;
mod layout;
mod model;
mod net;
mod parser;
mod router;
mod ui;

fn main() -> eframe::Result {
    let arg = std::env::args().nth(1);
    eframe::run_native(
        "hdl-diagrammer",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(eframe::egui::Visuals::light());
            let mut app = app::App::default();
            if let Some(p) = arg {
                app.open_file(std::path::Path::new(&p));
            }
            Ok(Box::new(app))
        }),
    )
}
