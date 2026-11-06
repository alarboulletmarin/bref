//! The one walk over the lines of a note that knows where the code blocks are.

use super::is_fence;

/// A line of a note, and where it stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line<'a> {
    /// Byte where the line starts.
    pub at: usize,
    /// Byte after the line, its `\n` included.
    pub end: usize,
    /// The line without its `\n`.
    pub text: &'a str,
    /// A ``` line, the one that opens a block, the one that closes it or a stray one.
    pub fence: bool,
    /// Inside a code block, fences excluded.
    pub code: bool,
}

impl Line<'_> {
    /// Neither a fence nor inside a block: the lines whose links, tags and headings count.
    pub fn outside(&self) -> bool {
        !self.fence && !self.code
    }
}

/// Every line of `text` with its place. One rule for the whole app, the one the editor draws: a ``` opens
/// a block only if another ``` follows, so a stray fence opens nothing and what follows stays text.
pub fn lines(text: &str) -> impl Iterator<Item = Line<'_>> {
    let mut left = text.lines().filter(|l| is_fence(l)).count();
    let (mut at, mut code) = (0, false);
    text.split_inclusive('\n').map(move |whole| {
        let (text, line_at) = (whole.strip_suffix('\n').unwrap_or(whole), at);
        at += whole.len();
        let fence = is_fence(text);
        if fence {
            left -= 1;
            code = !code && left > 0;
        }
        Line { at: line_at, end: at, text, fence, code: code && !fence }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outside(text: &str) -> Vec<&str> {
        lines(text).filter(|l| l.outside()).map(|l| l.text).collect()
    }

    #[test]
    fn walks_the_lines_outside_code() {
        assert_eq!(outside("a\n```\nb\n```\nc"), ["a", "c"]);
        assert_eq!(outside("a\n```\nb\n```\nc\n"), ["a", "c"]);
        assert_eq!(outside(""), Vec::<&str>::new());
        assert_eq!(outside("```\n```"), Vec::<&str>::new());
        assert_eq!(outside("a\n\n```\n\n```\n\nb"), ["a", "", "", "b"]);
        let l: Vec<_> = lines("é\n```\nx").collect();
        assert_eq!((l[1].at, l[1].end, l[2].at, l[2].end), (3, 7, 7, 8));
    }

    /// A stray fence opens nothing, as on screen: what follows it is text for links, headings and embeds.
    #[test]
    fn an_unclosed_fence_opens_no_block() {
        assert_eq!(outside("a\n```\nb"), ["a", "b"]);
        assert_eq!(outside("```\nb\n```\n```\nc"), ["c"]);
        assert_eq!(outside("```\na\n```\n```\nb\n```\n```\nc"), ["c"]);
        assert!(lines("a\n```\nb").all(|l| !l.code));
    }
}
