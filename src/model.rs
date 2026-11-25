use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    #[serde(rename = "IN")]
    In,
    #[serde(rename = "OUT")]
    Out,
    #[serde(rename = "INOUT")]
    InOut,
}

fn arial() -> String {
    "Arial".into()
}
fn t() -> bool {
    true
}
fn f8() -> u32 {
    8
}
fn f10() -> u32 {
    10
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Port {
    pub name: String,
    pub direction: Direction,
    pub signal: String,
    #[serde(default = "arial")]
    pub font_family: String,
    #[serde(default = "f8")]
    pub font_size: u32,
    #[serde(default)]
    pub font_bold: bool,
    #[serde(default)]
    pub font_italic: bool,
}

impl Port {
    pub fn new(name: &str, direction: Direction, signal: &str) -> Self {
        Port {
            name: name.into(),
            direction,
            signal: signal.into(),
            font_family: arial(),
            font_size: 8,
            font_bold: false,
            font_italic: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Instance {
    pub name: String,
    pub entity: String,
    pub ports: Vec<Port>,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub color_override: Option<String>,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub custom_width: i32,
    #[serde(default)]
    pub custom_height: i32,
    #[serde(default)]
    pub original_ports: Vec<Port>,
    #[serde(default = "t")]
    pub visible: bool,
    #[serde(default = "arial")]
    pub font_family: String,
    #[serde(default = "f10")]
    pub font_size: u32,
    #[serde(default = "t")]
    pub font_bold: bool,
    #[serde(default)]
    pub font_italic: bool,
    #[serde(default)]
    pub is_group: bool,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub children: Vec<Instance>,
}

impl Instance {
    pub fn new(name: &str, entity: &str, ports: Vec<Port>) -> Self {
        Instance {
            name: name.into(),
            entity: entity.into(),
            original_ports: ports.clone(),
            ports,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            color_override: None,
            locked: false,
            custom_width: 0,
            custom_height: 0,
            visible: true,
            font_family: arial(),
            font_size: 10,
            font_bold: true,
            font_italic: false,
            is_group: false,
            collapsed: false,
            children: vec![],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Diagram {
    #[serde(default)]
    pub instances: Vec<Instance>,
    #[serde(default)]
    pub top_level_pins: Vec<Port>,
}

impl Diagram {
    pub fn from_json(s: &str) -> Result<Diagram, serde_json::Error> {
        serde_json::from_str(s)
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("model serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_python_json() {
        let d = Diagram::from_json(include_str!("../tests/fixtures/python_diagram.json")).unwrap();
        assert_eq!(d.instances.len(), 2);
        assert_eq!(d.instances[0].ports[1].direction, Direction::Out);
        assert_eq!(d.top_level_pins.len(), 1);
    }

    #[test]
    fn missing_optional_fields_default() {
        let j = r#"{"instances":[{"name":"a","entity":"b","ports":[]}],"top_level_pins":[]}"#;
        let d = Diagram::from_json(j).unwrap();
        assert!(d.instances[0].visible);
        assert_eq!(d.instances[0].width, 0);
        assert_eq!(d.instances[0].font_size, 10);
    }

    #[test]
    fn roundtrip_is_stable() {
        let d = Diagram::from_json(include_str!("../tests/fixtures/python_diagram.json")).unwrap();
        assert_eq!(Diagram::from_json(&d.to_json()).unwrap(), d);
    }
}
