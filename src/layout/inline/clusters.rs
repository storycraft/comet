use core::iter::Peekable;

use parley::{Cluster, Run};

pub struct LineClusters<I: Iterator> {
    runs: Peekable<I>,
    index: usize,
}

impl<'a, I> LineClusters<I>
where
    I: Iterator<Item = Run<'a, ()>>,
{
    pub fn new(runs: I) -> Self {
        Self {
            runs: runs.peekable(),
            index: 0,
        }
    }
}

impl<'a, I> Iterator for LineClusters<I>
where
    I: Iterator<Item = Run<'a, ()>>,
{
    type Item = Cluster<'a, ()>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let run = self.runs.peek()?;
            // parley doesn't have length check so it may return next cluster without range check
            if self.index < run.len()
                && let Some(cluster) = run.get(self.index)
            {
                self.index += 1;
                return Some(cluster);
            }

            self.index = 0;
            _ = self.runs.next();
        }
    }
}
