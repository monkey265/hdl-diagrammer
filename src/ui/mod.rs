pub mod inspector;

/// Edits requested from the canvas context menu or the inspector; applied by the app.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    RenameInstance(usize),
    RenamePort(usize, usize),
    SetSize(usize),
    Color(usize, Option<String>),
    ToggleLock(usize),
    SetVisible(usize, bool),
    ResetPorts(usize),
    DeletePort(usize, usize),
    Highlight(Option<String>),
    Select(usize),
    ToggleCollapse(usize),
    Ungroup(usize),
    GroupSelection,
    NewGroup,
}
