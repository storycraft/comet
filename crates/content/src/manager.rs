//! The contents of a UI, by id.

use std::num::NonZeroU64;

use rustc_hash::FxHashMap;

use crate::{Content, ContentId};

/// Contents by [`ContentId`]. Boxes refer to them by id, so one content can
/// be drawn by any number of boxes.
///
/// The manager tracks no boxes: boxes using a content that changed size are
/// laid out again by marking their items dirty.
pub struct ContentManager {
    contents: FxHashMap<ContentId, Box<dyn Content>>,
    /// Id given to the next inserted content.
    next: NonZeroU64,
}

impl Default for ContentManager {
    fn default() -> Self {
        Self {
            contents: FxHashMap::default(),
            next: NonZeroU64::MIN,
        }
    }
}

impl ContentManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `content` under a new id.
    pub fn insert(&mut self, content: impl Content + 'static) -> ContentId {
        self.insert_boxed(Box::new(content))
    }

    /// Add `content` under a new id.
    pub fn insert_boxed(&mut self, content: Box<dyn Content>) -> ContentId {
        let id = ContentId::from_ffi(self.next);
        self.next = self.next.checked_add(1).expect("content ids exhausted");
        self.contents.insert(id, content);

        id
    }

    /// Replace the content of `id`, keeping its id, and return the old one.
    /// Does nothing and returns `None` if `id` is not in the manager.
    pub fn replace(
        &mut self,
        id: ContentId,
        content: impl Content + 'static,
    ) -> Option<Box<dyn Content>> {
        let entry = self.contents.get_mut(&id)?;

        Some(std::mem::replace(entry, Box::new(content)))
    }

    /// Remove the content of `id`. Its id is not given again.
    pub fn remove(&mut self, id: ContentId) -> Option<Box<dyn Content>> {
        self.contents.remove(&id)
    }

    pub fn get(&self, id: ContentId) -> Option<&dyn Content> {
        self.contents.get(&id).map(Box::as_ref)
    }

    pub fn contains(&self, id: ContentId) -> bool {
        self.contents.contains_key(&id)
    }

    pub fn len(&self) -> usize {
        self.contents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }

    /// The contents with their ids, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (ContentId, &dyn Content)> {
        self.contents
            .iter()
            .map(|(&id, content)| (id, content.as_ref()))
    }
}
