use crate::tree::{LayoutNodeKey, LayoutTree};

impl LayoutTree {
    /// Perform hit testing of outer coordinate.
    /// Returns inner coordinate on success.
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

    /// Perform hit testing relative to parent node.
    pub fn hit_child(&self, parent: LayoutNodeKey, pos: (f64, f64)) -> Option<HitResult> {
        for child_key in self.nodes.cursor(self.nodes.last_child(parent)).rev() {
            if let Some(inner_pos) = self.hit(child_key, pos) {
                return Some(HitResult {
                    node: child_key,
                    inner_pos,
                });
            }
        }

        None
    }

    /// From root node, iteratively search for hit child nodes.
    pub fn hit_iter(&self, root: LayoutNodeKey, pos: (f64, f64)) -> HitIter<'_> {
        HitIter {
            tree: self,
            next: Some(root),
            next_pos: pos,
        }
    }
}

pub struct HitIter<'a> {
    tree: &'a LayoutTree,
    next: Option<LayoutNodeKey>,
    next_pos: (f64, f64),
}

impl Iterator for HitIter<'_> {
    type Item = HitResult;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.tree.hit_child(self.next.take()?, self.next_pos)?;
        self.next = Some(next.node);
        self.next_pos = next.inner_pos;

        Some(next)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitResult {
    /// Target node
    pub node: LayoutNodeKey,
    /// Hit position relative to node
    pub inner_pos: (f64, f64),
}
