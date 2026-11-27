//! Extracting the selected text.

use comet_paint::selection::{Selection, text::SelectedText};

use super::Window;

impl Window {
    /// The text `selection` covers, from the last layout, in tree order.
    /// See [`SelectedText`] for what is left out and where line breaks go.
    pub fn selected_text(&self, selection: Selection) -> String {
        self.with_selected_text(selection, |text| text.collect())
    }

    /// Call `f` with the text `selection` covers, from the last layout, as
    /// chunks in tree order: see [`SelectedText`]. The UI tree is borrowed
    /// during the call.
    pub fn with_selected_text<R>(
        &self,
        selection: Selection,
        f: impl FnOnce(SelectedText<'_>) -> R,
    ) -> R {
        let trees = &self.pipeline.trees;
        let tree = self.ui.tree().borrow();
        let root = trees.box_fragment(self.ui.root()).unwrap_or_default();

        f(SelectedText::new(
            &tree.ui,
            &trees.inputs,
            &trees.fragments,
            &trees.prepaint,
            root,
            selection,
        ))
    }
}
