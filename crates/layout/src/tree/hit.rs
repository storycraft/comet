use crate::tree::{LayoutNodeKey, LayoutTree};

impl LayoutTree {
    /// Perform layout box hit testing using outer coordinate.
    /// Returns local inner coordinate on success.
    pub fn hit(&self, key: LayoutNodeKey, pos: (f64, f64)) -> Option<(f64, f64)> {
        let node = self.nodes.get(key)?;
        if !node.layout.hit(pos) {
            return None;
        }

        let inner_pos = (
            pos.0 - node.layout.location.x,
            pos.1 - node.layout.location.y,
        );
        Some(inner_pos)
    }
}
