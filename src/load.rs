use crate::model::Diagram;
use crate::net::design_to_diagram;
use crate::parser::{library, parse_file_top, read_filelist, resolve, sibling_files};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Loaded {
    pub diagram: Diagram,
    pub signals: HashMap<String, String>,
    pub warnings: Vec<String>,
    /// Source files the entities were looked up in (for descending into instances).
    pub files: Vec<PathBuf>,
    pub top: String,
}

/// Read a VHDL/SystemVerilog source (parsed and laid out), a `.f` file list, or a saved `.json` diagram.
pub fn load(path: &Path, grid: i32) -> Result<Loaded, String> {
    load_with(path, grid, None, None)
}

/// `top` picks the entity/module to draw; `files` is the project's file set (default: the
/// other HDL files in the same folder).
pub fn load_with(
    path: &Path,
    grid: i32,
    top: Option<&str>,
    files: Option<&[PathBuf]>,
) -> Result<Loaded, String> {
    let name = path.to_string_lossy();
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".f") {
        let (list, listed_top) = read_filelist(path)?;
        let lib = library(&list);
        let top = top
            .map(str::to_ascii_lowercase)
            .or(listed_top.map(|t| t.to_ascii_lowercase()))
            .or_else(|| {
                lib.order
                    .iter()
                    .find(|e| !lib.instantiated.contains(*e))
                    .cloned()
            })
            .ok_or("no entity found in the listed files")?;
        let file = lib
            .file_of
            .get(&top)
            .ok_or_else(|| format!("top \"{top}\" is not declared in the listed files"))?;
        return load_with(file, grid, Some(&top), Some(&list));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes); // Latin-1 comments are common in HDL
    if lower.ends_with(".json") {
        let diagram = Diagram::from_json(&text).map_err(|e| format!("bad diagram file: {e}"))?;
        return Ok(Loaded {
            diagram,
            signals: HashMap::new(),
            warnings: vec![],
            files: vec![],
            top: String::new(),
        });
    }
    let files: Vec<PathBuf> = files.map_or_else(|| sibling_files(path), <[PathBuf]>::to_vec);
    let mut design = parse_file_top(&name, &text, top);
    let mut lib = library(&files).ports;
    lib.extend(design.entities.clone()); // this file wins over the others
    resolve(&mut design, &lib);
    Ok(Loaded {
        diagram: design_to_diagram(&design, grid),
        signals: design.signals,
        warnings: design.warnings,
        files,
        top: design.top_name,
    })
}
