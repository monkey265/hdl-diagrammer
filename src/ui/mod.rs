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
    ToggleCollapse(usize),
    Ungroup(usize),
    GroupSelection,
    NewGroup,
    AutoLayout,
    AutoLayoutSelection,
    /// Select a block and pan the view to it.
    Focus(usize),
    CollapseInterfaces,
    ExpandInterfaces,
    Validate,
}
