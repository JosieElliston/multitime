//! The ground truth: a tree of notation strings. Segments fork only at ends.
//! Never touches a puzzle.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SegId(pub u32);

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

/// Where the first `>` that isn't in a comment is. In a segment's text, that's
/// always written as a fork instead.
fn find_fork(text: &str) -> Option<usize> {
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let code = &line[..line.find("//").unwrap_or(line.len())];
        if let Some(i) = code.find('>') {
            return Some(line_start + i);
        }
        line_start += line.len();
    }
    None
}
fn is_fork_line(line: &str) -> bool {
    line.trim() == ">"
}

impl Default for Doc {
    fn default() -> Self {
        Self::new("")
    }
}

impl Doc {
    pub fn new(root_text: &str) -> Self {
        let root = SegId(0);
        let seg = Segment {
            parent: None,
            children: vec![],
            text: String::new(),
        };
        let mut doc = Self {
            segs: BTreeMap::from([(root, seg)]),
            root,
            next: 1,
        };
        doc.set_text(root, root_text.to_owned());
        doc.normalize();
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
    /// Splits segments on any `>` in their text. Returns each segment that was
    /// split, the offset of the `>`, and the new segment.
    pub fn normalize(&mut self) -> Vec<(SegId, usize, SegId)> {
        let mut out = vec![];
        let mut todo: Vec<SegId> = self.ids().collect();
        while let Some(id) = todo.pop() {
            let text = &self.get(id).text;
            if let Some(at) = find_fork(text) {
                let (left, right) = (text[..at].to_owned(), text[at + 1..].to_owned());
                let new = self.split_into(id, left, right);
                out.push((id, at, new));
                todo.push(new);
            }
        }
        out
    }
    pub fn end(&self, seg: SegId) -> Pos {
        Pos {
            seg,
            off: self.get(seg).text.len(),
        }
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

    pub fn add_child(&mut self, parent: SegId, text: String) -> SegId {
        let id = SegId(self.next);
        self.next += 1;
        self.segs.insert(
            id,
            Segment {
                parent: Some(parent),
                children: vec![],
                text,
            },
        );
        self.get_mut(parent).children.push(id);
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

    /// Splits a segment into what's before and after a span, which is dropped.
    pub fn split_around(&mut self, seg: SegId, span: std::ops::Range<usize>) -> SegId {
        let text = &self.get(seg).text;
        let (left, right) = (text[..span.start].to_owned(), text[span.end..].to_owned());
        self.split_into(seg, left, right)
    }
    /// Splits a segment exactly at a position.
    pub fn split_at(&mut self, pos: Pos) -> SegId {
        let (left, right) = self.get(pos.seg).text.split_at(pos.off);
        let (left, right) = (left.to_owned(), right.to_owned());
        self.split_into(pos.seg, left, right)
    }
    /// Adds an empty sibling right after a segment other than the root.
    pub fn add_sibling(&mut self, id: SegId) -> Option<SegId> {
        let parent = self.get(id).parent?;
        let i = self
            .get(parent)
            .children
            .iter()
            .position(|c| *c == id)
            .expect("is parent's child");
        Some(self.insert_child(parent, i + 1))
    }
    /// Adds an empty child at an index among the children.
    pub fn insert_child(&mut self, parent: SegId, index: usize) -> SegId {
        let new = self.add_child(parent, String::new());
        let children = &mut self.get_mut(parent).children;
        children.pop();
        children.insert(index, new);
        new
    }

    /// Merges a segment into its parent, deleting its siblings. Returns where that text starts.
    pub fn join(&mut self, id: SegId) -> Option<Pos> {
        let parent = self.get(id).parent?;
        for s in self.siblings(id).to_vec() {
            if s != id {
                self.delete(s);
            }
        }
        let seg = self.segs.remove(&id)?;
        for c in &seg.children {
            self.get_mut(*c).parent = Some(parent);
        }
        let p = self.get_mut(parent);
        p.children = seg.children;
        let off = p.text.len();
        p.text.push_str(&seg.text);
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

    /// Serializes to plain text, with each branch indented under a line with its
    /// fork. The line breaks around that line aren't part of the segments.
    pub fn to_file(&self) -> String {
        let mut out = String::new();
        self.write(self.root, 0, &mut out);
        out
    }
    fn write(&self, id: SegId, depth: usize, out: &mut String) {
        let seg = self.get(id);
        for line in seg.text.split('\n') {
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
                doc.set_text(*stack.last().expect("nonempty"), lines.join("\n"));
                lines.clear();
                stack.truncate(depth + 1);
                stack.push(doc.add_child(stack[depth], String::new()));
            } else {
                let max = 2 * (stack.len() - 1);
                let strip = line.bytes().take(max).take_while(|b| *b == b' ').count();
                lines.push(&line[strip..]);
            }
        }
        doc.set_text(*stack.last().expect("nonempty"), lines.join("\n"));
        doc.normalize();
        Ok(doc)
    }

    /// Checks tree invariants. For tests.
    pub fn check(&self) {
        for (id, seg) in &self.segs {
            assert_eq!(seg.parent.is_none(), *id == self.root);
            assert_eq!(find_fork(&seg.text), None, "fork in {:?}", seg.text);
            for c in &seg.children {
                assert_eq!(self.get(*c).parent, Some(*id));
            }
            if let Some(p) = seg.parent {
                assert_eq!(self.get(p).children.iter().filter(|c| *c == id).count(), 1);
            }
        }
    }
}
