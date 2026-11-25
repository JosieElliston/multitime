//! The ground truth: a tree of notation strings. Segments fork only at ends.
//! Never touches a puzzle.

use crate::notation;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SegId(pub u32);

/// A segment has no forks in its text, and doesn't end in a comment if it has
/// children. So a path's text can be split back into its segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub parent: Option<SegId>,
    pub children: Vec<SegId>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Doc {
    segs: BTreeMap<SegId, Segment>,
    root: SegId,
    next: u32,
}

/// Position in the document: a byte offset into a segment's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    pub seg: SegId,
    pub off: usize,
}

/// Splits text on its forks: each `>` that isn't in a comment.
pub fn split_forks(text: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut start = 0;
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let code = &line[..line.find("//").unwrap_or(line.len())];
        for (i, _) in code.match_indices(FORK) {
            out.push(&text[start..line_start + i]);
            start = line_start + i + FORK.len();
        }
        line_start += line.len();
    }
    out.push(&text[start..]);
    out
}

/// How a fork is written between two segments of a path.
pub const FORK: &str = ">";

fn is_fork_line(line: &str) -> bool {
    line.trim() == FORK
}
/// Whether something appended to the text would be commented out.
fn in_comment(text: &str) -> bool {
    text.rsplit('\n').next().is_some_and(|line| line.contains("//"))
}

impl Default for Doc {
    fn default() -> Self {
        Self::new("")
    }
}

impl Doc {
    pub fn new(root_text: &str) -> Self {
        let root = SegId(0);
        let seg = Segment { parent: None, children: vec![], text: String::new() };
        let mut doc = Self { segs: BTreeMap::from([(root, seg)]), root, next: 1 };
        doc.set_split(root, root_text);
        doc
    }

    pub fn root(&self) -> SegId {
        self.root
    }
    pub fn get(&self, id: SegId) -> &Segment {
        &self.segs[&id]
    }
    pub fn ids(&self) -> impl Iterator<Item = SegId> + '_ {
        self.segs.keys().copied()
    }
    fn get_mut(&mut self, id: SegId) -> &mut Segment {
        self.segs.get_mut(&id).expect("segment exists")
    }
    pub fn set_text(&mut self, id: SegId, text: String) {
        self.get_mut(id).text = text;
    }
    /// Sets a segment's text, splitting it on forks. Returns the last piece,
    /// which gets its children.
    fn set_split(&mut self, id: SegId, text: &str) -> SegId {
        let mut pieces = split_forks(text).into_iter();
        let mut cur = id;
        let mut piece = pieces.next().expect("nonempty");
        for next in pieces {
            cur = self.split_into(cur, piece.to_owned(), String::new());
            piece = next;
        }
        self.set_text(cur, piece.to_owned());
        self.end_comment(cur);
        cur
    }
    /// Ends the comment that a fork would otherwise be in.
    fn end_comment(&mut self, id: SegId) {
        let seg = self.get_mut(id);
        if !seg.children.is_empty() && in_comment(&seg.text) {
            seg.text.push('\n');
        }
    }
    pub fn end(&self, seg: SegId) -> Pos {
        Pos { seg, off: self.get(seg).text.len() }
    }
    /// The segment and its siblings, in order.
    pub fn siblings(&self, id: SegId) -> &[SegId] {
        match self.get(id).parent {
            Some(p) => &self.get(p).children,
            None => std::slice::from_ref(&self.root),
        }
    }

    /// Segments from the root to `tip`.
    pub fn path(&self, tip: SegId) -> Vec<SegId> {
        let mut out = vec![tip];
        while let Some(p) = self.get(*out.last().expect("nonempty")).parent {
            out.push(p);
        }
        out.reverse();
        out
    }

    pub fn path_text(&self, tip: SegId) -> String {
        let texts: Vec<&str> = self.path(tip).iter().map(|s| self.get(*s).text.as_str()).collect();
        texts.join(FORK)
    }

    /// Byte offset in `path_text` of the start of each segment of the path.
    pub fn path_starts(&self, tip: SegId) -> Vec<usize> {
        let mut at = 0;
        self.path(tip)
            .iter()
            .map(|s| {
                let start = at;
                at += self.get(*s).text.len() + FORK.len();
                start
            })
            .collect()
    }

    pub fn path_offset(&self, tip: SegId, pos: Pos) -> Option<usize> {
        let i = self.path(tip).iter().position(|s| *s == pos.seg)?;
        Some(self.path_starts(tip)[i] + pos.off)
    }

    pub fn path_pos(&self, tip: SegId, offset: usize) -> Pos {
        let path = self.path(tip);
        let starts = self.path_starts(tip);
        for (i, &seg) in path.iter().enumerate() {
            let end = starts[i] + self.get(seg).text.len();
            if offset <= end || i + 1 == path.len() {
                return Pos { seg, off: offset.clamp(starts[i], end) - starts[i] };
            }
        }
        unreachable!("path is nonempty")
    }

    pub fn add_child(&mut self, parent: SegId, text: String) -> SegId {
        let id = SegId(self.next);
        self.next += 1;
        self.segs.insert(id, Segment { parent: Some(parent), children: vec![], text });
        self.get_mut(parent).children.push(id);
        self.end_comment(parent);
        id
    }

    /// Replaces a segment's text with `left` and moves its children to a new
    /// only child containing `right`.
    fn split_into(&mut self, id: SegId, left: String, right: String) -> SegId {
        let children = std::mem::take(&mut self.get_mut(id).children);
        self.set_text(id, left);
        let new = self.add_child(id, right);
        for c in &children {
            self.get_mut(*c).parent = Some(new);
        }
        self.get_mut(new).children = children;
        new
    }

    /// Returns the segment after the split, and the one that now has the
    /// children. They differ if cutting a `//` uncomments a fork.
    pub fn split(&mut self, pos: Pos) -> (SegId, SegId) {
        let c = notation::cut(&self.get(pos.seg).text, pos.off);
        let new = self.split_into(pos.seg, c.left, String::new());
        (new, self.set_split(new, &c.right))
    }

    /// Adds an empty branch that continues from `pos`. Right after or before a
    /// fork on the same line, it's a branch of that fork.
    pub fn fork(&mut self, pos: Pos) -> Pos {
        let c = notation::cut(&self.get(pos.seg).text, pos.off);
        let blank = |s: &str| s.trim().is_empty() && !s.contains('\n');
        let at = if let Some(parent) = self.get(pos.seg).parent
            && blank(&c.left)
        {
            parent
        } else if blank(&c.right) {
            pos.seg
        } else {
            let new = self.split_into(pos.seg, c.left, String::new());
            self.set_split(new, &c.right);
            pos.seg
        };
        let (text, off) = c.fork;
        Pos { seg: self.add_child(at, text), off }
    }

    /// Merges a segment into its parent. Its children take its place among its
    /// siblings, which now come after its text. Returns where that text starts.
    pub fn join(&mut self, id: SegId) -> Option<Pos> {
        let parent = self.get(id).parent?;
        let seg = self.segs.remove(&id)?;
        for c in &seg.children {
            self.get_mut(*c).parent = Some(parent);
        }
        let p = self.get_mut(parent);
        let i = p.children.iter().position(|c| *c == id).expect("is parent's child");
        p.children.splice(i..=i, seg.children);
        let off = p.text.len();
        p.text.push_str(&seg.text);
        self.end_comment(parent);
        Some(Pos { seg: parent, off })
    }

    /// Deletes a segment and its descendants. Returns its parent.
    pub fn delete(&mut self, id: SegId) -> Option<SegId> {
        let parent = self.get(id).parent?;
        self.get_mut(parent).children.retain(|c| *c != id);
        let mut todo = vec![id];
        while let Some(id) = todo.pop() {
            if let Some(seg) = self.segs.remove(&id) {
                todo.extend(seg.children);
            }
        }
        Some(parent)
    }

    /// Applies a text editor's change to the path ending at `tip`, where
    /// `caret` is the editor's caret afterward. Returns the new tip and caret.
    ///
    /// The path's text becomes exactly `new`, so forks that are gone from it get
    /// joined, and new ones split. (Except that `end_comment` may append to it.)
    pub fn edit_path(&mut self, tip: SegId, new: &str, caret: usize) -> (SegId, Pos) {
        let caret = new[..caret].replace('\r', "").len();
        let new = &new.replace('\r', "");
        let old = self.path_text(tip);
        let path = self.path(tip);
        let starts = self.path_starts(tip);

        // The edit ends at the caret, which disambiguates the diff.
        let (old_b, new_b) = (old.as_bytes(), new.as_bytes());
        let tail = &new_b[caret..];
        let suffix = if old_b.ends_with(tail) {
            tail.len()
        } else {
            let mut n = old.bytes().rev().zip(new.bytes().rev()).take_while(|(a, b)| a == b).count();
            while !old.is_char_boundary(old.len() - n) || !new.is_char_boundary(new.len() - n) {
                n -= 1;
            }
            n
        };
        let max = old.len().min(new.len()) - suffix;
        let mut p = old_b[..max].iter().zip(&new_b[..max]).take_while(|(a, b)| a == b).count();
        while !old.is_char_boundary(p) || !new.is_char_boundary(p) {
            p -= 1;
        }
        let e = old.len() - suffix;

        // Rewrite the segments that it touches, and any that it comments out.
        let index = |off| {
            let seg = self.path_pos(tip, off).seg;
            path.iter().position(|s| *s == seg).expect("on path")
        };
        let (h, mut t) = (index(p), index(e));
        let mut text = format!(
            "{}{}{}",
            &old[starts[h]..p],
            &new[p..new.len() - suffix],
            &old[e..starts[t] + self.get(path[t]).text.len()],
        );
        while in_comment(&text) && t + 1 < path.len() {
            t += 1;
            text.push_str(FORK);
            text.push_str(&self.get(path[t]).text);
        }
        for &s in &path[h + 1..=t] {
            self.set_text(s, String::new());
            self.join(s);
        }
        let last = self.set_split(path[h], &text);
        let tip = if t + 1 == path.len() { last } else { tip };
        (tip, self.path_pos(tip, caret))
    }

    /// Serializes to plain text, with each branch indented under a line with its
    /// fork. The line breaks around that line aren't part of the segments, except
    /// for one that ends a comment.
    pub fn to_file(&self) -> String {
        let mut out = String::new();
        self.write(self.root, 0, &mut out);
        out
    }
    fn write(&self, id: SegId, depth: usize, out: &mut String) {
        let seg = self.get(id);
        let ended = seg.text.strip_suffix('\n');
        let text = ended.filter(|s| !seg.children.is_empty() && in_comment(s)).unwrap_or(&seg.text);
        for line in text.split('\n') {
            if !line.is_empty() {
                out.extend(std::iter::repeat_n("  ", depth));
                out.push_str(line);
            }
            out.push('\n');
        }
        for &c in &seg.children {
            out.extend(std::iter::repeat_n("  ", depth));
            out.push_str(">\n");
            self.write(c, depth + 1, out);
        }
    }

    pub fn from_file(s: &str) -> Result<Self, String> {
        let s = s.replace('\r', "");
        let mut doc = Self::default();
        let mut stack = vec![doc.root];
        let mut lines: Vec<&str> = vec![];
        for (n, line) in s.strip_suffix('\n').unwrap_or(&s).split('\n').enumerate() {
            if is_fork_line(line) {
                let indent = line.len() - line.trim_start().len();
                let depth = indent / 2;
                if indent % 2 != 0 || depth >= stack.len() {
                    return Err(format!("line {}: bad indentation before `>`", n + 1));
                }
                let last = stack.last_mut().expect("nonempty");
                *last = doc.set_split(*last, &lines.join("\n"));
                lines.clear();
                stack.truncate(depth + 1);
                stack.push(doc.add_child(stack[depth], String::new()));
            } else {
                let max = 2 * (stack.len() - 1);
                let strip = line.bytes().take(max).take_while(|b| *b == b' ').count();
                lines.push(&line[strip..]);
            }
        }
        doc.set_split(*stack.last().expect("nonempty"), &lines.join("\n"));
        Ok(doc)
    }

    /// Checks tree invariants. For tests.
    pub fn check(&self) {
        for (id, seg) in &self.segs {
            assert_eq!(seg.parent.is_none(), *id == self.root);
            assert_eq!(split_forks(&seg.text).len(), 1, "fork in {:?}", seg.text);
            assert!(seg.children.is_empty() || !in_comment(&seg.text), "{:?}", seg.text);
            for c in &seg.children {
                assert_eq!(self.get(*c).parent, Some(*id));
            }
            if let Some(p) = seg.parent {
                assert_eq!(self.get(p).children.iter().filter(|c| *c == id).count(), 1);
            }
        }
    }
}
