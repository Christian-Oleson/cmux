use cmux_core::layout::{LayoutEngine, SplitDirection};
use cmux_core::screen::{ScreenBuffer, ScreenSnapshot};
use cmux_core::types::PaneId;
use std::collections::HashMap;

pub struct PaneManager {
    layout: LayoutEngine,
    screens: HashMap<PaneId, ScreenBuffer>,
}

impl PaneManager {
    pub fn new(rows: u16, cols: u16) -> Self {
        let layout = LayoutEngine::new(rows, cols);
        let mut screens = HashMap::new();
        // Initial pane 0 gets full terminal dimensions
        screens.insert(PaneId(0), ScreenBuffer::new(rows, cols));
        Self { layout, screens }
    }

    pub fn process_output(&mut self, pane_id: PaneId, data: &[u8]) {
        if let Some(screen) = self.screens.get_mut(&pane_id) {
            screen.process(data);
        }
    }

    pub fn split(&mut self, direction: SplitDirection) -> PaneId {
        let new_pane_id = self.layout.split(direction);

        // Resize all screen buffers to match new layout
        self.sync_screen_sizes();

        // Create screen buffer for new pane
        let rects = self.layout.pane_rects();
        if let Some(rect) = rects.iter().find(|r| r.pane_id == new_pane_id) {
            self.screens
                .insert(new_pane_id, ScreenBuffer::new(rect.height, rect.width));
        }

        new_pane_id
    }

    pub fn close_pane(&mut self, pane_id: PaneId) -> bool {
        if !self.layout.close_pane(pane_id) {
            return false;
        }
        self.screens.remove(&pane_id);
        self.sync_screen_sizes();
        true
    }

    pub fn resize_terminal(&mut self, rows: u16, cols: u16) {
        self.layout.resize_terminal(rows, cols);
        self.sync_screen_sizes();
    }

    fn sync_screen_sizes(&mut self) {
        let rects = self.layout.pane_rects();
        for rect in &rects {
            if let Some(screen) = self.screens.get_mut(&rect.pane_id) {
                screen.resize(rect.height, rect.width);
            }
        }
    }

    pub fn layout(&self) -> &LayoutEngine {
        &self.layout
    }

    pub fn layout_mut(&mut self) -> &mut LayoutEngine {
        &mut self.layout
    }

    pub fn active_pane(&self) -> PaneId {
        self.layout.active_pane()
    }

    pub fn snapshots(&self) -> HashMap<PaneId, ScreenSnapshot> {
        self.screens
            .iter()
            .map(|(id, screen)| (*id, screen.snapshot()))
            .collect()
    }
}
