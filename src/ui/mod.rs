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
    /// Move the selected blocks into this group.
    AddToGroup(usize),
    /// (group, child position): take a member out of the group.
    ReleaseChild(usize, usize),
    /// Remove the group and everything in it.
    DeleteGroup(usize),
    Ungroup(usize),
    GroupSelection,
    NewGroup,
    AutoLayout,
    AutoLayoutSelection,
    /// Select a block and pan the view to it.
    Focus(usize),
    /// Open the diagram of this instance's entity from the project files.
    Descend(usize),
    /// Back to the diagram above.
    Ascend,
    SaveView,
    GotoView(usize),
    DeleteView(usize),
    CollapseInterfaces,
    ExpandInterfaces,
    Validate,
}
