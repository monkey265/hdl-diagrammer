mod app;
mod canvas;
mod config;
mod edit;
mod gates;
mod geometry;
mod group;
mod history;
mod interface;
mod layout;
mod load;
mod model;
mod net;
mod parser;
mod recent;
mod router;
mod script;
mod svg;
mod ui;
mod validate;
mod views;

const USAGE: &str = "usage: hdl-diagrammer [file.vhd|.sv|.json] [--script file.tcl [--gui]]
  --script  run a Tcl script (headless unless --gui is also given)";

fn main() -> eframe::Result {
    let (mut file, mut script, mut gui) = (None, None, false);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--script" => script = args.next(),
            "--gui" => gui = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ if file.is_none() && !a.starts_with("--") => file = Some(a),
            _ => {
                eprintln!("unexpected argument '{a}'\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    let mut app = app::App::default();
    if let Some(p) = &file {
        app.open_file(std::path::Path::new(p));
    }
    if let Some(path) = script {
        app.warnings.iter().for_each(|w| eprintln!("warning: {w}"));
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("cannot read {path}: {e}");
            std::process::exit(2);
        });
        match app.run_script(&src) {
            Ok(out) if !out.is_empty() => println!("{out}"),
            Ok(_) => {}
            Err(e) => {
                eprintln!("{path}: {e}");
                std::process::exit(1);
            }
        }
        if !gui {
            return Ok(());
        }
    }
    eframe::run_native(
        "hdl-diagrammer",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]),
            ..Default::default()
        },
        Box::new(move |_cc| {
            let mut app = app;
            app.async_route = true;
            app.refresh();
            Ok(Box::new(app))
        }),
    )
}
