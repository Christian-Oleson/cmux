use cmux_core::layout::{LayoutEngine, SplitDirection};
use cmux_core::screen::{ScreenBuffer, ScreenSnapshot};
use cmux_core::types::PaneId;
use cmux_ipc::messages::WorkspaceInfo;
use std::collections::HashMap;

/// Per-workspace state: a layout engine and the associated screen buffers.
struct WorkspaceState {
    layout: LayoutEngine,
    screens: HashMap<PaneId, ScreenBuffer>,
}

pub struct PaneManager {
    workspaces: HashMap<u32, WorkspaceState>,
    workspace_names: HashMap<u32, String>,
    active_workspace: u32,
    session_name: String,
}

impl PaneManager {
    #[cfg(test)]
    pub fn new(rows: u16, cols: u16) -> Self {
        Self::new_with_session("0".into(), rows, cols)
    }

    pub fn new_with_session(session_name: String, rows: u16, cols: u16) -> Self {
        let layout = LayoutEngine::new(rows, cols);
        let mut screens = HashMap::new();
        screens.insert(
            PaneId(0),
            ScreenBuffer::new(rows, cols, cmux_config::defaults::DEFAULT_SCROLLBACK),
        );

        let ws_state = WorkspaceState { layout, screens };
        let mut workspaces = HashMap::new();
        workspaces.insert(0, ws_state);

        let mut workspace_names = HashMap::new();
        workspace_names.insert(0, "0".into());

        Self {
            workspaces,
            workspace_names,
            active_workspace: 0,
            session_name,
        }
    }

    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    /// Return the active workspace state (panics if missing, which should not happen).
    fn active_ws(&self) -> &WorkspaceState {
        self.workspaces
            .get(&self.active_workspace)
            .expect("active workspace must exist")
    }

    fn active_ws_mut(&mut self) -> &mut WorkspaceState {
        self.workspaces
            .get_mut(&self.active_workspace)
            .expect("active workspace must exist")
    }

    pub fn process_output(&mut self, pane_id: PaneId, data: &[u8]) {
        // Output could come for any workspace's pane, not just the active one.
        for ws in self.workspaces.values_mut() {
            if let Some(screen) = ws.screens.get_mut(&pane_id) {
                screen.process(data);
                return;
            }
        }
    }

    pub fn split(&mut self, direction: SplitDirection) -> PaneId {
        let ws = self.active_ws_mut();
        let new_pane_id = ws.layout.split(direction);

        // Resize all screen buffers to match new layout
        sync_screen_sizes(&mut ws.layout, &mut ws.screens);

        // Create screen buffer for new pane
        let rects = ws.layout.pane_rects();
        if let Some(rect) = rects.iter().find(|r| r.pane_id == new_pane_id) {
            ws.screens.insert(
                new_pane_id,
                ScreenBuffer::new(
                    rect.height,
                    rect.width,
                    cmux_config::defaults::DEFAULT_SCROLLBACK,
                ),
            );
        }

        new_pane_id
    }

    pub fn close_pane(&mut self, pane_id: PaneId) -> bool {
        let ws = self.active_ws_mut();
        if !ws.layout.close_pane(pane_id) {
            return false;
        }
        ws.screens.remove(&pane_id);
        sync_screen_sizes(&mut ws.layout, &mut ws.screens);
        true
    }

    pub fn resize_terminal(&mut self, rows: u16, cols: u16) {
        for ws in self.workspaces.values_mut() {
            ws.layout.resize_terminal(rows, cols);
            sync_screen_sizes(&mut ws.layout, &mut ws.screens);
        }
    }

    /// Find the pane at the given terminal (row, col) position, if any.
    pub fn pane_at_position(&self, row: u16, col: u16) -> Option<PaneId> {
        let rects = self.layout().pane_rects();
        for rect in &rects {
            if row >= rect.row
                && row < rect.row + rect.height
                && col >= rect.col
                && col < rect.col + rect.width
            {
                return Some(rect.pane_id);
            }
        }
        None
    }

    pub fn layout(&self) -> &LayoutEngine {
        &self.active_ws().layout
    }

    pub fn layout_mut(&mut self) -> &mut LayoutEngine {
        &mut self.active_ws_mut().layout
    }

    pub fn active_pane(&self) -> PaneId {
        self.active_ws().layout.active_pane()
    }

    pub fn snapshots(&self) -> HashMap<PaneId, ScreenSnapshot> {
        self.active_ws()
            .screens
            .iter()
            .map(|(id, screen)| (*id, screen.snapshot()))
            .collect()
    }

    // --- Workspace operations ---

    /// Switch to the workspace with the given id. Returns true if the switch happened.
    pub fn switch_workspace(&mut self, ws_id: u32) -> bool {
        if !self.workspaces.contains_key(&ws_id) {
            return false;
        }
        self.active_workspace = ws_id;
        true
    }

    /// Create a new workspace with the given id, initial pane, and name.
    pub fn create_workspace(&mut self, ws_id: u32, pane_id: PaneId, name: String) {
        // Determine dimensions from an existing workspace
        let (rows, cols) = self
            .workspaces
            .values()
            .next()
            .map(|ws| {
                let rects = ws.layout.pane_rects();
                // Use the terminal-level dimensions — get from the first workspace's layout
                // We sum up total height for now, but resize_terminal stores the actual dims.
                // Just use a pane rect to approximate. Actually, the LayoutEngine stores
                // terminal_rows/cols but we don't have a getter. We can read from pane_rects.
                // For a single-pane workspace the rect *is* the terminal size.
                // For a more robust approach, let's store terminal dimensions.
                if rects.len() == 1 {
                    (rects[0].height, rects[0].width)
                } else {
                    // Fallback: use 24x80
                    (24, 80)
                }
            })
            .unwrap_or((24, 80));

        let layout = LayoutEngine::new_with_pane_id(rows, cols, pane_id);
        let mut screens = HashMap::new();
        screens.insert(
            pane_id,
            ScreenBuffer::new(rows, cols, cmux_config::defaults::DEFAULT_SCROLLBACK),
        );

        self.workspaces
            .insert(ws_id, WorkspaceState { layout, screens });
        self.workspace_names.insert(ws_id, name);
    }

    /// Close a workspace, removing all its state. Returns true if it existed.
    pub fn close_workspace(&mut self, ws_id: u32) -> bool {
        let removed = self.workspaces.remove(&ws_id).is_some();
        self.workspace_names.remove(&ws_id);

        // If we closed the active workspace, switch to the first available
        if removed && self.active_workspace == ws_id {
            if let Some(&first_id) = self.workspaces.keys().min() {
                self.active_workspace = first_id;
            }
        }

        removed
    }

    /// Return a sorted list of (workspace_id, name, is_active) for the status bar.
    pub fn workspace_list(&self) -> Vec<(u32, String, bool)> {
        let mut list: Vec<(u32, String, bool)> = self
            .workspace_names
            .iter()
            .map(|(&id, name)| (id, name.clone(), id == self.active_workspace))
            .collect();
        list.sort_by_key(|&(id, _, _)| id);
        list
    }

    /// Return the active workspace id.
    pub fn active_workspace_id(&self) -> u32 {
        self.active_workspace
    }

    /// Get sorted workspace ids for next/prev navigation.
    pub fn workspace_ids_sorted(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.workspaces.keys().copied().collect();
        ids.sort();
        ids
    }

    /// Rebuild the entire pane manager state from a session state message (for reattach).
    pub fn rebuild_from_state(
        session_name: String,
        workspaces: &[WorkspaceInfo],
        active_workspace: u32,
        rows: u16,
        cols: u16,
    ) -> Self {
        let mut ws_map = HashMap::new();
        let mut ws_names = HashMap::new();

        for ws_info in workspaces {
            let first_pane_id = ws_info.pane_ids.first().copied().unwrap_or(0);
            let layout = LayoutEngine::new_with_pane_id(rows, cols, PaneId(first_pane_id));
            let mut screens = HashMap::new();

            // Create screen buffers for all panes in this workspace.
            // For reattach, the first pane is the layout root; additional panes
            // would require splits but we don't know the layout structure from
            // just pane ids. For now, create the primary pane and screen.
            // The daemon will send output that will populate the screens.
            for &pid in &ws_info.pane_ids {
                screens.insert(
                    PaneId(pid),
                    ScreenBuffer::new(rows, cols, cmux_config::defaults::DEFAULT_SCROLLBACK),
                );
            }

            ws_map.insert(ws_info.id, WorkspaceState { layout, screens });
            ws_names.insert(ws_info.id, ws_info.name.clone());
        }

        // If no workspaces were provided, create a default one
        if ws_map.is_empty() {
            let layout = LayoutEngine::new(rows, cols);
            let mut screens = HashMap::new();
            screens.insert(
                PaneId(0),
                ScreenBuffer::new(rows, cols, cmux_config::defaults::DEFAULT_SCROLLBACK),
            );
            ws_map.insert(0, WorkspaceState { layout, screens });
            ws_names.insert(0, "0".into());
        }

        Self {
            workspaces: ws_map,
            workspace_names: ws_names,
            active_workspace,
            session_name,
        }
    }
}

fn sync_screen_sizes(layout: &mut LayoutEngine, screens: &mut HashMap<PaneId, ScreenBuffer>) {
    let rects = layout.pane_rects();
    for rect in &rects {
        if let Some(screen) = screens.get_mut(&rect.pane_id) {
            screen.resize(rect.height, rect.width);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_single_workspace() {
        let pm = PaneManager::new(24, 80);
        assert_eq!(pm.workspace_list().len(), 1);
        assert_eq!(pm.active_workspace_id(), 0);
        assert_eq!(pm.active_pane(), PaneId(0));
    }

    #[test]
    fn create_and_switch_workspace() {
        let mut pm = PaneManager::new(24, 80);
        pm.create_workspace(1, PaneId(5), "work".into());

        assert_eq!(pm.workspace_list().len(), 2);

        assert!(pm.switch_workspace(1));
        assert_eq!(pm.active_workspace_id(), 1);
        assert_eq!(pm.active_pane(), PaneId(5));

        assert!(pm.switch_workspace(0));
        assert_eq!(pm.active_pane(), PaneId(0));
    }

    #[test]
    fn switch_nonexistent_workspace_fails() {
        let mut pm = PaneManager::new(24, 80);
        assert!(!pm.switch_workspace(99));
    }

    #[test]
    fn close_workspace() {
        let mut pm = PaneManager::new(24, 80);
        pm.create_workspace(1, PaneId(5), "work".into());
        pm.switch_workspace(1);

        assert!(pm.close_workspace(1));
        assert_eq!(pm.workspace_list().len(), 1);
        // Should have switched to remaining workspace
        assert_eq!(pm.active_workspace_id(), 0);
    }

    #[test]
    fn close_nonexistent_workspace() {
        let mut pm = PaneManager::new(24, 80);
        assert!(!pm.close_workspace(99));
    }

    #[test]
    fn workspace_list_sorted() {
        let mut pm = PaneManager::new(24, 80);
        pm.create_workspace(3, PaneId(10), "three".into());
        pm.create_workspace(1, PaneId(5), "one".into());

        let list = pm.workspace_list();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].0, 0);
        assert_eq!(list[1].0, 1);
        assert_eq!(list[2].0, 3);
        // Active is 0
        assert!(list[0].2);
        assert!(!list[1].2);
        assert!(!list[2].2);
    }

    #[test]
    fn split_operates_on_active_workspace() {
        let mut pm = PaneManager::new(24, 80);
        pm.create_workspace(1, PaneId(5), "work".into());
        pm.switch_workspace(1);

        let new_pane = pm.split(SplitDirection::Vertical);
        assert_eq!(pm.active_pane(), new_pane);

        // Switch back and verify workspace 0 is unaffected
        pm.switch_workspace(0);
        assert_eq!(pm.active_pane(), PaneId(0));
        assert_eq!(pm.snapshots().len(), 1);
    }

    #[test]
    fn process_output_routes_to_correct_workspace() {
        let mut pm = PaneManager::new(24, 80);
        pm.create_workspace(1, PaneId(5), "work".into());

        // Send output to workspace 1's pane while workspace 0 is active
        pm.process_output(PaneId(5), b"hello");

        // Switch to workspace 1 and verify
        pm.switch_workspace(1);
        let snaps = pm.snapshots();
        assert!(snaps.contains_key(&PaneId(5)));
    }

    #[test]
    fn rebuild_from_state() {
        let ws_infos = vec![
            WorkspaceInfo {
                id: 0,
                name: "main".into(),
                pane_ids: vec![0],
            },
            WorkspaceInfo {
                id: 1,
                name: "work".into(),
                pane_ids: vec![3],
            },
        ];

        let pm = PaneManager::rebuild_from_state("test".into(), &ws_infos, 1, 24, 80);
        assert_eq!(pm.session_name(), "test");
        assert_eq!(pm.active_workspace_id(), 1);
        assert_eq!(pm.workspace_list().len(), 2);
    }

    #[test]
    fn rebuild_from_empty_state() {
        let pm = PaneManager::rebuild_from_state("test".into(), &[], 0, 24, 80);
        assert_eq!(pm.workspace_list().len(), 1);
        assert_eq!(pm.active_workspace_id(), 0);
    }

    #[test]
    fn workspace_ids_sorted() {
        let mut pm = PaneManager::new(24, 80);
        pm.create_workspace(5, PaneId(10), "five".into());
        pm.create_workspace(2, PaneId(20), "two".into());

        let ids = pm.workspace_ids_sorted();
        assert_eq!(ids, vec![0, 2, 5]);
    }

    #[test]
    fn resize_terminal_affects_all_workspaces() {
        let mut pm = PaneManager::new(24, 80);
        pm.create_workspace(1, PaneId(5), "work".into());

        pm.resize_terminal(48, 160);

        // Check workspace 0
        let snaps0 = pm.snapshots();
        let snap0 = snaps0.get(&PaneId(0)).unwrap();
        assert_eq!(snap0.rows, 48);
        assert_eq!(snap0.cols, 160);

        // Check workspace 1
        pm.switch_workspace(1);
        let snaps1 = pm.snapshots();
        let snap1 = snaps1.get(&PaneId(5)).unwrap();
        assert_eq!(snap1.rows, 48);
        assert_eq!(snap1.cols, 160);
    }

    #[test]
    fn session_name_stored() {
        let pm = PaneManager::new_with_session("my-session".into(), 24, 80);
        assert_eq!(pm.session_name(), "my-session");
    }

    #[test]
    fn pane_at_position_single_pane() {
        let pm = PaneManager::new(24, 80);
        // Top-left corner
        assert_eq!(pm.pane_at_position(0, 0), Some(PaneId(0)));
        // Bottom-right corner (just inside)
        assert_eq!(pm.pane_at_position(23, 79), Some(PaneId(0)));
        // Outside bounds
        assert_eq!(pm.pane_at_position(24, 0), None);
        assert_eq!(pm.pane_at_position(0, 80), None);
    }

    #[test]
    fn pane_at_position_vertical_split() {
        let mut pm = PaneManager::new(24, 80);
        let new_pane = pm.split(SplitDirection::Vertical);

        let rects = pm.layout().pane_rects();
        let left = &rects[0];
        let right = &rects[1];

        // Click inside left pane
        assert_eq!(pm.pane_at_position(0, 0), Some(PaneId(0)));
        assert_eq!(pm.pane_at_position(12, left.width / 2), Some(PaneId(0)));

        // Click inside right pane
        assert_eq!(pm.pane_at_position(0, right.col), Some(new_pane));
        assert_eq!(
            pm.pane_at_position(12, right.col + right.width / 2),
            Some(new_pane)
        );

        // Click on border (between panes) should return None
        let border_col = left.width; // the border column
        assert_eq!(pm.pane_at_position(0, border_col), None);
    }

    #[test]
    fn pane_at_position_horizontal_split() {
        let mut pm = PaneManager::new(24, 80);
        let new_pane = pm.split(SplitDirection::Horizontal);

        let rects = pm.layout().pane_rects();
        let top = &rects[0];
        let bottom = &rects[1];

        // Click inside top pane
        assert_eq!(pm.pane_at_position(0, 40), Some(PaneId(0)));

        // Click inside bottom pane
        assert_eq!(pm.pane_at_position(bottom.row, 40), Some(new_pane));

        // Click on border row should return None
        let border_row = top.height;
        assert_eq!(pm.pane_at_position(border_row, 40), None);
    }
}
