//! Document plus cursor, undo history, and views. No UI toolkit.

use crate::doc::{Doc, Pos, SegId};
use crate::notation::{self, Inversion, Parsed, Unit};
use crate::puzzle::Perm;
use std::collections::BTreeMap;
use std::ops::Range;
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
/// them written as a bubble on a line of its own: an `OPEN` line break, a `MARK`
/// for each branch, and an `END` line break. Neither line break is part of a
/// segment. Every caret position on a bubble's line is in the bubble: before
/// its first `>`, or after one of them, whose branch is then the active one.
pub const OPEN: char = '\x03';
pub const MARK: char = '\x01';
pub const END: char = '\x02';

/// Where the caret is on the line of the bubble before its segment, if it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inside {
    /// Not on it.
    No,
    /// Before the first `>`. The segment is any of the bubble's branches.
    Start,
    /// Right after the segment's `>`.
    Mark,
}

/// A stretch of the text box's text. A bubble's is the one before its segment.
struct Span {
    seg: SegId,
    bubble: usize,
    start: usize,
    end: usize,
}

/// One end of a selection: a position, and where it is on a bubble's line.
type Spot = (Pos, Inside);

#[derive(Debug, Clone)]
struct Snapshot {
    doc: Doc,
    tip: SegId,
    caret: Pos,
    inside: Inside,
    anchor: Option<Spot>,
}

#[derive(Debug, Clone)]
pub struct Session {
    doc: Doc,
    /// Leaf that the path goes to.
    tip: SegId,
    /// Which child each segment's path last went to. Can be stale.
    active: BTreeMap<SegId, SegId>,
    caret: Pos,
    /// Whether the caret is on the line of the bubble before it.
    inside: Inside,
    /// Fixed end of the selection. The caret is the other end. Either both are
    /// in one bubble, or neither is in a bubble and both are on the path.
    anchor: Option<Spot>,
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
            inside: Inside::No,
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
    pub fn inside(&self) -> Inside {
        self.inside
    }
    fn in_bubble(&self) -> bool {
        self.inside != Inside::No
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
        if self.doc.get(seg).parent.is_some() {
            self.inside = Inside::Mark;
        }
    }
    /// Puts the caret in the bubble before a segment, before its first mark,
    /// keeping the active branch.
    pub fn enter_start(&mut self, seg: SegId) {
        let Some(parent) = self.doc.get(seg).parent else {
            return;
        };
        let active = self.active_child(parent).unwrap_or(seg);
        self.go_to(Pos {
            seg: active,
            off: 0,
        });
        self.inside = Inside::Start;
    }
    fn active_child(&self, parent: SegId) -> Option<SegId> {
        let children = &self.doc.get(parent).children;
        let active = self.active.get(&parent).filter(|c| children.contains(c));
        active.or(children.first()).copied()
    }
    /// Puts the caret at a spot.
    fn place(&mut self, (pos, inside): Spot) {
        match inside {
            Inside::No => self.go_to(pos),
            Inside::Mark => self.enter(pos.seg),
            Inside::Start => self.enter_start(pos.seg),
        }
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
        Snapshot {
            doc: self.doc.clone(),
            tip: self.tip,
            caret: self.caret,
            inside: self.inside,
            anchor: self.anchor,
        }
    }
    fn restore(&mut self, s: Snapshot) {
        (self.doc, self.tip, self.caret, self.inside) = (s.doc, s.tip, s.caret, s.inside);
        self.show(self.tip);
        self.typing = None;
        self.anchor = s.anchor;
    }
    /// Splits on any `>` that edits uncommented, and extends the path to a leaf.
    fn normalize(&mut self) {
        for (seg, span, new) in self.doc.normalize() {
            if self.tip == seg {
                self.tip = new;
            }
            if self.caret.seg == seg && self.caret.off > span.start {
                self.caret = Pos {
                    seg: new,
                    off: self.caret.off.saturating_sub(span.end),
                };
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
    /// Like `record`, but keeps the selection.
    fn record_keeping_selection(&mut self, before: Snapshot) {
        let anchor = self.anchor;
        self.record(before);
        self.anchor = anchor;
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
        self.inside = Inside::No;
        self.anchor = None;
    }

    /// Selects a stretch of a path, leaving the caret at `caret`. One end must be
    /// on the path to the other; otherwise this only moves the caret.
    pub fn set_selection(&mut self, anchor: Pos, caret: Pos) {
        self.go_to(anchor);
        self.go_to(caret);
        let on_path = self.doc.path(self.tip).contains(&anchor.seg);
        self.anchor = (on_path && anchor != caret).then_some((anchor, Inside::No));
    }
    /// Moves the caret, selecting from the old anchor or caret.
    pub fn extend_to(&mut self, pos: Pos) {
        self.set_selection(self.anchor(), pos);
    }
    /// Fixed end of the selection, or the caret if there's no selection.
    pub fn anchor(&self) -> Pos {
        self.anchor_spot().0
    }
    fn anchor_spot(&self) -> Spot {
        self.anchor.unwrap_or((self.caret, self.inside))
    }
    /// The anchor's offset in the text.
    pub fn anchor_offset(&self) -> usize {
        self.spot_offset(self.anchor_spot()).expect("on path")
    }
    /// Moves the caret to an offset in the text.
    pub fn set_caret_offset(&mut self, off: usize) {
        self.place(self.at_offset(off));
    }
    /// Selects between two offsets in the text, leaving the caret at the
    /// second. A selection that starts in a bubble stays in it, and one that
    /// starts outside of a bubble goes all the way over it or not at all.
    pub fn select_offsets(&mut self, anchor: usize, caret: usize) {
        let a = self.at_offset(anchor);
        let bubble = self
            .bubbles()
            .into_iter()
            .find(|(r, _)| r.start < caret && caret < r.end);
        let caret = if a.1 != Inside::No {
            let (r, _) = self
                .bubbles()
                .into_iter()
                .find(|(r, _)| r.start < anchor && anchor < r.end)
                .expect("in a bubble");
            caret.clamp(r.start + 1, r.end - 1)
        } else if let Some((r, _)) = bubble {
            if anchor <= r.start { r.start } else { r.end }
        } else {
            caret
        };
        self.set_caret_offset(caret);
        if caret != anchor {
            self.anchor = Some(a);
        }
    }
    /// Ends of the unit that a click on the character at `pos` selects. Off a
    /// segment's end, that's the bubble after it.
    pub fn unit_at(&self, pos: Pos, unit: Unit) -> Option<(Pos, Pos)> {
        let text = &self.doc.get(pos.seg).text;
        let r = notation::unit_at(text, pos.off, unit)?;
        Some((
            Pos {
                seg: pos.seg,
                off: r.start,
            },
            Pos {
                seg: pos.seg,
                off: r.end,
            },
        ))
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
        let key = |p: &Pos| {
            (
                path.iter().position(|s| *s == p.seg).expect("on path"),
                p.off,
            )
        };
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
            self.go_to(Pos {
                seg: sibs[j],
                off: 0,
            });
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
        let at = |gap| Pos {
            seg,
            off: notation::gap_offset(text, gap),
        };
        if self.in_bubble() && forward {
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
            Pos {
                seg,
                off: notation::gap_offset(text, last),
            }
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
        if self.in_bubble() {
            self.step_bubble(forward);
            return;
        }
        let path = self.doc.path(self.tip);
        let Pos { seg, off } = self.caret;
        let i = path.iter().position(|s| *s == seg).expect("caret on path");
        let tokens = notation::parse(&self.doc.get(seg).text).tokens;
        let pos = if forward {
            match tokens.iter().find(|t| t.span.end > off) {
                Some(t) => Pos {
                    seg,
                    off: t.span.end,
                },
                None => path
                    .get(i + 1)
                    .map_or(self.caret, |&next| Pos { seg: next, off: 0 }),
            }
        } else {
            match tokens.iter().rev().find(|t| t.span.start < off) {
                Some(t) => Pos {
                    seg,
                    off: t.span.start,
                },
                None if i > 0 => self.doc.end(path[i - 1]),
                None => Pos { seg, off: 0 },
            }
        };
        self.move_caret(pos, extend);
    }
    /// Like `step`, but past a whole run: the tokens up to a bracket,
    /// separator or line break.
    pub fn step_run(&mut self, forward: bool, extend: bool) {
        self.move_caret(self.run_stop(forward), extend);
    }
    fn run_stop(&self, forward: bool) -> Pos {
        let Pos { seg, off } = self.caret;
        let text = &self.doc.get(seg).text;
        let n = notation::parse(text).tokens.len();
        let (gap, partway) = notation::gap(text, off);
        let at = |gap| Pos {
            seg,
            off: notation::gap_offset(text, gap),
        };
        if self.in_bubble() {
            self.stepped(forward)
        } else if forward && gap < n {
            at(notation::run_gap(text, gap, true))
        } else if !forward && partway {
            at(notation::run_gap(text, gap + 1, false))
        } else if !forward && gap > 0 {
            at(notation::run_gap(text, gap, false))
        } else {
            self.stepped(forward)
        }
    }
    /// Moves the caret to the end of its segment, or its start going
    /// backwards, or else to that of the next segment along the path.
    pub fn step_segment(&mut self, forward: bool, extend: bool) {
        self.move_caret(self.segment_stop(forward), extend);
    }
    fn segment_stop(&self, forward: bool) -> Pos {
        let path = self.doc.path(self.tip);
        let Pos { seg, off } = self.caret;
        let i = path.iter().position(|s| *s == seg).expect("caret on path");
        let end = |seg| {
            let text = &self.doc.get(seg).text;
            let n = notation::parse(text).tokens.len();
            Pos {
                seg,
                off: notation::gap_offset(text, n),
            }
        };
        if forward {
            if self.in_bubble() || off < end(seg).off {
                end(seg)
            } else {
                path.get(i + 1).map_or(self.caret, |&next| end(next))
            }
        } else if off > 0 && !self.in_bubble() {
            Pos { seg, off: 0 }
        } else {
            let prev = path[..i].last().copied().unwrap_or(seg);
            Pos { seg: prev, off: 0 }
        }
    }
    /// Moves the caret to the start or end of the path.
    pub fn step_all(&mut self, forward: bool, extend: bool) {
        let pos = if forward {
            self.doc.end(self.tip)
        } else {
            Pos {
                seg: self.doc.root(),
                off: 0,
            }
        };
        self.move_caret(pos, extend);
    }

    /// Moves the caret between the sides of a bubble and its active branch.
    /// Returns whether there's a bubble there.
    pub fn step_bubble(&mut self, forward: bool) -> bool {
        let Pos { seg, off } = self.caret;
        let path = self.doc.path(self.tip);
        let i = path.iter().position(|s| *s == seg).expect("caret on path");
        if self.in_bubble() {
            let pos = if forward {
                self.caret
            } else {
                self.doc.end(path[i - 1])
            };
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

    /// Deletes the selection, or else up to where `step_run` or `step_segment`
    /// would go, and any group whose bracket that covers. Across a fork, it
    /// deletes the fork, like `delete`.
    pub fn delete_to(&mut self, forward: bool, by_segment: bool) {
        let to = if by_segment {
            self.segment_stop(forward)
        } else {
            self.run_stop(forward)
        };
        let Pos { seg, off } = self.caret;
        if self.anchor.is_some() || self.in_bubble() || to.seg != seg {
            self.delete(forward);
            return;
        }
        if to.off == off {
            return;
        }
        let before = self.snapshot();
        let text = &self.doc.get(seg).text;
        let (mut lo, hi) = (off.min(to.off), off.max(to.off));
        // Going backwards, from the first token, so as to keep a line break.
        if !forward {
            lo += text[lo..hi].len() - text[lo..hi].trim_start().len();
        }
        let span = notation::close_groups(text, lo..hi);
        let (new, off) = notation::delete_span(text, span, off);
        self.doc.set_text(seg, new);
        self.caret.off = off;
        self.record(before);
    }

    /// Deletes the selection, or else one token. A bubble counts as a token.
    pub fn delete(&mut self, forward: bool) {
        let before = self.snapshot();
        let (a, c) = (self.anchor_offset(), self.caret_offset());
        let Pos { seg, off } = self.caret;
        if a != c {
            self.remove(a.min(c), a.max(c), true);
        } else if self.in_bubble() {
            if !forward {
                self.remove(c - 1, c, false);
            }
        } else if let Some((new, off)) = notation::delete(&self.doc.get(seg).text, off, forward) {
            self.doc.set_text(seg, new);
            self.caret.off = off;
        } else {
            let path = self.doc.path(self.tip);
            let i = path.iter().position(|s| *s == seg).expect("caret on path");
            let bubble = if forward {
                path.get(i + 1).copied()
            } else {
                Some(seg)
            };
            if let Some(seg) = bubble {
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
                at += self.doc.siblings(seg).len() + 2;
            }
            let start = at;
            at += s.text.len();
            Span {
                seg,
                bubble,
                start,
                end: at,
            }
        };
        path.iter().map(span).collect()
    }
    pub fn text(&self) -> String {
        let mut out = String::new();
        for s in self.spans() {
            if s.start > s.bubble {
                out.push(OPEN);
                out.extend(std::iter::repeat_n(MARK, s.start - s.bubble - 2));
                out.push(END);
            }
            out.push_str(&self.doc.get(s.seg).text);
        }
        out
    }
    /// Offset in the text, if it's on the path.
    pub fn offset_of(&self, pos: Pos) -> Option<usize> {
        self.spans()
            .iter()
            .find(|s| s.seg == pos.seg)
            .map(|s| s.start + pos.off)
    }
    fn offset(&self, pos: Pos) -> usize {
        self.offset_of(pos).expect("on path")
    }
    /// Offset of a spot in the text, if it's there.
    fn spot_offset(&self, (pos, inside): Spot) -> Option<usize> {
        if inside == Inside::No {
            return self.offset_of(pos);
        }
        let parent = self.doc.get(pos.seg).parent?;
        let sibs = &self.doc.get(parent).children;
        let s = self.spans().into_iter().find(|s| sibs.contains(&s.seg))?;
        Some(match inside {
            Inside::Start => s.bubble + 1,
            _ => s.bubble + 2 + sibs.iter().position(|c| *c == pos.seg)?,
        })
    }
    pub fn caret_offset(&self) -> usize {
        self.spot_offset((self.caret, self.inside))
            .expect("on path")
    }
    /// The position at an offset in the text, and where it is on a bubble's
    /// line. Before a bubble's first mark, the position is the active branch's.
    pub fn at_offset(&self, off: usize) -> (Pos, Inside) {
        for s in self.spans() {
            if off < s.start {
                let gap = off - s.bubble - 1;
                if gap == 0 {
                    return (Pos { seg: s.seg, off: 0 }, Inside::Start);
                }
                let sibs = self.doc.siblings(s.seg);
                return (
                    Pos {
                        seg: sibs[gap - 1],
                        off: 0,
                    },
                    Inside::Mark,
                );
            }
            if off <= s.end {
                return (
                    Pos {
                        seg: s.seg,
                        off: off - s.start,
                    },
                    Inside::No,
                );
            }
        }
        (self.doc.end(self.tip), Inside::No)
    }
    /// Each bubble's extent in the text, from the end of the line before it to
    /// the start of the line after it, and its active branch.
    pub fn bubbles(&self) -> Vec<(std::ops::Range<usize>, SegId)> {
        self.spans()[1..]
            .iter()
            .map(|s| (s.bubble..s.start, s.seg))
            .collect()
    }

    /// Deletes a range of the text. Within a bubble's marks, that's their
    /// branches, but a lone `>` that isn't `selected` keeps its text. A bubble's
    /// line break, from outside of it, deletes it the same way, and from
    /// inside, nothing.
    fn remove(&mut self, mut a: usize, mut b: usize, selected: bool) {
        let was_inside = self.in_bubble();
        for (r, seg) in self.bubbles() {
            let touches = a < r.end && r.start < b;
            if !touches || (a <= r.start && r.end <= b) {
                continue;
            }
            let marks = r.start + 1..r.end - 1;
            if marks.start <= a && b <= marks.end {
                let sibs = self.doc.siblings(seg).to_vec();
                let range = a - marks.start..b - marks.start;
                if range.is_empty() {
                    return;
                }
                if range.len() == sibs.len() {
                    if !selected {
                        self.join_into_parent(seg);
                        return;
                    }
                    let parent = self.doc.get(seg).parent.expect("in a bubble");
                    for s in &sibs {
                        self.doc.delete(*s);
                    }
                    self.show(parent);
                    self.go_to(self.doc.end(parent));
                    return;
                }
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
                        self.enter_start(sibs[range.end]);
                    }
                }
                return;
            }
            if b - a == 1 && !selected && (a == r.start || b == r.end) {
                if !was_inside {
                    self.join_into_parent(seg);
                }
                return;
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
        self.inside = Inside::No;
    }
    /// Types a character at the caret. A `>` outside of a comment makes a
    /// bubble, or on a bubble's line, another branch. Anything else before a
    /// bubble's first mark goes on a line of its own before the bubble.
    fn type_char(&mut self, c: char) {
        let Pos { seg, off } = self.caret;
        let parent = self.doc.get(seg).parent;
        let new = match (c, self.inside) {
            ('>', Inside::Mark) => self.doc.add_sibling(seg),
            ('>', Inside::Start) => Some(self.doc.insert_child(parent.expect("in a bubble"), 0)),
            ('>', Inside::No) if !notation::parse(&self.doc.get(seg).text[..off]).in_comment => {
                Some(self.doc.fork_at(seg, off))
            }
            ('\n', Inside::Start) => {
                let p = parent.expect("in a bubble");
                let mut text = self.doc.get(p).text.clone();
                text.push('\n');
                self.doc.set_text(p, text);
                return;
            }
            (_, Inside::Start) => {
                let p = parent.expect("in a bubble");
                self.doc.set_text(p, format!("{}\n", self.doc.get(p).text));
                self.caret = self.doc.end(p);
                self.inside = Inside::No;
                return self.type_char(c);
            }
            _ => None,
        };
        if let Some(new) = new {
            if self.tip == seg || self.in_bubble() {
                self.tip = new;
            }
            self.caret = Pos { seg: new, off: 0 };
            self.inside = Inside::Mark;
        } else {
            let mut text = self.doc.get(seg).text.clone();
            text.insert(off, c);
            self.doc.set_text(seg, text);
            self.caret.off += c.len_utf8();
            self.inside = Inside::No;
        }
    }
    /// Puts text in at the caret as is, and then splits it on any `>`. The
    /// text after the caret goes with the last of the pasted text.
    fn insert(&mut self, text: &str) {
        if let Some(seg) = (self.inside == Inside::Start).then_some(self.caret.seg) {
            let parent = self.doc.get(seg).parent.expect("in a bubble");
            let t = format!("{}\n", self.doc.get(parent).text);
            self.doc.set_text(parent, t);
            self.caret = self.doc.end(parent);
        }
        let Pos { seg, off } = self.caret;
        let mut t = self.doc.get(seg).text.clone();
        t.insert_str(off, text);
        self.doc.set_text(seg, t);
        self.caret.off += text.len();
        self.inside = Inside::No;
        self.normalize();
    }

    /// Puts brackets around the selection, leaving what was selected selected.
    /// Returns false, doing nothing, if there's no selection.
    pub fn wrap(&mut self, open: &str, close: &str) -> bool {
        let Some((anchor, Inside::No)) = self.anchor else {
            return false;
        };
        let before = self.snapshot();
        let caret = self.caret;
        let backward = self.offset(caret) < self.offset(anchor);
        let (mut lo, mut hi) = if backward {
            (caret, anchor)
        } else {
            (anchor, caret)
        };
        // The end first, so that the start's offset still holds.
        for (pos, c) in [(hi, close), (lo, open)] {
            let mut text = self.doc.get(pos.seg).text.clone();
            text.insert_str(pos.off, c);
            self.doc.set_text(pos.seg, text);
        }
        lo.off += open.len();
        if hi.seg == lo.seg {
            hi.off += open.len();
        }
        self.record(before);
        if backward {
            self.set_selection(hi, lo);
        } else {
            self.set_selection(lo, hi);
        }
        true
    }

    pub fn twist(&mut self, mv: &str, inversion: Inversion) {
        let before = self.snapshot();
        let text = &self.doc.get(self.caret.seg).text;
        let (text, off) = notation::twist(text, self.caret.off, mv, inversion);
        self.doc.set_text(self.caret.seg, text);
        self.caret.off = off;
        self.inside = Inside::No;
        self.record(before);
    }
    /// Applies an edit made in the text box, where `caret` is its caret after.
    pub fn edit(&mut self, new: &str, caret: usize) {
        let before = self.snapshot();
        let selected = self.anchor.take().is_some();
        let now = Instant::now();
        let coalesce = self
            .typing
            .is_some_and(|(pos, t)| pos == self.caret && now - t < Duration::from_secs(1));

        // The edit ends at the caret, which disambiguates the diff.
        let old = self.text();
        let tail = &new[caret..];
        let suffix = if old.ends_with(tail) {
            tail.len()
        } else {
            let pairs = old.chars().rev().zip(new.chars().rev());
            pairs
                .take_while(|(a, b)| a == b)
                .map(|(c, _)| c.len_utf8())
                .sum()
        };
        let (old_head, new_head) = (&old[..old.len() - suffix], &new[..new.len() - suffix]);
        let pairs = old_head.chars().zip(new_head.chars());
        let prefix: usize = pairs
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum();
        if prefix < old_head.len() {
            self.remove(prefix, old_head.len(), selected);
            self.show(self.tip);
        } else {
            (self.caret, self.inside) = self.at_offset(prefix);
            if self.in_bubble() {
                self.show(self.caret.seg);
            }
        }
        for c in new_head[prefix..].chars() {
            if !matches!(c, '\r' | OPEN | MARK | END) {
                self.type_char(c);
            }
        }
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
        if let Some((parent, index)) = self.beside_bubble() {
            let new = self.doc.insert_child(parent, index);
            self.show(new);
            self.enter(new);
        } else if self.in_bubble() {
            self.type_char('>');
        } else {
            let new = self.split_at_gap();
            (self.caret, self.inside) = (Pos { seg: new, off: 0 }, Inside::Mark);
        }
        self.record(before);
    }
    /// Right beside a bubble, as tokens go, the bubble and the index on that
    /// side of it.
    fn beside_bubble(&self) -> Option<(SegId, usize)> {
        if self.in_bubble() {
            return None;
        }
        let Pos { seg, off } = self.caret;
        let text = &self.doc.get(seg).text;
        let (gap, partway) = notation::gap(text, off);
        let gap = gap + usize::from(partway);
        let path = self.doc.path(self.tip);
        let i = path.iter().position(|s| *s == seg).expect("caret on path");
        if gap == 0
            && let Some(parent) = self.doc.get(seg).parent
        {
            Some((parent, self.doc.get(parent).children.len()))
        } else if gap == notation::parse(text).tokens.len() && i + 1 < path.len() {
            Some((seg, 0))
        } else {
            None
        }
    }
    /// Forks at the gap at or after the caret, so past a token or comment that
    /// it's partway through. The fork takes the gap's place, whitespace and
    /// all. Returns the new segment.
    fn split_at_gap(&mut self) -> SegId {
        let Pos { seg, off } = self.caret;
        let text = self.doc.get(seg).text.clone();
        let tokens = notation::parse(&text).tokens;
        let (gap, partway) = notation::gap(&text, off);
        let gap = gap + usize::from(partway);
        let start = notation::gap_offset(&text, gap);
        let end = tokens.get(gap).map_or(text.len(), |t| t.span.start);
        let new = self.doc.split_around(seg, start..end);
        if self.tip == seg {
            self.tip = new;
        }
        new
    }
    /// Deletes the bubble before the caret's segment, keeping the active
    /// branch, the same as Backspace right after it in twist mode.
    pub fn join(&mut self) {
        let before = self.snapshot();
        self.join_into_parent(self.caret.seg);
        self.record(before);
    }
    /// Deletes a bubble's line: the parent and the branch are left with the
    /// line break before it between them.
    fn join_into_parent(&mut self, seg: SegId) {
        let Some(parent) = self.doc.get(seg).parent else {
            return;
        };
        let text = format!("{}\n", self.doc.get(parent).text);
        self.doc.set_text(parent, text);
        if let Some(start) = self.doc.join(seg) {
            if self.caret.seg == seg {
                self.caret = Pos {
                    seg: start.seg,
                    off: start.off + self.caret.off,
                };
                self.inside = Inside::No;
            }
            if self.tip == seg {
                self.tip = start.seg;
            }
        }
    }

    // ---- the clipboard ----

    /// Which gap between a bubble's marks a spot in it is in.
    fn gap_of(&self, (pos, inside): Spot) -> usize {
        match inside {
            Inside::Mark => {
                1 + self
                    .doc
                    .siblings(pos.seg)
                    .iter()
                    .position(|s| *s == pos.seg)
                    .expect("is own sibling")
            }
            _ => 0,
        }
    }
    fn spot_at_gap(&self, parent: SegId, gap: usize) -> Spot {
        match gap.checked_sub(1) {
            Some(i) => (
                Pos {
                    seg: self.doc.get(parent).children[i],
                    off: 0,
                },
                Inside::Mark,
            ),
            None => (
                Pos {
                    seg: self.active_child(parent).expect("has children"),
                    off: 0,
                },
                Inside::Start,
            ),
        }
    }
    /// The bubble that the selection is in, and the indices of its selected
    /// branches.
    fn selected_branches(&self) -> Option<(SegId, Range<usize>)> {
        let anchor = self.anchor.filter(|a| a.1 != Inside::No)?;
        let parent = self.doc.get(self.caret.seg).parent?;
        let (a, c) = (self.gap_of(anchor), self.gap_of((self.caret, self.inside)));
        Some((parent, a.min(c)..a.max(c)))
    }
    /// The selection as it's copied, or `None` if there isn't one. Selected
    /// `>`s copy their branches whole, written as in a file. Across a bubble,
    /// only its active branch is.
    pub fn copy(&self) -> Option<String> {
        let (a, c) = (self.anchor_offset(), self.caret_offset());
        if a == c {
            return None;
        }
        if let Some((parent, range)) = self.selected_branches() {
            let branches = &self.doc.get(parent).children[range];
            return Some(self.doc.branches_to_file(branches));
        }
        let mut out = String::new();
        for ch in self.text()[a.min(c)..a.max(c)].chars() {
            match ch {
                OPEN => out.push_str("\n>"),
                MARK => {}
                END => out.push('\n'),
                ch => out.push(ch),
            }
        }
        Some(out)
    }
    /// Copies and deletes the selection.
    pub fn cut(&mut self) -> Option<String> {
        let out = self.copy()?;
        self.delete(false);
        Some(out)
    }
    /// Pastes in place of the selection. Text that starts with a `>` is
    /// branches, which go in a bubble at the caret like Cmd+Enter's empty one;
    /// otherwise, the text after the caret goes with the last of the pasted
    /// text.
    pub fn paste(&mut self, text: &str) {
        let text = text.replace('\r', "");
        let before = self.snapshot();
        let branches = text
            .starts_with('>')
            .then(|| Doc::from_file(&text).ok())
            .flatten();
        let selected = self.selected_branches();
        if let Some((parent, range)) = &selected
            && branches.is_some()
        {
            let replaced: Vec<SegId> = self.doc.get(*parent).children[range.clone()].to_vec();
            for c in replaced {
                self.doc.delete(c);
            }
        } else if self.anchor.is_some() {
            let (a, c) = (self.anchor_offset(), self.caret_offset());
            self.remove(a.min(c), a.max(c), true);
        }
        let Some(src) = branches else {
            self.insert(&text);
            self.record(before);
            return;
        };
        let (parent, index) = match selected {
            Some((parent, range)) => (parent, range.start),
            None => self.branch_slot(),
        };
        let mut last = parent;
        for (i, &b) in src.get(src.root()).children.iter().enumerate() {
            last = self.doc.graft(parent, index + i, &src, b);
        }
        self.show(last);
        self.enter(last);
        self.record(before);
    }
    /// Where new branches go at the caret, as Cmd+Enter's does: a bubble and
    /// an index in it. Mid-text, this forks, and the text after is the first
    /// branch unless there isn't any.
    fn branch_slot(&mut self) -> (SegId, usize) {
        let Pos { seg, .. } = self.caret;
        match self.inside {
            Inside::Start => (self.doc.get(seg).parent.expect("in a bubble"), 0),
            Inside::Mark => {
                let parent = self.doc.get(seg).parent.expect("in a bubble");
                (parent, self.gap_of((self.caret, self.inside)))
            }
            Inside::No => {
                if let Some(slot) = self.beside_bubble() {
                    return slot;
                }
                let new = self.split_at_gap();
                let n = self.doc.get(new);
                if n.text.is_empty() && n.children.is_empty() {
                    self.doc.delete(new);
                    self.tip = seg;
                }
                (seg, self.doc.get(seg).children.len())
            }
        }
    }

    // ---- moving and duplicating lines ----

    /// On a bubble's line, the selected `>`s, or else the one before the
    /// caret: the bubble, and their indices.
    fn branch_block(&self) -> Option<(SegId, Range<usize>)> {
        if let Some(b) = self.selected_branches() {
            return Some(b);
        }
        if self.inside != Inside::Mark {
            return None;
        }
        let g = self.gap_of((self.caret, self.inside));
        Some((self.doc.get(self.caret.seg).parent?, g - 1..g))
    }
    /// Moves the caret and anchor, both in a bubble, by a number of gaps.
    fn shift_gaps(&mut self, parent: SegId, (anchor, caret): (Option<usize>, usize), by: isize) {
        let at = |g: usize| g.saturating_add_signed(by);
        self.place(self.spot_at_gap(parent, at(caret)));
        self.anchor = anchor.map(|a| self.spot_at_gap(parent, at(a)));
    }
    fn gaps(&self) -> (Option<usize>, usize) {
        (
            self.anchor.map(|a| self.gap_of(a)),
            self.gap_of((self.caret, self.inside)),
        )
    }
    /// The lines that the selection or caret is on, if they're in one
    /// segment: it, their extent without the last line break, and the end of
    /// the selection, which can be at the start of the line after them.
    fn line_block(&self) -> Option<(SegId, Range<usize>, usize)> {
        let a = self.anchor_spot();
        if self.in_bubble() || a.1 != Inside::No || a.0.seg != self.caret.seg {
            return None;
        }
        let seg = self.caret.seg;
        let text = &self.doc.get(seg).text;
        let (lo, hi) = (a.0.off.min(self.caret.off), a.0.off.max(self.caret.off));
        // A selection up to the start of a line doesn't take it in.
        let last = if hi > lo && text[..hi].ends_with('\n') {
            hi - 1
        } else {
            hi
        };
        let start = text[..lo].rfind('\n').map_or(0, |i| i + 1);
        let end = text[last..].find('\n').map_or(text.len(), |i| last + i);
        Some((seg, start..end, hi))
    }
    /// Option+Up or Down: moves the lines that the selection or caret is on
    /// past the next line, which can be a bubble's. On a bubble's line, it moves
    /// the selected `>`s, or the one before the caret, among their siblings,
    /// and before the first, it moves the bubble's line.
    pub fn move_lines(&mut self, down: bool) {
        let before = self.snapshot();
        if let Some((parent, range)) = self.branch_block() {
            let n = self.doc.get(parent).children.len();
            let gaps = self.gaps();
            if down && range.end < n {
                self.doc.move_child(parent, range.end, range.start);
                self.shift_gaps(parent, gaps, 1);
            } else if !down && range.start > 0 {
                self.doc.move_child(parent, range.start - 1, range.end - 1);
                self.shift_gaps(parent, gaps, -1);
            }
        } else if self.inside == Inside::Start {
            let seg = self.caret.seg;
            let parent = self.doc.get(seg).parent.expect("in a bubble");
            if down {
                let text = &self.doc.get(seg).text;
                let end = text.find('\n').unwrap_or(text.len());
                self.move_block(seg, 0..end, end, false);
            } else {
                let text = &self.doc.get(parent).text;
                let start = text.rfind('\n').map_or(0, |i| i + 1);
                self.move_block(parent, start..text.len(), text.len(), true);
            }
            self.enter_start(seg);
        } else if let Some((seg, block, end)) = self.line_block() {
            self.move_block(seg, block, end, down);
        }
        self.record_keeping_selection(before);
    }
    /// Moves a segment's lines past the next line, carrying the caret and
    /// anchor along if they're in them, or up to `end`. Past a bubble, they go
    /// to the start of the active branch or the end of the parent.
    fn move_block(&mut self, seg: SegId, block: Range<usize>, end: usize, down: bool) {
        let text = self.doc.get(seg).text.clone();
        let lines = &text[block.clone()];
        let path = self.doc.path(self.tip);
        let i = path.iter().position(|s| *s == seg).expect("on path");
        // Where the block's start goes.
        let to = if down && block.end < text.len() {
            let next_end = text[block.end + 1..]
                .find('\n')
                .map_or(text.len(), |i| block.end + 1 + i);
            let next = &text[block.end + 1..next_end];
            let new = format!(
                "{}{next}\n{lines}{}",
                &text[..block.start],
                &text[next_end..]
            );
            self.doc.set_text(seg, new);
            Pos {
                seg,
                off: block.start + next.len() + 1,
            }
        } else if !down && block.start > 0 {
            let prev_start = text[..block.start - 1].rfind('\n').map_or(0, |i| i + 1);
            let prev = &text[prev_start..block.start - 1];
            let new = format!(
                "{}{lines}\n{prev}{}",
                &text[..prev_start],
                &text[block.end..]
            );
            self.doc.set_text(seg, new);
            Pos {
                seg,
                off: prev_start,
            }
        } else if text.is_empty() {
            return;
        } else if down && let Some(&child) = path.get(i + 1) {
            self.doc
                .set_text(seg, text[..block.start.saturating_sub(1)].to_owned());
            let old = &self.doc.get(child).text;
            let sep = if old.is_empty() { "" } else { "\n" };
            self.doc.set_text(child, format!("{lines}{sep}{old}"));
            Pos { seg: child, off: 0 }
        } else if !down && i > 0 {
            let parent = path[i - 1];
            let rest = text.get(block.end + 1..).unwrap_or("").to_owned();
            self.doc.set_text(seg, rest);
            let old = self.doc.get(parent).text.clone();
            let sep = if old.is_empty() { "" } else { "\n" };
            self.doc.set_text(parent, format!("{old}{sep}{lines}"));
            Pos {
                seg: parent,
                off: old.len() + sep.len(),
            }
        } else {
            return;
        };
        let len = self.doc.get(to.seg).text.len();
        let carry = |p: &mut Pos| {
            if p.seg == seg && (block.start..=end.max(block.end)).contains(&p.off) {
                *p = Pos {
                    seg: to.seg,
                    off: (to.off + p.off - block.start).min(len),
                };
            }
        };
        if self.inside == Inside::No {
            carry(&mut self.caret);
        }
        if let Some((a, Inside::No)) = &mut self.anchor {
            carry(a);
        }
    }
    /// Shift+Option+Up or Down: duplicates the lines that the selection or
    /// caret is on, leaving it on the upper or lower copy. On a bubble's line,
    /// it duplicates the selected `>`s, or the one before the caret, and their
    /// branches.
    pub fn duplicate_lines(&mut self, down: bool) {
        let before = self.snapshot();
        if let Some((parent, range)) = self.branch_block() {
            let gaps = self.gaps();
            let src = self.doc.clone();
            let at = if down { range.end } else { range.start };
            for (i, &c) in src.get(parent).children[range.clone()].iter().enumerate() {
                self.doc.graft(parent, at + i, &src, c);
            }
            let by = if down { range.len() as isize } else { 0 };
            self.shift_gaps(parent, gaps, by);
        } else if let Some((seg, block, _)) = self.line_block() {
            let text = &self.doc.get(seg).text;
            let new = format!(
                "{}\n{}{}",
                &text[..block.end],
                &text[block.clone()],
                &text[block.end..]
            );
            self.doc.set_text(seg, new);
            if down {
                let by = block.len() + 1;
                self.caret.off += by;
                if let Some((a, _)) = &mut self.anchor {
                    a.off += by;
                }
            }
        }
        self.record_keeping_selection(before);
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
        self.inside = Inside::No;
        self.record(before);
    }
}
