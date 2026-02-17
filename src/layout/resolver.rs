use anyrender::Paint;
use comet_div::ui::NodeKey;
use parley::TextStyle;
use rustc_hash::FxHashMap;
use slotmap::{SlotMap, new_key_type};

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
}

pub struct InlineStyle {
    pub span: NodeKey,
    pub color: Paint,
    pub text: TextStyle<'static, ()>,
}

fn find_nearest_span() -> Option<NodeKey> {
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
