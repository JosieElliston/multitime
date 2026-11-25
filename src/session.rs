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
        Self { doc, tip, caret, undo: vec![], redo: vec![], typing: None }
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
    }
    fn record(&mut self, before: Snapshot) {
        if before.doc != self.doc {
            self.undo.push(before);
            self.redo.clear();
        }
        self.typing = None;
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
    }
    /// Shows a path ending at `tip`.
    pub fn set_tip(&mut self, tip: SegId) {
        self.tip = tip;
        if !self.doc.path(tip).contains(&self.caret.seg) {
            self.caret = self.doc.end(tip);
        }
    }
    /// Makes a segment the tip, with the caret at its end.
    pub fn select(&mut self, seg: SegId) {
        self.tip = seg;
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

    /// Moves the caret by one token along the path. Entering a fork counts.
    pub fn step(&mut self, forward: bool) {
        let path = self.doc.path(self.tip);
        let i = path.iter().position(|s| *s == self.caret.seg).expect("caret on path");
        let tokens = notation::parse(&self.doc.get(self.caret.seg).text).tokens;
        let mut stops = tokens.iter().map(|t| t.span.end);
        let off = if forward {
            stops.find(|s| *s > self.caret.off)
        } else {
            let prev = stops.rfind(|s| *s < self.caret.off);
            prev.or((self.caret.off > 0).then_some(0))
        };
        if let Some(off) = off {
            self.caret.off = off;
        } else if forward && i + 1 < path.len() {
            self.caret = Pos { seg: path[i + 1], off: 0 };
        } else if !forward && i > 0 {
            self.caret = self.doc.end(path[i - 1]);
        }
    }

    pub fn twist(&mut self, mv: &str, inversion: Inversion) {
        let before = self.snapshot();
        let text = &self.doc.get(self.caret.seg).text;
        let (text, off) = notation::twist(text, self.caret.off, mv, inversion);
        self.doc.set_text(self.caret.seg, text);
        self.caret.off = off;
        self.record(before);
    }

    /// Applies an edit made in the path's text box.
    pub fn edit(&mut self, new: &str, caret: usize) {
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
        let before = self.snapshot();
        let seg = self.caret.seg;
        if let Some(start) = self.doc.join(seg) {
            self.caret = Pos { seg: start.seg, off: start.off + self.caret.off };
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
