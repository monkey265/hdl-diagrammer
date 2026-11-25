use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 10;

#[derive(Serialize, Deserialize, Default)]
struct Config {
    #[serde(default)]
    recent_files: Vec<String>,
}

/// Move `path` to the front, dropping duplicates and anything past 10 entries.
pub fn add(list: &mut Vec<String>, path: &str) {
    list.retain(|p| p != path);
    list.insert(0, path.to_string());
    list.truncate(MAX_RECENT);
}

fn config_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".hdl_diagrammer_config.json"))
}

/// Missing or unreadable config is not an error: start with no recents.
pub fn load() -> Vec<String> {
    let text = config_path().and_then(|p| std::fs::read_to_string(p).ok());
    text.and_then(|t| serde_json::from_str::<Config>(&t).ok()).map(|c| c.recent_files).unwrap_or_default()
}

pub fn save(list: &[String]) {
    if let Some(p) = config_path() {
        let cfg = Config { recent_files: list.to_vec() };
        let _ = std::fs::write(p, serde_json::to_string(&cfg).unwrap_or_default()); // best effort
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_files_dedup_and_cap_10() {
        let mut l = Vec::new();
        for i in 0..12 {
            add(&mut l, &format!("f{i}"));
        }
        assert_eq!(l.len(), 10);
        assert_eq!(l[0], "f11");
        add(&mut l, "f5");
        assert_eq!(l[0], "f5");
        assert_eq!(l.iter().filter(|p| *p == "f5").count(), 1);
        assert_eq!(l.len(), 10);
    }
}
