//! Saved views (bookmarks): a name plus the pan and zoom of the canvas. Kept next to the opened
//! source as `<file>.views.json`.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SavedView {
    pub name: String,
    pub pan: (f32, f32),
    pub zoom: f32,
}

fn sidecar(source: &Path) -> PathBuf {
    let mut s = source.as_os_str().to_owned();
    s.push(".views.json");
    PathBuf::from(s)
}

/// Missing or unreadable files just mean "no bookmarks".
pub fn load(source: &Path) -> Vec<SavedView> {
    std::fs::read_to_string(sidecar(source))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Write the list (or remove the file when it is empty); failures are ignored, bookmarks are a convenience.
pub fn save(source: &Path, views: &[SavedView]) {
    let f = sidecar(source);
    if views.is_empty() {
        let _ = std::fs::remove_file(f);
    } else if let Ok(t) = serde_json::to_string_pretty(views) {
        let _ = std::fs::write(f, t);
    }
}

/// Insert or replace by name.
pub fn put(views: &mut Vec<SavedView>, v: SavedView) {
    match views.iter_mut().find(|x| x.name == v.name) {
        Some(x) => *x = v,
        None => views.push(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_roundtrip_and_replace_by_name() {
        let dir = std::env::temp_dir().join(format!("hdl_views_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("top.vhd");
        assert!(load(&src).is_empty());
        let mut v = vec![];
        put(
            &mut v,
            SavedView {
                name: "a".into(),
                pan: (1.0, 2.0),
                zoom: 0.5,
            },
        );
        put(
            &mut v,
            SavedView {
                name: "a".into(),
                pan: (3.0, 4.0),
                zoom: 2.0,
            },
        );
        assert_eq!(v.len(), 1);
        save(&src, &v);
        assert_eq!(load(&src), v);
        save(&src, &[]);
        assert!(load(&src).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }
}
