use anyrender::Paint;
use parley::TextStyle;
use rustc_hash::FxHashMap;
use slotmap::{SlotMap, new_key_type};

use crate::{
    layout::{LayoutBoxKey, tree::LayoutBoxTree},
    ui::NodeKey,
};

new_key_type! { pub struct InlineStyleKey; }

pub struct InlineStyleResolver {
    pub styles: SlotMap<InlineStyleKey, InlineStyle>,
    map: FxHashMap<u32, InlineStyleKey>,
}

impl Default for InlineStyleResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineStyleResolver {
    pub fn new() -> Self {
        Self {
            styles: SlotMap::with_key(),
            map: FxHashMap::default(),
        }
    }

    pub fn resolve(
        &mut self,
        tree: &LayoutBoxTree,
        root: LayoutBoxKey,
        key: LayoutBoxKey,
    ) -> Option<InlineStyleKey> {
        let span = find_nearest_span(tree, key)?;

        None
    }

    fn resolve_inner(&mut self, tree: &LayoutBoxTree, key: NodeKey) {}
}

pub struct InlineStyle {
    pub span: NodeKey,
    pub color: Paint,
    pub text: TextStyle<'static, ()>,
}

fn find_nearest_span(tree: &LayoutBoxTree, key: LayoutBoxKey) -> Option<NodeKey> {
    todo!()
    // let node = tree.boxes.get(key)?;
    // match node.span {
    //     Some(span) => Some(span),
    //     None => {
    //         if let Some(parent) = tree.boxes.parent(key) {
    //             find_nearest_span(tree, parent)
    //         } else {
    //             None
    //         }
    //     }
    // }
}
