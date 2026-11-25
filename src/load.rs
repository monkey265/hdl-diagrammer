use crate::model::Diagram;
use crate::net::design_to_diagram;
use crate::parser::parse_file;
use std::collections::HashMap;
use std::path::Path;

pub struct Loaded {
    pub diagram: Diagram,
    pub signals: HashMap<String, String>,
    pub warnings: Vec<String>,
}

/// Read a VHDL/SystemVerilog source (parsed and laid out) or a saved `.json` diagram.
pub fn load(path: &Path, grid: i32) -> Result<Loaded, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes); // Latin-1 comments are common in HDL
    let name = path.to_string_lossy();
    if name.to_ascii_lowercase().ends_with(".json") {
        let diagram = Diagram::from_json(&text).map_err(|e| format!("bad diagram file: {e}"))?;
        Ok(Loaded {
            diagram,
            signals: HashMap::new(),
            warnings: vec![],
        })
    } else {
        let design = parse_file(&name, &text);
        Ok(Loaded {
            diagram: design_to_diagram(&design, grid),
            signals: design.signals,
            warnings: design.warnings,
        })
    }
}
