//! Document plus cursor, undo history, and views. No UI toolkit.

use crate::doc::{Doc, Pos, SegId};
use crate::notation::{self, Inversion, Parsed};
use crate::puzzle::Perm;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InversionMode {
    /// Uses the caret's inversion.
    Attached,
    Detached(Inversion),
}

impl InversionMode {
    pub fn resolve(self, caret: Inversion) -> Inversion {
        match self {
            Self::Attached => caret,
            Self::Detached(i) => i,
        }
    }
}

#[derive(Debug, Clone)]
struct Snapshot {
    doc: Doc,
    tip: SegId,
    caret: Pos,
}

#[derive(Debug, Clone)]
pub struct Session {
    doc: Doc,
    tip: SegId,
    caret: Pos,
    /// Fixed end of the selection. The caret is the other end. On the path.
    anchor: Option<Pos>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Where and when the last typing edit ended, to coalesce typing.
    typing: Option<(Pos, Instant)>,
}

impl Session {
    pub fn new(doc: Doc) -> Self {
        // Start on the first branch.
        let mut tip = doc.root();
        while let Some(c) = doc.get(tip).children.first() {
            tip = *c;
        }
        let caret = doc.end(tip);
        Self { doc, tip, caret, anchor: None, undo: vec![], redo: vec![], typing: None }
    }

    pub fn doc(&self) -> &Doc {
        &self.doc
    }
    pub fn tip(&self) -> SegId {
        self.tip
    }
    pub fn caret(&self) -> Pos {
        self.caret
    }

    /// State and inversion at a position. Derived from scratch.
    pub fn eval(&self, pos: Pos) -> (Perm, Inversion) {
        let mut state = Perm::ID;
        for seg in self.doc.path(pos.seg) {
            let text = &self.doc.get(seg).text;
            if seg == pos.seg {
                let p: Parsed = notation::parse(&text[..pos.off]);
                return (p.op.apply(&state), p.inversion());
            }
            state = notation::parse(text).op.apply(&state);
        }
        unreachable!("path ends at pos.seg")
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { doc: self.doc.clone(), tip: self.tip, caret: self.caret }
    }
    fn restore(&mut self, s: Snapshot) {
        (self.doc, self.tip, self.caret) = (s.doc, s.tip, s.caret);
        self.typing = None;
        self.anchor = None;
    }
    fn record(&mut self, before: Snapshot) {
        if before.doc != self.doc {
            self.undo.push(before);
            self.redo.clear();
        }
        self.typing = None;
        self.anchor = None;
    }

    pub fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(s);
        }
    }
    pub fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(s);
        }
    }

    /// Moves the caret, changing the tip only if it is off the current path.
    pub fn go_to(&mut self, pos: Pos) {
        if !self.doc.path(self.tip).contains(&pos.seg) {
            self.tip = pos.seg;
        }
        self.caret = pos;
        self.anchor = None;
    }

    /// Selects a stretch of a path, leaving the caret at `caret`. One end must be
    /// on the path to the other; otherwise this only moves the caret.
    pub fn set_selection(&mut self, anchor: Pos, caret: Pos) {
        self.go_to(anchor);
        self.go_to(caret);
        let on_path = self.doc.path(self.tip).contains(&anchor.seg);
        self.anchor = (on_path && anchor != caret).then_some(anchor);
    }
    /// Moves the caret, selecting from the old anchor or caret.
    pub fn extend_to(&mut self, pos: Pos) {
        self.set_selection(self.anchor(), pos);
    }
    /// Fixed end of the selection, or the caret if there's no selection.
    pub fn anchor(&self) -> Pos {
        self.anchor.unwrap_or(self.caret)
    }
    /// Shows a path ending at `tip`.
    pub fn set_tip(&mut self, tip: SegId) {
        self.tip = tip;
        self.anchor = None;
        if !self.doc.path(tip).contains(&self.caret.seg) {
            self.caret = self.doc.end(tip);
        }
    }
    /// Makes a segment the tip, with the caret at its end.
    pub fn select(&mut self, seg: SegId) {
        self.tip = seg;
        self.anchor = None;
        self.caret = self.doc.end(seg);
    }

    /// Switches the caret's segment to its next or previous sibling.
    pub fn cycle_sibling(&mut self, delta: isize) {
        let sibs = self.doc.siblings(self.caret.seg);
        let i = sibs.iter().position(|s| *s == self.caret.seg).expect("is own sibling");
        let j = (i as isize + delta).rem_euclid(sibs.len() as isize) as usize;
        if i != j {
            self.select(sibs[j]);
        }
    }

    /// The caret moved to the next gap between tokens on the path. A fork
    /// counts as a token.
    fn stepped(&self, forward: bool) -> Pos {
        let path = self.doc.path(self.tip);
        let seg = self.caret.seg;
        let i = path.iter().position(|s| *s == seg).expect("caret on path");
        let text = &self.doc.get(seg).text;
        let (gap, partway) = notation::gap(text, self.caret.off);
        let at = |gap| Pos { seg, off: notation::gap_offset(text, gap) };
        if forward {
            if gap < notation::parse(text).tokens.len() {
                at(gap + 1)
            } else if let Some(&next) = path.get(i + 1) {
                Pos { seg: next, off: 0 }
            } else {
                self.caret
            }
        } else if partway {
            at(gap)
        } else if gap > 0 {
            at(gap - 1)
        } else if i > 0 {
            let seg = path[i - 1];
            let text = &self.doc.get(seg).text;
            let last = notation::parse(text).tokens.len();
            Pos { seg, off: notation::gap_offset(text, last) }
        } else {
            self.caret
        }
    }
    /// Moves the caret, or just its end of the selection if `extend`.
    pub fn move_caret(&mut self, pos: Pos, extend: bool) {
        if extend {
            self.extend_to(pos);
        } else {
            self.go_to(pos);
        }
    }
    pub fn step(&mut self, forward: bool, extend: bool) {
        self.move_caret(self.stepped(forward), extend);
    }
    /// Moves the caret to the start or end of the path.
    pub fn step_all(&mut self, forward: bool, extend: bool) {
        let pos = if forward {
            self.doc.end(self.tip)
        } else {
            Pos { seg: self.doc.root(), off: 0 }
        };
        self.move_caret(pos, extend);
    }

    /// Deletes the selection, or else one token. A fork counts as a token.
    pub fn delete(&mut self, forward: bool) {
        let (a, c) = (self.offset(self.anchor()), self.offset(self.caret));
        let mut text = self.doc.path_text(self.tip);
        let caret = if a != c {
            text.replace_range(a.min(c)..a.max(c), "");
            a.min(c)
        } else {
            let seg = &self.doc.get(self.caret.seg).text;
            let Some((new, off)) = notation::delete(seg, self.caret.off, forward) else {
                self.delete_fork(forward);
                return;
            };
            let start = c - self.caret.off;
            text.replace_range(start..start + seg.len(), &new);
            start + off
        };
        self.edit_once(&text, caret);
    }
    fn delete_fork(&mut self, forward: bool) {
        let path = self.doc.path(self.tip);
        let i = path.iter().position(|s| *s == self.caret.seg).expect("caret on path");
        if !forward {
            self.join();
        } else if let Some(&next) = path.get(i + 1) {
            self.join_segment(next);
        }
    }

    /// Offset in the path's text.
    fn offset(&self, pos: Pos) -> usize {
        self.doc.path_offset(self.tip, pos).expect("on path")
    }

    pub fn twist(&mut self, mv: &str, inversion: Inversion) {
        let before = self.snapshot();
        let text = &self.doc.get(self.caret.seg).text;
        let (text, off) = notation::twist(text, self.caret.off, mv, inversion);
        self.doc.set_text(self.caret.seg, text);
        self.caret.off = off;
        self.record(before);
    }

    /// Like `edit`, but always its own undo step.
    pub fn edit_once(&mut self, new: &str, caret: usize) {
        self.typing = None;
        self.edit(new, caret);
        self.typing = None;
    }
    /// Applies an edit made in the path's text box.
    pub fn edit(&mut self, new: &str, caret: usize) {
        self.anchor = None;
        let before = self.snapshot();
        let now = Instant::now();
        let coalesce = self.typing.is_some_and(|(pos, t)| {
            pos == self.caret && now - t < Duration::from_secs(1)
        });
        (self.tip, self.caret) = self.doc.edit_path(self.tip, new, caret);
        if before.doc == self.doc {
            return;
        }
        if !coalesce {
            self.record(before);
        }
        self.redo.clear();
        self.typing = Some((self.caret, now));
    }

    pub fn split(&mut self) {
        let before = self.snapshot();
        let old = self.caret.seg;
        let new = self.doc.split(self.caret);
        if self.tip == old {
            self.tip = new;
        }
        self.caret = Pos { seg: new, off: 0 };
        self.record(before);
    }
    pub fn fork(&mut self) {
        let before = self.snapshot();
        self.caret = self.doc.fork(self.caret);
        self.tip = self.caret.seg;
        self.record(before);
    }
    /// Joins the caret's segment to its parent.
    pub fn join(&mut self) {
        self.join_segment(self.caret.seg);
    }
    fn join_segment(&mut self, seg: SegId) {
        let before = self.snapshot();
        if let Some(start) = self.doc.join(seg) {
            if self.caret.seg == seg {
                self.caret = Pos { seg: start.seg, off: start.off + self.caret.off };
            }
            if self.tip == seg {
                self.tip = start.seg;
            }
        }
        self.record(before);
    }
    /// Deletes the caret's segment and everything after it.
    pub fn delete_branch(&mut self) {
        let before = self.snapshot();
        if let Some(parent) = self.doc.delete(self.caret.seg) {
            self.select(parent);
        }
        self.record(before);
    }
    pub fn replace(&mut self, doc: Doc) {
        let before = self.snapshot();
        let new = Self::new(doc);
        (self.doc, self.tip, self.caret) = (new.doc, new.tip, new.caret);
        self.record(before);
    }
}
