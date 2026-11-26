pub mod lex;
pub mod sv;
pub mod vhdl;

use crate::model::{Instance, Port};
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct Design {
    pub top_name: String,
    pub top_ports: Vec<Port>,
    pub instances: Vec<Instance>,
    pub signals: HashMap<String, String>,
    pub warnings: Vec<String>,
    /// Every entity/module declared in the file: lowercase name -> ports (for cross-file lookup).
    /// Positional connections by instance label, resolved later when the entity is found elsewhere.
    pub positional: HashMap<String, Vec<String>>,
    pub entities: HashMap<String, Vec<Port>>,
}

/// Dispatch on file extension; unknown extensions yield an empty design with a warning.
pub fn parse_file(path_or_ext: &str, text: &str) -> Design {
    parse_file_top(path_or_ext, text, None)
}

/// Like `parse_file`, but build the diagram of the entity/module named `top`.
pub fn parse_file_top(path_or_ext: &str, text: &str, top: Option<&str>) -> Design {
    let ext = path_or_ext
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "vhd" | "vhdl" => vhdl::parse_vhdl_top(text, top),
        "sv" | "v" | "svh" => sv::parse_sv_top(text, top),
        _ => Design {
            warnings: vec![format!("unsupported file type '.{ext}'")],
            ..Default::default()
        },
    }
}

/// Fill each instance from `lib` (entity ports gathered from other files): real directions,
/// all declared ports in declaration order, unconnected ones with an empty signal.
pub fn resolve(d: &mut Design, lib: &HashMap<String, Vec<Port>>) {
    for inst in &mut d.instances {
        let Some(decl) = lib.get(&inst.entity.to_ascii_lowercase()) else {
            continue;
        };
        let mut old = std::mem::take(&mut inst.ports);
        if let Some(acts) = d.positional.get(&inst.name) {
            // `entity work.sub port map (a, b)`: the k-th actual goes to the k-th declared port
            for (dp, act) in decl.iter().zip(acts) {
                old.push(Port::new(&dp.name, dp.direction, act));
            }
            let tag = format!("in instance {} ", inst.name);
            d.warnings
                .retain(|w| !(w.contains("positional") && w.contains(&tag)));
        }
        let mut ports: Vec<Port> = decl
            .iter()
            .map(|dp| {
                match old
                    .iter()
                    .position(|p| p.name.eq_ignore_ascii_case(&dp.name))
                {
                    Some(i) => Port {
                        direction: dp.direction,
                        ..old.remove(i)
                    },
                    None => Port::new(&dp.name, dp.direction, ""),
                }
            })
            .collect();
        ports.extend(old); // connections to names the declaration lacks stay visible
        inst.original_ports = ports.clone();
        inst.ports = ports;
    }
}

fn is_hdl(f: &std::path::Path) -> bool {
    let e = f
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(e.as_str(), "vhd" | "vhdl" | "sv" | "v" | "svh")
}

/// HDL files in the folder of `path`, `path` itself included, sorted.
pub fn sibling_files(path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => std::path::Path::new("."),
    };
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|f| is_hdl(f))
        .collect();
    if !files.iter().any(|f| f == path) {
        files.push(path.to_path_buf());
    }
    files.sort();
    files
}

/// A project file list (`.f`): one path per line, relative to the list; `#` and `//` comments;
/// `-top NAME` / `+top+NAME` name the top. Returns (files, top).
pub fn read_filelist(
    path: &std::path::Path,
) -> Result<(Vec<std::path::PathBuf>, Option<String>), String> {
    let text = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&text);
    let dir = path.parent().unwrap_or(std::path::Path::new("."));
    let (mut files, mut top) = (Vec::new(), None);
    let mut words = text
        .lines()
        .map(|l| {
            l.split('#')
                .next()
                .unwrap_or("")
                .split("//")
                .next()
                .unwrap_or("")
                .trim()
        })
        .flat_map(|l| l.split_whitespace());
    while let Some(w) = words.next() {
        if w == "-top" {
            top = words.next().map(String::from);
        } else if let Some(t) = w.strip_prefix("+top+") {
            top = Some(t.to_string());
        } else if !w.starts_with(['-', '+']) {
            files.push(dir.join(w));
        }
    }
    if files.is_empty() {
        return Err(format!("{}: no source files listed", path.display()));
    }
    Ok((files, top))
}

/// What the files declare: entity ports (for filling instances) and which file defines each entity.
#[derive(Default)]
pub struct Library {
    pub ports: HashMap<String, Vec<Port>>,
    pub file_of: HashMap<String, std::path::PathBuf>,
    /// Entity names (lowercase) in file order.
    pub order: Vec<String>,
    /// Entities some other entity instantiates (what is left over are the candidate tops).
    pub instantiated: std::collections::HashSet<String>,
}

pub fn library(files: &[std::path::PathBuf]) -> Library {
    let mut lib = Library::default();
    for f in files {
        let Ok(bytes) = std::fs::read(f) else {
            continue;
        };
        let des = parse_file(&f.to_string_lossy(), &String::from_utf8_lossy(&bytes));
        for (k, v) in des.entities {
            lib.order.push(k.clone());
            lib.file_of.entry(k.clone()).or_insert_with(|| f.clone());
            lib.ports.entry(k).or_insert(v);
        }
        lib.instantiated
            .extend(des.instances.iter().map(|i| i.entity.to_ascii_lowercase()));
    }
    lib
}

#[cfg(test)]
mod tests {
    use crate::model::Direction;

    #[test]
    fn instances_get_real_ports_from_sibling_file() {
        let dir = std::env::temp_dir().join(format!("hdl_multi_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("top.vhd"),
            "entity top is port (a : in std_logic); end entity;
architecture x of top is begin u1 : entity work.sub port map (i => a); end;",
        )
        .unwrap();
        std::fs::write(
            dir.join("sub.vhd"),
            "entity sub is port (i : in std_logic; o : out std_logic; spare : out std_logic); end entity;",
        )
        .unwrap();
        let l = crate::load::load(&dir.join("top.vhd"), 20).unwrap();
        let ports = &l.diagram.instances[0].ports;
        let got: Vec<_> = ports
            .iter()
            .map(|p| (p.name.as_str(), p.signal.as_str(), p.direction))
            .collect();
        assert_eq!(
            got,
            vec![
                ("i", "a", Direction::In),
                ("o", "", Direction::Out),
                ("spare", "", Direction::Out)
            ]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn positional_connections_resolve_through_sibling_file() {
        let dir = std::env::temp_dir().join(format!("hdl_pos_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("top.vhd"),
            "entity top is port (a : in std_logic; q : out std_logic); end entity;
architecture x of top is begin u1 : entity work.sub port map (a, q); end;",
        )
        .unwrap();
        std::fs::write(
            dir.join("sub.vhd"),
            "entity sub is port (i : in std_logic; o : out std_logic); end entity;",
        )
        .unwrap();
        let l = crate::load::load(&dir.join("top.vhd"), 20).unwrap();
        let got: Vec<_> = l.diagram.instances[0]
            .ports
            .iter()
            .map(|p| (p.name.as_str(), p.signal.as_str()))
            .collect();
        assert_eq!(got, vec![("i", "a"), ("o", "q")]);
        assert!(
            l.warnings.iter().all(|w| !w.contains("positional")),
            "{:?}",
            l.warnings
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
