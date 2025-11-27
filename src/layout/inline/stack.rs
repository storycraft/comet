use core::mem;

use parley::Cluster;

use crate::ui::NodeKey;

pub struct InlineStack {
    states: Vec<InlineState>,
    start_offset: isize,
}

impl InlineStack {
    pub fn new() -> Self {
        Self {
            states: vec![],
            start_offset: 0,
        }
    }

    #[inline]
    pub fn push_state(&mut self, span: NodeKey) {
        self.states.push(InlineState {
            span,
            remaining_texts: mem::replace(&mut self.start_offset, 0),
        });
    }

    #[inline]
    pub fn spans(&mut self) -> impl Iterator<Item = NodeKey> {
        self.states.iter().map(|state| state.span)
    }

    pub fn add_texts(&mut self, texts: usize) {
        let Some(last) = self.states.last_mut() else {
            return;
        };

        last.remaining_texts += texts as isize;
    }

    pub fn read<'a>(&mut self, mut clusters: impl Iterator<Item = Cluster<'a, ()>>) -> ReadResult {
        let Some(mut last) = self.states.pop() else {
            return ReadResult::NoState;
        };
        if last.remaining_texts <= 0 {
            self.states.push(last);
            return ReadResult::EndRead;
        }

        let Some(mut cluster) = clusters.next() else {
            self.states.push(last);
            return ReadResult::Exhausted;
        };
        let start = cluster.path().logical_index();
        loop {
            last.remaining_texts -= cluster.text_range().len() as isize;
            if last.remaining_texts <= 0 {
                self.start_offset = last.remaining_texts;
                return ReadResult::Read {
                    start,
                    to: cluster.path().logical_index(),
                };
            }

            if let Some(next) = clusters.next() {
                cluster = next;
            } else {
                self.states.push(last);
                return ReadResult::Read {
                    start,
                    to: cluster.path().logical_index(),
                };
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ReadResult {
    NoState,
    Exhausted,
    Read { start: usize, to: usize },
    EndRead,
}

#[derive(Clone, Copy)]
struct InlineState {
    span: NodeKey,
    remaining_texts: isize,
}
