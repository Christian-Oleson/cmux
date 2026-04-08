use crate::types::PaneId;
use serde::{Deserialize, Serialize};

/// Direction of a split between two panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDirection {
    /// Top / bottom split.
    Horizontal,
    /// Left / right split.
    Vertical,
}

/// A binary tree representing the pane layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayoutNode {
    Leaf {
        pane_id: PaneId,
    },
    Split {
        direction: SplitDirection,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

/// Position and size of a pane in terminal coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneRect {
    pub pane_id: PaneId,
    pub row: u16,
    pub col: u16,
    pub height: u16,
    pub width: u16,
}

/// Manages the tree-based pane layout for a workspace.
pub struct LayoutEngine {
    root: LayoutNode,
    terminal_rows: u16,
    terminal_cols: u16,
    next_pane_id: u32,
    active_pane: PaneId,
    zoomed_pane: Option<PaneId>,
}

impl LayoutEngine {
    /// Create a new layout engine with a single pane filling the terminal.
    pub fn new(rows: u16, cols: u16) -> Self {
        Self::new_with_pane_id(rows, cols, PaneId(0))
    }

    /// Create a new layout engine with a single pane using the given pane id.
    pub fn new_with_pane_id(rows: u16, cols: u16, pane_id: PaneId) -> Self {
        Self {
            root: LayoutNode::Leaf { pane_id },
            terminal_rows: rows,
            terminal_cols: cols,
            next_pane_id: pane_id.0 + 1,
            active_pane: pane_id,
            zoomed_pane: None,
        }
    }

    /// Return the rectangles for all visible panes.
    ///
    /// If a pane is zoomed, only that pane is returned at full terminal size.
    pub fn pane_rects(&self) -> Vec<PaneRect> {
        if let Some(zoomed) = self.zoomed_pane {
            return vec![PaneRect {
                pane_id: zoomed,
                row: 0,
                col: 0,
                height: self.terminal_rows,
                width: self.terminal_cols,
            }];
        }

        let mut rects = Vec::new();
        Self::collect_rects(
            &self.root,
            0,
            0,
            self.terminal_rows,
            self.terminal_cols,
            &mut rects,
        );
        rects
    }

    fn collect_rects(
        node: &LayoutNode,
        area_row: u16,
        area_col: u16,
        area_height: u16,
        area_width: u16,
        out: &mut Vec<PaneRect>,
    ) {
        match node {
            LayoutNode::Leaf { pane_id } => {
                out.push(PaneRect {
                    pane_id: *pane_id,
                    row: area_row,
                    col: area_col,
                    height: area_height,
                    width: area_width,
                });
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => match direction {
                SplitDirection::Horizontal => {
                    if area_height < 3 {
                        // Too small to split — treat as first child only.
                        Self::collect_rects(
                            first,
                            area_row,
                            area_col,
                            area_height,
                            area_width,
                            out,
                        );
                        return;
                    }
                    let first_height = ((area_height - 1) as f32 * ratio) as u16;
                    let first_height = first_height.max(1);
                    let second_height = area_height.saturating_sub(1 + first_height);
                    let second_height = second_height.max(1);
                    Self::collect_rects(first, area_row, area_col, first_height, area_width, out);
                    Self::collect_rects(
                        second,
                        area_row + first_height + 1,
                        area_col,
                        second_height,
                        area_width,
                        out,
                    );
                }
                SplitDirection::Vertical => {
                    if area_width < 3 {
                        Self::collect_rects(
                            first,
                            area_row,
                            area_col,
                            area_height,
                            area_width,
                            out,
                        );
                        return;
                    }
                    let first_width = ((area_width - 1) as f32 * ratio) as u16;
                    let first_width = first_width.max(1);
                    let second_width = area_width.saturating_sub(1 + first_width);
                    let second_width = second_width.max(1);
                    Self::collect_rects(first, area_row, area_col, area_height, first_width, out);
                    Self::collect_rects(
                        second,
                        area_row,
                        area_col + first_width + 1,
                        area_height,
                        second_width,
                        out,
                    );
                }
            },
        }
    }

    /// Split the active pane in the given direction, returning the new pane id.
    pub fn split(&mut self, direction: SplitDirection) -> PaneId {
        let new_id = PaneId(self.next_pane_id);
        self.next_pane_id += 1;

        let target = self.active_pane;
        Self::split_leaf(&mut self.root, target, direction, new_id);

        self.active_pane = new_id;
        new_id
    }

    /// Recursively find the leaf with `target` id and replace it with a split.
    fn split_leaf(
        node: &mut LayoutNode,
        target: PaneId,
        direction: SplitDirection,
        new_id: PaneId,
    ) -> bool {
        match node {
            LayoutNode::Leaf { pane_id } => {
                if *pane_id == target {
                    let old_leaf = LayoutNode::Leaf { pane_id: *pane_id };
                    let new_leaf = LayoutNode::Leaf { pane_id: new_id };
                    *node = LayoutNode::Split {
                        direction,
                        ratio: 0.5,
                        first: Box::new(old_leaf),
                        second: Box::new(new_leaf),
                    };
                    true
                } else {
                    false
                }
            }
            LayoutNode::Split { first, second, .. } => {
                Self::split_leaf(first, target, direction, new_id)
                    || Self::split_leaf(second, target, direction, new_id)
            }
        }
    }

    /// Close a pane by id. Returns false if it is the last remaining pane.
    ///
    /// The split containing the pane is replaced with the sibling subtree.
    /// If the closed pane was active, the active pane is set to the first leaf
    /// of the sibling.
    pub fn close_pane(&mut self, pane_id: PaneId) -> bool {
        // Cannot close the only remaining pane.
        if matches!(self.root, LayoutNode::Leaf { .. }) {
            return false;
        }

        if !Self::remove_pane(&mut self.root, pane_id) {
            return false;
        }

        // Clear zoom on close.
        if self.zoomed_pane == Some(pane_id) {
            self.zoomed_pane = None;
        }

        // If the closed pane was active, pick the first leaf in the tree.
        if self.active_pane == pane_id {
            self.active_pane = Self::first_leaf(&self.root);
        }

        true
    }

    /// Remove the leaf with `pane_id` from the tree, replacing the parent split
    /// with the sibling subtree. Returns true if found.
    fn remove_pane(node: &mut LayoutNode, pane_id: PaneId) -> bool {
        match node {
            LayoutNode::Leaf { .. } => false,
            LayoutNode::Split { first, second, .. } => {
                // Check if `first` is the target leaf.
                if let LayoutNode::Leaf { pane_id: id } = first.as_ref() {
                    if *id == pane_id {
                        // Replace this split with the second child.
                        *node = *second.clone();
                        return true;
                    }
                }
                // Check if `second` is the target leaf.
                if let LayoutNode::Leaf { pane_id: id } = second.as_ref() {
                    if *id == pane_id {
                        *node = *first.clone();
                        return true;
                    }
                }
                // Recurse.
                Self::remove_pane(first, pane_id) || Self::remove_pane(second, pane_id)
            }
        }
    }

    fn first_leaf(node: &LayoutNode) -> PaneId {
        match node {
            LayoutNode::Leaf { pane_id } => *pane_id,
            LayoutNode::Split { first, .. } => Self::first_leaf(first),
        }
    }

    /// Navigate to an adjacent pane in the given direction.
    ///
    /// `forward` means right (Vertical) or down (Horizontal).
    pub fn navigate(&mut self, direction: SplitDirection, forward: bool) {
        let rects = self.pane_rects();
        let active_rect = match rects.iter().find(|r| r.pane_id == self.active_pane) {
            Some(r) => r.clone(),
            None => return,
        };

        let mut best: Option<&PaneRect> = None;
        let mut best_dist = i32::MAX;

        for r in &rects {
            if r.pane_id == self.active_pane {
                continue;
            }

            match (direction, forward) {
                // Right: candidate must be to the right.
                (SplitDirection::Vertical, true) => {
                    if r.col <= active_rect.col {
                        continue;
                    }
                    let dist = (r.col as i32 - active_rect.col as i32).abs();
                    let overlap =
                        Self::range_overlap(active_rect.row, active_rect.height, r.row, r.height);
                    if overlap > 0 && dist < best_dist {
                        best_dist = dist;
                        best = Some(r);
                    }
                }
                // Left.
                (SplitDirection::Vertical, false) => {
                    if r.col >= active_rect.col {
                        continue;
                    }
                    let dist = (active_rect.col as i32 - r.col as i32).abs();
                    let overlap =
                        Self::range_overlap(active_rect.row, active_rect.height, r.row, r.height);
                    if overlap > 0 && dist < best_dist {
                        best_dist = dist;
                        best = Some(r);
                    }
                }
                // Down.
                (SplitDirection::Horizontal, true) => {
                    if r.row <= active_rect.row {
                        continue;
                    }
                    let dist = (r.row as i32 - active_rect.row as i32).abs();
                    let overlap =
                        Self::range_overlap(active_rect.col, active_rect.width, r.col, r.width);
                    if overlap > 0 && dist < best_dist {
                        best_dist = dist;
                        best = Some(r);
                    }
                }
                // Up.
                (SplitDirection::Horizontal, false) => {
                    if r.row >= active_rect.row {
                        continue;
                    }
                    let dist = (active_rect.row as i32 - r.row as i32).abs();
                    let overlap =
                        Self::range_overlap(active_rect.col, active_rect.width, r.col, r.width);
                    if overlap > 0 && dist < best_dist {
                        best_dist = dist;
                        best = Some(r);
                    }
                }
            }
        }

        if let Some(target) = best {
            self.active_pane = target.pane_id;
        }
    }

    /// Compute the overlap length of two 1-D ranges [a_start, a_start+a_len) and
    /// [b_start, b_start+b_len).
    fn range_overlap(a_start: u16, a_len: u16, b_start: u16, b_len: u16) -> i32 {
        let a_end = a_start as i32 + a_len as i32;
        let b_end = b_start as i32 + b_len as i32;
        let start = (a_start as i32).max(b_start as i32);
        let end = a_end.min(b_end);
        (end - start).max(0)
    }

    /// Cycle to the next or previous pane in DFS order, wrapping around.
    pub fn cycle_pane(&mut self, forward: bool) {
        let ids = self.pane_ids();
        if ids.is_empty() {
            return;
        }
        let pos = ids.iter().position(|id| *id == self.active_pane);
        let idx = match pos {
            Some(i) => {
                if forward {
                    (i + 1) % ids.len()
                } else {
                    (i + ids.len() - 1) % ids.len()
                }
            }
            None => 0,
        };
        self.active_pane = ids[idx];
    }

    /// Resize the active pane by adjusting the ratio of the nearest ancestor
    /// split with the matching direction.
    ///
    /// `amount` is in terminal cells: positive grows the first child,
    /// negative shrinks it.
    pub fn resize_pane(&mut self, direction: SplitDirection, amount: i16) {
        let target = self.active_pane;
        Self::adjust_ratio(
            &mut self.root,
            target,
            direction,
            amount,
            self.terminal_rows,
            self.terminal_cols,
        );
    }

    /// Walk the tree looking for a Split whose `direction` matches that contains
    /// `target` (directly or indirectly), then adjust its ratio.
    fn adjust_ratio(
        node: &mut LayoutNode,
        target: PaneId,
        direction: SplitDirection,
        amount: i16,
        area_height: u16,
        area_width: u16,
    ) -> bool {
        match node {
            LayoutNode::Leaf { pane_id } => *pane_id == target,
            LayoutNode::Split {
                direction: d,
                ratio,
                first,
                second,
            } => {
                // Compute child areas.
                let (first_h, first_w, second_h, second_w) = match d {
                    SplitDirection::Horizontal => {
                        let fh = ((area_height.saturating_sub(1)) as f32 * *ratio) as u16;
                        let sh = area_height.saturating_sub(1 + fh);
                        (fh, area_width, sh, area_width)
                    }
                    SplitDirection::Vertical => {
                        let fw = ((area_width.saturating_sub(1)) as f32 * *ratio) as u16;
                        let sw = area_width.saturating_sub(1 + fw);
                        (area_height, fw, area_height, sw)
                    }
                };

                let in_first = Self::contains_pane(first, target);
                let in_second = Self::contains_pane(second, target);

                if !in_first && !in_second {
                    return false;
                }

                // If this split's direction matches and the target is somewhere
                // inside, adjust the ratio here.
                if *d == direction {
                    let dimension = match direction {
                        SplitDirection::Horizontal => area_height,
                        SplitDirection::Vertical => area_width,
                    };
                    if dimension > 0 {
                        let delta = amount as f32 / dimension as f32;
                        *ratio = (*ratio + delta).clamp(0.1, 0.9);
                    }
                    return true;
                }

                // Direction doesn't match — recurse deeper.
                if in_first {
                    Self::adjust_ratio(first, target, direction, amount, first_h, first_w)
                } else {
                    Self::adjust_ratio(second, target, direction, amount, second_h, second_w)
                }
            }
        }
    }

    fn contains_pane(node: &LayoutNode, target: PaneId) -> bool {
        match node {
            LayoutNode::Leaf { pane_id } => *pane_id == target,
            LayoutNode::Split { first, second, .. } => {
                Self::contains_pane(first, target) || Self::contains_pane(second, target)
            }
        }
    }

    /// Toggle zoom on the active pane.
    ///
    /// When zoomed, `pane_rects` returns only the zoomed pane at full terminal
    /// size. Toggling again restores the normal layout.
    pub fn toggle_zoom(&mut self) {
        if self.zoomed_pane.is_some() {
            self.zoomed_pane = None;
        } else {
            self.zoomed_pane = Some(self.active_pane);
        }
    }

    /// Update the terminal dimensions. Does not change the tree structure.
    pub fn resize_terminal(&mut self, rows: u16, cols: u16) {
        self.terminal_rows = rows;
        self.terminal_cols = cols;
    }

    /// Return all pane ids in left-to-right depth-first order.
    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut ids = Vec::new();
        Self::collect_pane_ids(&self.root, &mut ids);
        ids
    }

    fn collect_pane_ids(node: &LayoutNode, out: &mut Vec<PaneId>) {
        match node {
            LayoutNode::Leaf { pane_id } => out.push(*pane_id),
            LayoutNode::Split { first, second, .. } => {
                Self::collect_pane_ids(first, out);
                Self::collect_pane_ids(second, out);
            }
        }
    }

    /// Return the currently active pane id.
    pub fn active_pane(&self) -> PaneId {
        self.active_pane
    }

    /// Set the active pane id.
    pub fn set_active_pane(&mut self, id: PaneId) {
        self.active_pane = id;
    }

    /// Compute the border cells between panes.
    ///
    /// Returns `(row, col, character)` tuples for every border position.
    /// Horizontal borders use `'─'`, vertical borders use `'│'`, and
    /// intersections use `'┼'`.
    pub fn border_cells(&self) -> Vec<(u16, u16, char)> {
        if self.zoomed_pane.is_some() {
            return Vec::new();
        }
        let mut cells = Vec::new();
        Self::collect_borders(
            &self.root,
            0,
            0,
            self.terminal_rows,
            self.terminal_cols,
            &mut cells,
        );
        // De-duplicate and resolve intersections.
        Self::resolve_intersections(&mut cells);
        cells
    }

    fn collect_borders(
        node: &LayoutNode,
        area_row: u16,
        area_col: u16,
        area_height: u16,
        area_width: u16,
        out: &mut Vec<(u16, u16, char)>,
    ) {
        match node {
            LayoutNode::Leaf { .. } => {}
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                match direction {
                    SplitDirection::Horizontal => {
                        if area_height < 3 {
                            return;
                        }
                        let first_height = ((area_height - 1) as f32 * ratio) as u16;
                        let first_height = first_height.max(1);
                        let border_row = area_row + first_height;
                        let second_height = area_height.saturating_sub(1 + first_height);
                        let second_height = second_height.max(1);

                        // Draw horizontal border line.
                        for c in area_col..area_col + area_width {
                            out.push((border_row, c, '─'));
                        }

                        // Recurse into children.
                        Self::collect_borders(
                            first,
                            area_row,
                            area_col,
                            first_height,
                            area_width,
                            out,
                        );
                        Self::collect_borders(
                            second,
                            border_row + 1,
                            area_col,
                            second_height,
                            area_width,
                            out,
                        );
                    }
                    SplitDirection::Vertical => {
                        if area_width < 3 {
                            return;
                        }
                        let first_width = ((area_width - 1) as f32 * ratio) as u16;
                        let first_width = first_width.max(1);
                        let border_col = area_col + first_width;
                        let second_width = area_width.saturating_sub(1 + first_width);
                        let second_width = second_width.max(1);

                        // Draw vertical border line.
                        for r in area_row..area_row + area_height {
                            out.push((r, border_col, '│'));
                        }

                        // Recurse into children.
                        Self::collect_borders(
                            first,
                            area_row,
                            area_col,
                            area_height,
                            first_width,
                            out,
                        );
                        Self::collect_borders(
                            second,
                            area_row,
                            border_col + 1,
                            area_height,
                            second_width,
                            out,
                        );
                    }
                }
            }
        }
    }

    /// Where a horizontal and vertical border overlap, replace with '┼'.
    fn resolve_intersections(cells: &mut Vec<(u16, u16, char)>) {
        use std::collections::HashMap;

        // Group by (row, col).
        let mut map: HashMap<(u16, u16), Vec<char>> = HashMap::new();
        for &(r, c, ch) in cells.iter() {
            map.entry((r, c)).or_default().push(ch);
        }

        // Replace entries where both '─' and '│' appear.
        cells.retain(|&(r, c, _)| map.contains_key(&(r, c)));
        cells.dedup_by_key(|&mut (r, c, _)| (r, c));

        for cell in cells.iter_mut() {
            let chars = &map[&(cell.0, cell.1)];
            if chars.contains(&'─') && chars.contains(&'│') {
                cell.2 = '┼';
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_single_pane() {
        let engine = LayoutEngine::new(24, 80);
        let rects = engine.pane_rects();
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].pane_id, PaneId(0));
        assert_eq!(rects[0].row, 0);
        assert_eq!(rects[0].col, 0);
        assert_eq!(rects[0].height, 24);
        assert_eq!(rects[0].width, 80);
    }

    #[test]
    fn split_vertical() {
        let mut engine = LayoutEngine::new(24, 80);
        let new_id = engine.split(SplitDirection::Vertical);
        assert_eq!(new_id, PaneId(1));

        let rects = engine.pane_rects();
        assert_eq!(rects.len(), 2);

        // First pane (left) + border (1) + second pane (right) = 80.
        let left = &rects[0];
        let right = &rects[1];
        assert_eq!(left.col, 0);
        assert!(right.col > left.col);
        assert_eq!(left.width + 1 + right.width, 80);
        assert_eq!(left.height, 24);
        assert_eq!(right.height, 24);
    }

    #[test]
    fn split_horizontal() {
        let mut engine = LayoutEngine::new(24, 80);
        let new_id = engine.split(SplitDirection::Horizontal);
        assert_eq!(new_id, PaneId(1));

        let rects = engine.pane_rects();
        assert_eq!(rects.len(), 2);

        let top = &rects[0];
        let bottom = &rects[1];
        assert_eq!(top.row, 0);
        assert!(bottom.row > top.row);
        assert_eq!(top.height + 1 + bottom.height, 24);
        assert_eq!(top.width, 80);
        assert_eq!(bottom.width, 80);
    }

    #[test]
    fn nested_split() {
        let mut engine = LayoutEngine::new(24, 80);
        // Split pane 0 vertically -> pane 1 (now active).
        engine.split(SplitDirection::Vertical);
        // Split pane 1 horizontally -> pane 2 (now active).
        engine.split(SplitDirection::Horizontal);

        let rects = engine.pane_rects();
        assert_eq!(rects.len(), 3);

        // All three should have distinct pane ids.
        let ids: Vec<PaneId> = rects.iter().map(|r| r.pane_id).collect();
        assert!(ids.contains(&PaneId(0)));
        assert!(ids.contains(&PaneId(1)));
        assert!(ids.contains(&PaneId(2)));
    }

    #[test]
    fn close_pane_removes() {
        let mut engine = LayoutEngine::new(24, 80);
        let new_id = engine.split(SplitDirection::Vertical);
        assert_eq!(engine.pane_rects().len(), 2);

        let closed = engine.close_pane(new_id);
        assert!(closed);
        let rects = engine.pane_rects();
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].pane_id, PaneId(0));
        // Sibling should expand to fill.
        assert_eq!(rects[0].width, 80);
        assert_eq!(rects[0].height, 24);
    }

    #[test]
    fn close_last_pane_fails() {
        let mut engine = LayoutEngine::new(24, 80);
        assert!(!engine.close_pane(PaneId(0)));
    }

    #[test]
    fn cycle_pane_wraps() {
        let mut engine = LayoutEngine::new(24, 80);
        engine.split(SplitDirection::Vertical);
        // Active is now PaneId(1).
        assert_eq!(engine.active_pane(), PaneId(1));

        // Cycle forward should wrap to PaneId(0).
        engine.cycle_pane(true);
        assert_eq!(engine.active_pane(), PaneId(0));

        // Cycle forward again back to PaneId(1).
        engine.cycle_pane(true);
        assert_eq!(engine.active_pane(), PaneId(1));

        // Cycle backward should go to PaneId(0).
        engine.cycle_pane(false);
        assert_eq!(engine.active_pane(), PaneId(0));

        // Cycle backward should wrap to PaneId(1).
        engine.cycle_pane(false);
        assert_eq!(engine.active_pane(), PaneId(1));
    }

    #[test]
    fn resize_pane_adjusts_ratio() {
        let mut engine = LayoutEngine::new(24, 80);
        engine.split(SplitDirection::Vertical);
        // Active is pane 1 (right side).

        let rects_before = engine.pane_rects();
        let left_before = rects_before[0].width;

        // Grow the active pane's split by 10 columns.
        engine.resize_pane(SplitDirection::Vertical, 10);

        let rects_after = engine.pane_rects();
        let left_after = rects_after[0].width;

        // The left pane should have grown (ratio increased means first child
        // gets more space).
        assert_ne!(left_before, left_after);

        // Ratio should be clamped.
        engine.resize_pane(SplitDirection::Vertical, 1000);
        let rects_clamped = engine.pane_rects();
        let total = rects_clamped[0].width + 1 + rects_clamped[1].width;
        assert_eq!(total, 80);
    }

    #[test]
    fn zoom_returns_single_rect() {
        let mut engine = LayoutEngine::new(24, 80);
        engine.split(SplitDirection::Vertical);
        assert_eq!(engine.pane_rects().len(), 2);

        engine.toggle_zoom();
        let rects = engine.pane_rects();
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].pane_id, engine.active_pane());
        assert_eq!(rects[0].height, 24);
        assert_eq!(rects[0].width, 80);

        // Toggle again restores.
        engine.toggle_zoom();
        assert_eq!(engine.pane_rects().len(), 2);
    }

    #[test]
    fn resize_terminal_updates() {
        let mut engine = LayoutEngine::new(24, 80);
        engine.split(SplitDirection::Vertical);

        let rects_before = engine.pane_rects();

        engine.resize_terminal(48, 160);
        let rects_after = engine.pane_rects();

        // Pane dimensions should change.
        assert_ne!(rects_before[0].width, rects_after[0].width);
        assert_ne!(rects_before[0].height, rects_after[0].height);

        // Should still sum correctly.
        assert_eq!(rects_after[0].width + 1 + rects_after[1].width, 160);
        assert_eq!(rects_after[0].height, 48);
    }

    #[test]
    fn border_cells_vertical() {
        let mut engine = LayoutEngine::new(24, 80);
        engine.split(SplitDirection::Vertical);

        let borders = engine.border_cells();
        assert!(!borders.is_empty());

        // All border cells should be '│' for a single vertical split.
        for &(_, _, ch) in &borders {
            assert_eq!(ch, '│');
        }

        // There should be exactly `terminal_rows` border cells (one per row).
        assert_eq!(borders.len(), 24);
    }

    #[test]
    fn border_cells_horizontal() {
        let mut engine = LayoutEngine::new(24, 80);
        engine.split(SplitDirection::Horizontal);

        let borders = engine.border_cells();
        assert!(!borders.is_empty());

        // All border cells should be '─' for a single horizontal split.
        for &(_, _, ch) in &borders {
            assert_eq!(ch, '─');
        }

        // There should be exactly `terminal_cols` border cells (one per col).
        assert_eq!(borders.len(), 80);
    }

    #[test]
    fn pane_dimensions_account_for_border() {
        let mut engine = LayoutEngine::new(24, 80);
        engine.split(SplitDirection::Vertical);

        let rects = engine.pane_rects();
        let total_width: u16 = rects.iter().map(|r| r.width).sum::<u16>() + 1; // +1 for border
        assert_eq!(total_width, 80);

        let mut engine2 = LayoutEngine::new(24, 80);
        engine2.split(SplitDirection::Horizontal);

        let rects2 = engine2.pane_rects();
        let total_height: u16 = rects2.iter().map(|r| r.height).sum::<u16>() + 1;
        assert_eq!(total_height, 24);
    }
}
