//! The ground truth: a tree of notation strings. Segments fork only at ends.
//! Never touches a puzzle.

use crate::notation;
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

/// How a fork is written between two segments of a path.
pub const FORK: &str = "\n>\n";

fn is_fork_line(line: &str) -> bool {
    line.trim() == ">"
}

/// Drops fork lines.
pub fn without_forks(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').filter(|l| !is_fork_line(l)).collect();
    lines.join("\n")
}

/// Splits text on fork lines.
fn split_forks(text: &str) -> Vec<String> {
    let mut out: Vec<Vec<&str>> = vec![vec![]];
    for line in text.split('\n') {
        if is_fork_line(line) {
            out.push(vec![]);
        } else {
            out.last_mut().expect("nonempty").push(line);
        }
    }
    out.into_iter().map(|lines| lines.join("\n")).collect()
}

/// Finds which piece of `split_forks(text)` an offset lands in, and where.
fn caret_piece(text: &str, pieces: &[String], off: usize) -> (usize, usize) {
    let line_start = text[..off].rfind('\n').map_or(0, |n| n + 1);
    let line_end = text[off..].find('\n').map_or(text.len(), |n| off + n);
    let on_fork = is_fork_line(&text[line_start..line_end]);
    let before = split_forks(&text[..line_start]);
    let mut i = before.len() - 1;
    if on_fork {
        // A caret on a fork line goes to the start of the next segment.
        return (i + 1, 0);
    }
    // `before` ends with the empty start of the caret's line.
    let piece_start = before[i].len();
    i = i.min(pieces.len() - 1);
    (i, (piece_start + off - line_start).min(pieces[i].len()))
}

impl Default for Doc {
    fn default() -> Self {
        Self::new("")
    }
}

impl Doc {
    pub fn new(root_text: &str) -> Self {
        let root = SegId(0);
        let seg = Segment { parent: None, children: vec![], text: root_text.to_owned() };
        Self { segs: BTreeMap::from([(root, seg)]), root, next: 1 }
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
            // The second half of the fork belongs to the next segment.
            if offset < end + 2 || i + 1 == path.len() {
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

    pub fn split(&mut self, pos: Pos) -> SegId {
        let c = notation::cut(&self.get(pos.seg).text, pos.off);
        self.split_into(pos.seg, c.left, c.right)
    }

    /// Adds an empty branch that continues from `pos`.
    pub fn fork(&mut self, pos: Pos) -> Pos {
        let c = notation::cut(&self.get(pos.seg).text, pos.off);
        let parent = self.get(pos.seg).parent;
        let at = if c.right.trim().is_empty() {
            pos.seg
        } else if let Some(parent) = parent
            && c.left.trim().is_empty()
        {
            parent
        } else {
            self.split_into(pos.seg, c.left, c.right);
            pos.seg
        };
        let (text, off) = c.fork;
        Pos { seg: self.add_child(at, text), off }
    }

    pub fn can_join(&self, id: SegId) -> bool {
        self.get(id).parent.is_some() && self.siblings(id).len() == 1
    }

    /// Merges a segment without siblings into its parent. Returns where the
    /// segment's text now starts.
    pub fn join(&mut self, id: SegId) -> Option<Pos> {
        if !self.can_join(id) {
            return None;
        }
        let parent = self.get(id).parent?;
        let seg = self.segs.remove(&id)?;
        for c in &seg.children {
            self.get_mut(*c).parent = Some(parent);
        }
        let p = self.get_mut(parent);
        p.children = seg.children;
        if !p.text.is_empty() && !seg.text.is_empty() {
            p.text.push('\n');
        }
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

    /// Applies a text editor's change to the path ending at `tip`, where
    /// `caret` is the editor's caret afterward. Returns the new tip and caret.
    ///
    /// Forks that were touched get joined, unless they have siblings. Fork
    /// lines that were written split the segment.
    pub fn edit_path(&mut self, tip: SegId, new: &str, caret: usize) -> (SegId, Pos) {
        let old = self.path_text(tip);
        let path = self.path(tip);
        let starts = self.path_starts(tip);
        let end_of = |i: usize, doc: &Self| starts[i] + doc.get(path[i]).text.len();

        // The edit ends at the caret, which disambiguates the diff.
        let (old_b, new_b) = (old.as_bytes(), new.as_bytes());
        let tail = &new_b[caret.min(new.len())..];
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
        let ins = new[p..new.len() - suffix].replace('\r', "");

        // Range of segments affected, and what is kept at each end.
        let (h, t, p, e) = if p == e {
            let pos = self.path_pos(tip, p);
            let i = path.iter().position(|s| *s == pos.seg).expect("on path");
            let at = starts[i] + pos.off;
            (i, i, at, at)
        } else {
            let h = (0..path.len()).rev().find(|&i| starts[i] <= p).expect("starts at 0");
            let t = (h..path.len()).find(|&i| e <= end_of(i, self)).expect("ends at len");
            (h, t, p, e)
        };
        let (h_end, t_start) = (end_of(h, self), starts[t]);
        let mut head = self.get(path[h]).text[..p.min(h_end) - starts[h]].to_owned();
        let tail = self.get(path[t]).text[e.max(t_start) - t_start..].to_owned();
        let within_fork = p >= h_end && e <= t_start;
        head.push_str(&ins);
        let caret_off = head.len();

        // Empty out the interior, then join what can be joined.
        for &s in &path[h..=t] {
            self.set_text(s, String::new());
        }
        let mut survivor = path[h];
        let mut first_group_is_all = true;
        for &s in &path[h + 1..=t] {
            if self.can_join(s) {
                self.join(s);
            } else {
                survivor = s;
                first_group_is_all = false;
            }
        }
        if first_group_is_all {
            let nl = h != t && within_fork && !head.is_empty() && !tail.is_empty();
            let nl = if nl { "\n" } else { "" };
            self.set_text(survivor, format!("{head}{nl}{tail}"));
        } else {
            self.set_text(path[h], head);
            self.set_text(survivor, tail);
        }
        let mut tip = if self.segs.contains_key(&tip) { tip } else { survivor };
        let mut caret = Pos { seg: path[h], off: caret_off };

        // Later segments first, so that `path[h]`'s caret is handled last.
        let mut affected = vec![survivor, path[h]];
        affected.dedup();
        for seg in affected {
            let text = self.get(seg).text.clone();
            let pieces = split_forks(&text);
            if pieces.len() == 1 {
                continue;
            }
            let new_caret = (caret.seg == seg).then(|| caret_piece(&text, &pieces, caret.off));
            let mut cur = seg;
            let mut ids = vec![seg];
            let mut pieces = pieces.into_iter();
            let mut piece = pieces.next().expect("nonempty");
            for next in pieces {
                cur = self.split_into(cur, piece, String::new());
                ids.push(cur);
                piece = next;
            }
            self.set_text(cur, piece);
            if tip == seg {
                tip = cur;
            }
            if let Some((i, off)) = new_caret {
                caret = Pos { seg: ids[i], off };
            }
        }
        (tip, caret)
    }

    /// Serializes to plain text: the same as `path_text`, but with every branch
    /// and with children indented.
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
        Ok(doc)
    }

    /// Checks tree invariants. For tests.
    pub fn check(&self) {
        for (id, seg) in &self.segs {
            assert_eq!(seg.parent.is_none(), *id == self.root);
            assert!(!seg.text.split('\n').any(is_fork_line), "fork line in {:?}", seg.text);
            for c in &seg.children {
                assert_eq!(self.get(*c).parent, Some(*id));
            }
            if let Some(p) = seg.parent {
                assert_eq!(self.get(p).children.iter().filter(|c| *c == id).count(), 1);
            }
        }
    }
}
