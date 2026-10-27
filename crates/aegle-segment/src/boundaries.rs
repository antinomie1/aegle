//! Word starts as byte offsets, with the queries editing needs.

use std::ops::Range;

/// Where words start in one text, as UTF-8 byte offsets.
pub struct Boundaries {
    starts: Vec<usize>,
    /// Whether the byte before `starts[i]` is whitespace, so stepping forward
    /// can run through the spaces between words.
    after_space: Vec<bool>,
    /// Whether the byte at `starts[i]` is whitespace, so stepping backward can
    /// run through them too.
    at_space: Vec<bool>,
    len: usize,
}

impl Boundaries {
    /// Builds the queryable form of a segmenter's output, keeping only
    /// ascending offsets at character boundaries and adding the ends.
    pub(crate) fn new(text: &str, mut starts: Vec<usize>) -> Self {
        starts.push(0);
        starts.push(text.len());
        starts.retain(|&at| at <= text.len() && text.is_char_boundary(at));
        starts.sort_unstable();
        starts.dedup();
        let after_space = starts
            .iter()
            .map(|&at| {
                text[..at]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace)
            })
            .collect();
        let at_space = starts
            .iter()
            .map(|&at| text[at..].chars().next().is_some_and(char::is_whitespace))
            .collect();
        Self {
            starts,
            after_space,
            at_space,
            len: text.len(),
        }
    }

    /// The word starts, including `0` and the text's length.
    pub fn starts(&self) -> &[usize] {
        &self.starts
    }

    /// Where the word after `byte` starts: the first start past it that does
    /// not sit in the whitespace following a word, or the end of the text.
    pub fn next(&self, byte: usize) -> Option<usize> {
        let first = self.starts.partition_point(|&at| at <= byte);
        self.starts[first..]
            .iter()
            .zip(&self.after_space[first..])
            .find(|&(&at, &after_space)| at == self.len || !after_space)
            .map(|(&at, _)| at)
    }

    /// Where the word before `byte` starts: the last start before it that is
    /// not itself whitespace, or the start of the text.
    pub fn previous(&self, byte: usize) -> Option<usize> {
        let end = self.starts.partition_point(|&at| at < byte);
        self.starts[..end]
            .iter()
            .zip(&self.at_space[..end])
            .rev()
            .find(|&(&at, &at_space)| at == 0 || !at_space)
            .map(|(&at, _)| at)
    }

    /// The byte range of the word containing `byte`; whitespace between words
    /// is a segment of its own, as it is for the platform's segmenter.
    pub fn containing(&self, byte: usize) -> Range<usize> {
        let last = self.starts.len() - 1;
        let index = match self.starts[..last].partition_point(|&at| at <= byte) {
            0 => 0,
            index => index - 1,
        }
        .min(last.saturating_sub(1));
        self.starts[index]..self.starts[index + 1]
    }
}
