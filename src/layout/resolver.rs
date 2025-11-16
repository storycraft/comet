use parley::TextStyle;
use slotmap::SecondaryMap;

use crate::layout::LayoutBoxKey;

pub struct TextStyleResolver {
    map: SecondaryMap<LayoutBoxKey, TextStyle<'static, ()>>,
}
