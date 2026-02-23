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
    pub fn hit_child(&self, parent: LayoutNodeKey, pos: (f64, f64)) -> Option<LayoutNodeKey> {
        for child_key in self.nodes.cursor(self.nodes.last_child(parent)).rev() {
            let Some(child) = self.nodes.get(child_key) else {
                continue;
            };

            if child.layout.hit(pos) {
                return Some(child_key);
            }
        }

        None
    }

    /// Starting from root node, iteratively search hit nodes.
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
        let node = self.next.take()?;
        self.next_pos = self.tree.hit(node, self.next_pos)?;
        self.next = self.tree.hit_child(node, self.next_pos);

        Some(HitResult {
            node,
            inner_pos: self.next_pos,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitResult {
    pub node: LayoutNodeKey,
    pub inner_pos: (f64, f64),
}
