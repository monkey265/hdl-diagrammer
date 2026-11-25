pub mod lex;

use crate::model::{Instance, Port};
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct Design {
    pub top_name: String,
    pub top_ports: Vec<Port>,
    pub instances: Vec<Instance>,
    pub signals: HashMap<String, String>,
    pub warnings: Vec<String>,
}

/// Dispatch on file extension; unknown extensions yield an empty design with a warning.
pub fn parse_file(path_or_ext: &str, text: &str) -> Design {
    let ext = path_or_ext.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let _ = text;
    Design { warnings: vec![format!("unsupported file type '.{ext}'")], ..Default::default() }
}
