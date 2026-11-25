//! Document plus cursor, undo history, and views. No UI toolkit.

use crate::doc::{Doc, Pos, SegId};
use crate::notation::{self, Inversion, Parsed, Unit};
use crate::puzzle::Perm;
use std::collections::BTreeMap;
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

/// The text box's text is the segments of the path, with a fork between two of
/// them written as a bubble: a `MARK` for each branch, then an `END`. The caret
/// is inside of a bubble when it's right after a `MARK`, whose branch is then
/// the active one.
pub const MARK: char = '\x01';
pub const END: char = '\x02';

/// The text box's text as it's copied. Only the active branch of a bubble is.
pub fn to_clipboard(text: &str) -> String {
    let mut out = String::new();
    let mut after_bubble = false;
    for c in text.chars() {
        match c {
            MARK => {}
            END => {
                // So that pasting makes a separate bubble.
                if out.rsplit('\n').next().is_some_and(|line| line.contains("//")) {
                    out.push('\n');
                } else if after_bubble {
                    out.push(' ');
                }
                out.push('>');
            }
            c => out.push(c),
        }
        if c != MARK {
            after_bubble = c == END;
        }
    }
    out
}

/// A stretch of the text box's text. A bubble's is the one before its segment.
struct Span {
    seg: SegId,
    bubble: usize,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone)]
struct Snapshot {
    doc: Doc,
    tip: SegId,
    caret: Pos,
    inside: bool,
}

#[derive(Debug, Clone)]
pub struct Session {
    doc: Doc,
    /// Leaf that the path goes to.
    tip: SegId,
    /// Which child each segment's path last went to. Can be stale.
    active: BTreeMap<SegId, SegId>,
    caret: Pos,
    /// The caret is inside of the bubble before it, rather than after it.
    inside: bool,
    /// Fixed end of the selection. The caret is the other end. On the path.
    anchor: Option<Pos>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Where and when the last typing edit ended, to coalesce typing.
    typing: Option<(Pos, Instant)>,
}

impl Session {
    pub fn new(doc: Doc) -> Self {
        let root = doc.root();
        let mut s = Self {
            caret: doc.end(root),
            doc,
            tip: root,
            active: BTreeMap::new(),
            inside: false,
            anchor: None,
            undo: vec![],
            redo: vec![],
            typing: None,
        };
        s.show(root);
        s.caret = s.doc.end(s.tip);
        s
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
    pub fn inside(&self) -> bool {
        self.inside
    }

    /// Makes the path go through a segment, and then through active branches.
    fn show(&mut self, seg: SegId) {
        self.tip = seg;
        loop {
            let children = &self.doc.get(self.tip).children;
            let active = self.active.get(&self.tip).filter(|c| children.contains(c));
            let Some(&next) = active.or(children.first()) else {
                break;
            };
            self.tip = next;
        }
        for pair in self.doc.path(self.tip).windows(2) {
            self.active.insert(pair[0], pair[1]);
        }
    }
    /// Puts the caret inside of a bubble, right after the branch's mark.
    pub fn enter(&mut self, seg: SegId) {
        self.go_to(Pos { seg, off: 0 });
        self.inside = self.doc.get(seg).parent.is_some();
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
        Snapshot { doc: self.doc.clone(), tip: self.tip, caret: self.caret, inside: self.inside }
    }
    fn restore(&mut self, s: Snapshot) {
        (self.doc, self.tip, self.caret, self.inside) = (s.doc, s.tip, s.caret, s.inside);
        self.show(self.tip);
        self.typing = None;
        self.anchor = None;
    }
    /// Splits on any `>` that edits uncommented, and extends the path to a leaf.
    fn normalize(&mut self) {
        for (seg, off, new) in self.doc.normalize() {
            if self.tip == seg {
                self.tip = new;
            }
            if self.caret.seg == seg && self.caret.off > off {
                self.caret = Pos { seg: new, off: self.caret.off - off - 1 };
            }
        }
        self.show(self.tip);
    }
    fn record(&mut self, before: Snapshot) {
        self.normalize();
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

    /// Moves the caret, changing the path only if it is off the current one.
    pub fn go_to(&mut self, pos: Pos) {
        if !self.doc.path(self.tip).contains(&pos.seg) {
            self.show(pos.seg);
        }
        self.caret = pos;
        self.inside = false;
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
    /// Ends of the unit that a click on the character at `pos` selects. Off a
    /// segment's end, that's the bubble after it.
    pub fn unit_at(&self, pos: Pos, unit: Unit) -> Option<(Pos, Pos)> {
        let text = &self.doc.get(pos.seg).text;
        let r = notation::unit_at(text, pos.off, unit)?;
        Some((Pos { seg: pos.seg, off: r.start }, Pos { seg: pos.seg, off: r.end }))
    }
    /// The same for the character at an offset in the text box. A bubble's
    /// unit is the bubble, keeping its active branch.
    pub fn unit_at_offset(&self, off: usize, unit: Unit) -> Option<(Pos, Pos)> {
        if let Some((_, seg)) = self.bubbles().into_iter().find(|(r, _)| r.contains(&off)) {
            let parent = self.doc.get(seg).parent.expect("bubbles have parents");
            return Some((self.doc.end(parent), Pos { seg, off: 0 }));
        }
        self.unit_at(self.at_offset(off).0, unit)
    }
    /// Selects from the start of one unit to the end of another, and any group
    /// whose bracket that covers. The caret goes at `target`'s far end. Does
    /// nothing if no path has both.
    pub fn select_units(&mut self, anchor: (Pos, Pos), target: (Pos, Pos)) {
        let (a, t) = (anchor.1.seg, target.1.seg);
        let deeper = if self.doc.path(t).contains(&a) {
            t
        } else if self.doc.path(a).contains(&t) {
            a
        } else {
            return;
        };
        if !self.doc.path(self.tip).contains(&deeper) {
            self.show(deeper);
        }
        let path = self.doc.path(self.tip);
        let key = |p: &Pos| (path.iter().position(|s| *s == p.seg).expect("on path"), p.off);
        let mut start = std::cmp::min_by_key(anchor.0, target.0, key);
        let mut end = std::cmp::max_by_key(anchor.1, target.1, key);
        let text = |seg| &self.doc.get(seg).text;
        if start.seg == end.seg {
            let r = notation::close_groups(text(start.seg), start.off..end.off);
            (start.off, end.off) = (r.start, r.end);
        } else {
            start.off =
                notation::close_groups(text(start.seg), start.off..text(start.seg).len()).start;
            end.off = notation::close_groups(text(end.seg), 0..end.off).end;
        }
        if key(&target.0) < key(&anchor.0) {
            self.set_selection(end, start);
        } else {
            self.set_selection(start, end);
        }
    }
    /// Moves the caret to the end of a segment.
    pub fn select(&mut self, seg: SegId) {
        self.go_to(self.doc.end(seg));
    }

    /// Switches branches at the bubble before the caret, which goes to the start
    /// of the new branch.
    pub fn cycle_sibling(&mut self, delta: isize) {
        let seg = self.caret.seg;
        let sibs = self.doc.siblings(seg);
        let i = sibs.iter().position(|s| *s == seg).expect("is own sibling");
        let j = (i as isize + delta).rem_euclid(sibs.len() as isize) as usize;
        if i != j {
            let inside = self.inside;
            self.go_to(Pos { seg: sibs[j], off: 0 });
            self.inside = inside;
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
        if self.inside && forward {
            return self.caret;
        }
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
    /// Moves the caret like Option+arrows in text: to the start of the token
    /// before it, or the end of the one after it. A fork counts as a token, and
    /// passing it keeps the branch.
    pub fn step_word(&mut self, forward: bool, extend: bool) {
        if self.inside {
            self.step_bubble(forward);
            return;
        }
        let path = self.doc.path(self.tip);
        let Pos { seg, off } = self.caret;
        let i = path.iter().position(|s| *s == seg).expect("caret on path");
        let tokens = notation::parse(&self.doc.get(seg).text).tokens;
        let pos = if forward {
            match tokens.iter().find(|t| t.span.end > off) {
                Some(t) => Pos { seg, off: t.span.end },
                None => path.get(i + 1).map_or(self.caret, |&next| Pos { seg: next, off: 0 }),
            }
        } else {
            match tokens.iter().rev().find(|t| t.span.start < off) {
                Some(t) => Pos { seg, off: t.span.start },
                None if i > 0 => self.doc.end(path[i - 1]),
                None => Pos { seg, off: 0 },
            }
        };
        self.move_caret(pos, extend);
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

    /// Moves the caret between the sides of a bubble and its active branch.
    /// Returns whether there's a bubble there.
    pub fn step_bubble(&mut self, forward: bool) -> bool {
        let Pos { seg, off } = self.caret;
        let path = self.doc.path(self.tip);
        let i = path.iter().position(|s| *s == seg).expect("caret on path");
        if self.inside {
            let pos = if forward { self.caret } else { self.doc.end(path[i - 1]) };
            self.go_to(pos);
        } else if !forward && off == 0 && i > 0 {
            self.enter(seg);
        } else if let Some(&next) = path.get(i + 1)
            && forward
            && self.caret == self.doc.end(seg)
        {
            self.enter(next);
        } else {
            return false;
        }
        true
    }

    /// Deletes the selection, or else one token. A bubble counts as a token.
    pub fn delete(&mut self, forward: bool) {
        let before = self.snapshot();
        let (a, c) = (self.offset(self.anchor()), self.offset(self.caret));
        let Pos { seg, off } = self.caret;
        if a != c {
            self.remove(a.min(c), a.max(c));
        } else if self.inside {
            if !forward {
                let c = self.caret_offset();
                self.remove(c - 1, c);
            }
        } else if let Some((new, off)) = notation::delete(&self.doc.get(seg).text, off, forward) {
            self.doc.set_text(seg, new);
            self.caret.off = off;
        } else {
            let path = self.doc.path(self.tip);
            let i = path.iter().position(|s| *s == seg).expect("caret on path");
            let bubble = if forward { path.get(i + 1).copied() } else { Some(seg) };
            if let Some(seg) = bubble
            {
                self.join_into_parent(seg);
            }
        }
        self.record(before);
    }

    // ---- the text box's text ----

    fn spans(&self) -> Vec<Span> {
        let mut at = 0;
        let path = self.doc.path(self.tip);
        let span = |&seg: &SegId| {
            let s = self.doc.get(seg);
            let bubble = at;
            if s.parent.is_some() {
                at += self.doc.siblings(seg).len() + 1;
            }
            let start = at;
            at += s.text.len();
            Span { seg, bubble, start, end: at }
        };
        path.iter().map(span).collect()
    }
    pub fn text(&self) -> String {
        let mut out = String::new();
        for s in self.spans() {
            if s.start > s.bubble {
                out.extend(std::iter::repeat_n(MARK, s.start - s.bubble - 1));
                out.push(END);
            }
            out.push_str(&self.doc.get(s.seg).text);
        }
        out
    }
    /// Offset in the text, if it's on the path.
    pub fn offset_of(&self, pos: Pos) -> Option<usize> {
        self.spans().iter().find(|s| s.seg == pos.seg).map(|s| s.start + pos.off)
    }
    fn offset(&self, pos: Pos) -> usize {
        self.offset_of(pos).expect("on path")
    }
    pub fn caret_offset(&self) -> usize {
        if !self.inside {
            return self.offset(self.caret);
        }
        let sibs = self.doc.siblings(self.caret.seg);
        let i = sibs.iter().position(|s| *s == self.caret.seg).expect("is own sibling");
        self.offset(self.caret) - (sibs.len() - i)
    }
    /// The position at an offset in the text, and whether it's inside a bubble.
    pub fn at_offset(&self, off: usize) -> (Pos, bool) {
        for s in self.spans() {
            if off < s.start {
                let sibs = self.doc.siblings(s.seg);
                return (Pos { seg: sibs[off - s.bubble - 1], off: 0 }, true);
            }
            if off <= s.end {
                return (Pos { seg: s.seg, off: off - s.start }, false);
            }
        }
        (self.doc.end(self.tip), false)
    }
    /// Each bubble's extent in the text, and its active branch.
    pub fn bubbles(&self) -> Vec<(std::ops::Range<usize>, SegId)> {
        self.spans()[1..].iter().map(|s| (s.bubble..s.start, s.seg)).collect()
    }

    /// Deletes a range of the text.
    fn remove(&mut self, mut a: usize, mut b: usize) {
        let was_inside = self.inside;
        let outside_left = !was_inside && self.caret_offset() == a;
        for (r, seg) in self.bubbles() {
            let touches = a < r.end && r.start < b;
            if !touches || (a <= r.start && r.end <= b) {
                continue;
            }
            let within = r.start <= a && b <= r.end;
            // Backspace after the bubble, or Delete before it, deletes it all.
            let whole = !within || (!was_inside && (b == r.end || outside_left));
            if !whole {
                let sibs = self.doc.siblings(seg).to_vec();
                let range = a - r.start..(b - r.start).min(sibs.len());
                if range.is_empty() {
                    return;
                }
                if range.len() < sibs.len() {
                    for s in &sibs[range.clone()] {
                        self.doc.delete(*s);
                    }
                    // The caret stays where the deleted marks were.
                    match range.start.checked_sub(1) {
                        Some(left) => {
                            self.show(sibs[left]);
                            self.enter(sibs[left]);
                        }
                        None => {
                            self.show(sibs[range.end]);
                            let parent = self.doc.get(sibs[range.end]).parent.expect("in a bubble");
                            self.go_to(self.doc.end(parent));
                        }
                    }
                    return;
                }
            }
            (a, b) = (a.min(r.start), b.max(r.end));
        }
        let spans = self.spans();
        let h = spans.iter().position(|s| a <= s.end).expect("in bounds");
        let t = spans.iter().rposition(|s| s.start <= b).expect("in bounds");
        let mut text = self.doc.get(spans[h].seg).text[..a - spans[h].start].to_owned();
        let off = text.len();
        text.push_str(&self.doc.get(spans[t].seg).text[b - spans[t].start..]);
        for s in &spans[h + 1..=t] {
            self.doc.join(s.seg);
        }
        let seg = spans[h].seg;
        self.doc.set_text(seg, text);
        if t + 1 == spans.len() {
            self.tip = seg;
        }
        self.caret = Pos { seg, off };
        self.inside = false;
    }
    /// Types text at the caret. A `>` outside of a comment makes a bubble, or
    /// inside of a bubble, another branch.
    fn insert(&mut self, text: &str) {
        for c in text.chars() {
            let Pos { seg, off } = self.caret;
            let before = &self.doc.get(seg).text[..off];
            let new = if c != '>' {
                None
            } else if self.inside {
                self.doc.add_sibling(seg)
            } else if notation::parse(before).in_comment {
                None
            } else {
                Some(self.doc.split_at(self.caret))
            };
            if let Some(new) = new {
                if self.tip == seg || self.inside {
                    self.tip = new;
                }
                self.caret = Pos { seg: new, off: 0 };
                self.inside = true;
            } else {
                let mut text = self.doc.get(seg).text.clone();
                text.insert(off, c);
                self.doc.set_text(seg, text);
                self.caret.off += c.len_utf8();
                self.inside = false;
            }
        }
    }

    pub fn twist(&mut self, mv: &str, inversion: Inversion) {
        let before = self.snapshot();
        let text = &self.doc.get(self.caret.seg).text;
        let (text, off) = notation::twist(text, self.caret.off, mv, inversion);
        self.doc.set_text(self.caret.seg, text);
        self.caret.off = off;
        self.inside = false;
        self.record(before);
    }

    /// Like `edit`, but always its own undo step.
    pub fn edit_once(&mut self, new: &str, caret: usize) {
        self.typing = None;
        self.edit(new, caret);
        self.typing = None;
    }
    /// Applies an edit made in the text box, where `caret` is its caret after.
    pub fn edit(&mut self, new: &str, caret: usize) {
        self.anchor = None;
        let before = self.snapshot();
        let now = Instant::now();
        let coalesce = self.typing.is_some_and(|(pos, t)| {
            pos == self.caret && now - t < Duration::from_secs(1)
        });

        // The edit ends at the caret, which disambiguates the diff.
        let old = self.text();
        let tail = &new[caret..];
        let suffix = if old.ends_with(tail) {
            tail.len()
        } else {
            let pairs = old.chars().rev().zip(new.chars().rev());
            pairs.take_while(|(a, b)| a == b).map(|(c, _)| c.len_utf8()).sum()
        };
        let (old_head, new_head) = (&old[..old.len() - suffix], &new[..new.len() - suffix]);
        let pairs = old_head.chars().zip(new_head.chars());
        let prefix: usize = pairs.take_while(|(a, b)| a == b).map(|(c, _)| c.len_utf8()).sum();
        if prefix < old_head.len() {
            self.remove(prefix, old_head.len());
            self.show(self.tip);
        } else {
            (self.caret, self.inside) = self.at_offset(prefix);
            if self.inside {
                self.show(self.caret.seg);
            }
        }
        self.insert(&new_head[prefix..].replace(['\r', MARK, END], ""));
        self.normalize();

        if before.doc == self.doc {
            return;
        }
        // Adding or deleting branches is always its own step.
        let forked = !self.doc.ids().eq(before.doc.ids());
        if !coalesce || forked {
            self.record(before);
        }
        self.redo.clear();
        self.typing = (!forked).then_some((self.caret, now));
    }

    /// Right before or after a bubble (with only whitespace between), adds a
    /// branch first or last in it. Otherwise types a `>`, even in a comment or
    /// in twist mode: splits the text at the caret with a bubble, or in a
    /// bubble, adds a branch.
    pub fn fork(&mut self) {
        let before = self.snapshot();
        let Pos { seg, off } = self.caret;
        let text = self.doc.get(seg).text.clone();
        let (gap, partway) = notation::gap(&text, off);
        let path = self.doc.path(self.tip);
        let next = path.iter().position(|s| *s == seg).and_then(|i| path.get(i + 1));
        // Right beside a bubble, as tokens go, it adds a branch on that side.
        let beside = if self.inside || partway {
            None
        } else if gap == 0 && let Some(parent) = self.doc.get(seg).parent {
            Some(self.doc.insert_child(parent, self.doc.get(parent).children.len()))
        } else if gap == notation::parse(&text).tokens.len() && next.is_some() {
            Some(self.doc.insert_child(seg, 0))
        } else {
            None
        };
        if let Some(new) = beside {
            self.show(new);
            self.enter(new);
        } else if !self.inside && notation::parse(&text[..off]).in_comment {
            // Since a `>` would only be text there.
            let new = self.doc.split_at(self.caret);
            if self.tip == seg {
                self.tip = new;
            }
            (self.caret, self.inside) = (Pos { seg: new, off: 0 }, true);
        } else {
            self.insert(">");
        }
        self.record(before);
    }
    /// Deletes the bubble before the caret's segment, keeping the active branch,
    /// the same as Backspace right after it in twist mode.
    pub fn join(&mut self) {
        let before = self.snapshot();
        self.join_into_parent(self.caret.seg);
        self.record(before);
    }
    /// Adds a space if the tokens on either side would run together.
    fn join_into_parent(&mut self, seg: SegId) {
        if let Some(parent) = self.doc.get(seg).parent {
            let text = &self.doc.get(parent).text;
            let padding = notation::padding(text, &self.doc.get(seg).text);
            self.doc.set_text(parent, format!("{text}{padding}"));
        }
        if let Some(start) = self.doc.join(seg) {
            if self.caret.seg == seg {
                self.caret = Pos { seg: start.seg, off: start.off + self.caret.off };
                self.inside = false;
            }
            if self.tip == seg {
                self.tip = start.seg;
            }
        }
    }
    /// Deletes the caret's segment and everything after it.
    pub fn delete_branch(&mut self) {
        let before = self.snapshot();
        if let Some(parent) = self.doc.delete(self.caret.seg) {
            self.show(parent);
            self.select(parent);
        }
        self.record(before);
    }
    pub fn replace(&mut self, doc: Doc) {
        let before = self.snapshot();
        let new = Self::new(doc);
        (self.doc, self.tip, self.caret) = (new.doc, new.tip, new.caret);
        self.inside = false;
        self.record(before);
    }
}
