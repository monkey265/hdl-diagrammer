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
    let ext = path_or_ext
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "vhd" | "vhdl" => vhdl::parse_vhdl(text),
        "sv" | "v" | "svh" => sv::parse_sv(text),
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

/// Parse every other HDL file next to `path` and merge their entities into one lookup table.
pub fn sibling_entities(path: &std::path::Path) -> HashMap<String, Vec<Port>> {
    let mut lib = HashMap::new();
    let Some(dir) = path.parent().map(|p| {
        if p.as_os_str().is_empty() {
            std::path::Path::new(".")
        } else {
            p
        }
    }) else {
        return lib;
    };
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|f| f != path)
        .filter(|f| {
            let e = f
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            matches!(e.as_str(), "vhd" | "vhdl" | "sv" | "v" | "svh")
        })
        .collect();
    files.sort();
    for f in files {
        let Ok(bytes) = std::fs::read(&f) else {
            continue;
        };
        let des = parse_file(&f.to_string_lossy(), &String::from_utf8_lossy(&bytes));
        for (k, v) in des.entities {
            lib.entry(k).or_insert(v);
        }
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
