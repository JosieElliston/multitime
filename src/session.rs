//! Document plus cursor, undo history, and views. No UI toolkit.

use crate::doc::{Doc, Pos, SegId};
use crate::notation::{self, Inversion, Parsed, Unit};
use crate::puzzle::Perm;
use crate::select;
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
/// them written as a bubble on a line of its own: a `MARK` for each branch,
/// with line breaks that only separate it from other lines: an `OPEN` if
/// there's any text before it, and an `END` if its branch has text. So an
/// empty segment has no line. Neither line break is part of a segment.
///
/// The caret can be on a bubble's line before its first `>`, or right after
/// one, which is that branch's start, the same as the start of its first line
/// but for how the caret moves. That's where the timeline's branch starts are.
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
    /// Right after the segment's `>`. An empty segment's start is always
    /// there, since it has no line.
    Mark,
}

/// A stretch of the text box's text. A bubble's is the one before its segment.
struct Span {
    seg: SegId,
    /// The end of the parent.
    bubble: usize,
    /// The start of the bubble's line, before its first `>`.
    line: usize,
    /// The start of the segment's text.
    start: usize,
    end: usize,
}

/// Where a bubble is in the text box's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bubble {
    /// From the end of the parent to the start of the active branch's text.
    pub range: Range<usize>,
    /// The start of its line, before its first `>`.
    pub line: usize,
    /// How many branches it has.
    pub n: usize,
    /// The active branch, and its index.
    pub seg: SegId,
    pub active: usize,
}

impl Bubble {
    /// The caret positions on its line.
    pub fn marks(&self) -> std::ops::RangeInclusive<usize> {
        self.line..=self.line + self.n
    }
    /// Where the caret is after the active branch's `>`.
    pub fn active_mark(&self) -> usize {
        self.line + 1 + self.active
    }
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
    /// The anchor and caret of a selection that a double or triple click
    /// made. While that's still the selection, it has no direction, so
    /// either Shift+Option+arrow grows it.
    clicked: Option<(Pos, Pos)>,
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
            clicked: None,
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
    fn at_start(&self) -> bool {
        self.inside == Inside::Start
    }
    /// Whether a spot is right after a `>`.
    fn on_mark(&self, (pos, inside): Spot) -> bool {
        let seg = self.doc.get(pos.seg);
        inside == Inside::Mark
            || (inside == Inside::No && pos.off == 0 && seg.parent.is_some() && seg.text.is_empty())
    }
    /// Whether the caret is right after a `>`.
    fn at_mark(&self) -> bool {
        self.on_mark((self.caret, self.inside))
    }
    /// Whether the caret is on a bubble's line.
    fn in_bubble(&self) -> bool {
        self.at_start() || self.at_mark()
    }
    /// Whether a spot is on a bubble's line: whose parent, if so.
    fn bubble_of(&self, spot: Spot) -> Option<SegId> {
        let parent = self.doc.get(spot.0.seg).parent?;
        (spot.1 == Inside::Start || self.on_mark(spot)).then_some(parent)
    }
    /// Whether the selection is of `>`s in a bubble.
    fn bubble_selection(&self) -> bool {
        self.anchor.is_some_and(|a| {
            let b = self.bubble_of(a);
            b.is_some() && b == self.bubble_of((self.caret, self.inside))
        })
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
    /// Puts the caret right after a branch's `>`.
    pub fn enter(&mut self, seg: SegId) {
        self.go_to(Pos { seg, off: 0 });
        if self.doc.get(seg).parent.is_some() {
            self.inside = Inside::Mark;
        }
    }
    /// Moves the caret as the timeline does, where a branch's start is right
    /// after its `>`.
    pub fn go_to_on_timeline(&mut self, pos: Pos) {
        if pos.off == 0 {
            self.enter(pos.seg);
        } else {
            self.go_to(pos);
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
        self.clicked = None;
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
    /// second. A selection that starts on a bubble's line stays on it. One
    /// from outside goes all the way over a bubble or not at all: past it if
    /// `after`, or else to the nearer side.
    pub fn select_offsets(&mut self, anchor: usize, caret: usize, after: Option<bool>) {
        let a = if anchor == self.anchor_offset() {
            self.anchor_spot()
        } else {
            self.at_offset(anchor)
        };
        let bubbles = self.bubbles();
        let spot = if let Some(parent) = self.bubble_of(a) {
            let b = bubbles
                .iter()
                .find(|b| self.doc.get(b.seg).parent == Some(parent))
                .expect("on path");
            let m = b.marks();
            self.at_offset(caret.clamp(*m.start(), *m.end()))
        } else if let Some(b) = bubbles.iter().find(|b| b.marks().contains(&caret)) {
            let r = &b.range;
            if after.unwrap_or(caret - r.start > r.end - caret) {
                (Pos { seg: b.seg, off: 0 }, Inside::No)
            } else {
                let parent = self.doc.get(b.seg).parent.expect("bubbles have parents");
                (self.doc.end(parent), Inside::No)
            }
        } else {
            self.at_offset(caret)
        };
        self.select_spots(a, spot);
    }
    fn select_spots(&mut self, anchor: Spot, caret: Spot) {
        self.place(caret);
        if (self.caret, self.inside) != anchor {
            self.anchor = Some(anchor);
        }
    }
    /// Selects all marks on a bubble's line, keeping its active branch.
    pub fn select_bubble(&mut self, branch: SegId) {
        let parent = self.doc.get(branch).parent.expect("bubbles have parents");
        let start = self.spot_at_gap(parent, 0);
        let end = self.spot_at_gap(parent, self.doc.get(parent).children.len());
        self.select_spots(end, start);
    }
    /// Where a key that moved the text box's caret from `old` to `new` puts
    /// it instead: Up or Down (`vertical`) onto a bubble's line go to after
    /// its active `>`, since anywhere else would switch branches.
    pub fn key_offset(&self, new: usize, vertical: bool) -> usize {
        self.bubbles()
            .into_iter()
            .find(|b| vertical && b.marks().contains(&new))
            .map_or(new, |b| b.active_mark())
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
    /// The same for the timeline's cell for the token at `pos`. The timeline
    /// goes by tokens, so a unit there ends after a whole token.
    pub fn unit_on_timeline(&self, pos: Pos, unit: Unit) -> Option<(Pos, Pos)> {
        let (start, mut end) = self.unit_at(pos, unit)?;
        let text = &self.doc.get(end.seg).text;
        let (gap, partway) = notation::gap(text, end.off);
        if partway {
            end.off = notation::parse(text).tokens[gap].span.end;
        }
        Some((start, end))
    }
    /// The same for the character at an offset in the text box. A bubble's
    /// unit is the bubble, keeping its active branch.
    pub fn unit_at_offset(&self, off: usize, unit: Unit) -> Option<(Pos, Pos)> {
        if let Some(b) = self.bubbles().into_iter().find(|b| b.range.contains(&off)) {
            let parent = self.doc.get(b.seg).parent.expect("bubbles have parents");
            return Some((self.doc.end(parent), Pos { seg: b.seg, off: 0 }));
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
        self.clicked = (anchor == target).then_some((self.anchor(), self.caret));
    }

    /// Shift+Option+arrows: grows or shrinks the selection by meaning. See
    /// `select`. A selection that isn't semantic snaps to the smallest one
    /// that covers it. Otherwise, toward the caret's end (or either way with
    /// no selection, or one that a click made), it grows, and toward the
    /// anchor's, it shrinks. With `runs`, as in twist mode, a run of moves on
    /// one line counts as one. In a bubble, it's Shift+Cmd+arrow's
    /// `step_line`. In twist mode, right after a `>` is the branch's start,
    /// not in the bubble.
    pub fn select_by_meaning(&mut self, forward: bool, runs: bool) {
        let in_bubble = if runs {
            self.at_start()
        } else {
            self.in_bubble()
        };
        if in_bubble || self.bubble_selection() {
            self.step_line(forward, true);
            return;
        }
        let path = self.doc.path(self.tip);
        let texts: Vec<&str> = path
            .iter()
            .map(|s| self.doc.get(*s).text.as_str())
            .collect();
        let model = select::Model::new(&texts);
        let pt = |p: Pos| {
            (
                path.iter().position(|s| *s == p.seg).expect("on path"),
                p.off,
            )
        };
        let (a, c) = (pt(self.anchor()), pt(self.caret));
        let head_forward = c >= a;
        let (lo, hi) = model.trim(a.min(c), a.max(c));
        let sel = model.join(lo, hi);
        let semantic = if lo == hi {
            sel.lo == sel.hi
        } else {
            model.span(sel) == (lo, hi)
        };
        let oriented = a != c && self.clicked != Some((self.anchor(), self.caret));
        let (new, head) = if !semantic {
            (sel, forward)
        } else if !oriented || forward == head_forward {
            (model.grow(sel, forward, runs), forward)
        } else {
            (model.shrink(sel, head_forward, runs), head_forward)
        };
        let (start, end) = model.span(new);
        let pos = |(i, off): select::Pt| Pos { seg: path[i], off };
        if start == end {
            self.go_to(pos(start));
        } else if head {
            self.set_selection(pos(start), pos(end));
        } else {
            self.set_selection(pos(end), pos(start));
        }
        self.clicked = None;
    }
    /// Shift+arrow in text on a bubble's line, at its end that way: selects
    /// the whole bubble, as Shift+Cmd+arrow does. Returns whether it did.
    pub fn select_bubble_at_edge(&mut self, forward: bool) -> bool {
        let caret = (self.caret, self.inside);
        let Some(parent) = self.bubble_of(caret) else {
            return false;
        };
        if self.anchor.is_some() && !self.bubble_selection() {
            return false;
        }
        let n = self.doc.get(parent).children.len();
        if self.gap_of(caret) != if forward { n } else { 0 } {
            return false;
        }
        self.step_line(forward, true);
        true
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
        if self.at_start() && forward {
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
    /// The same, as the timeline does, where a branch's start is right after
    /// its `>`.
    fn move_on_timeline(&mut self, pos: Pos, extend: bool) {
        if extend {
            self.extend_to(pos);
        } else {
            self.go_to_on_timeline(pos);
        }
    }
    pub fn step(&mut self, forward: bool, extend: bool) {
        self.move_on_timeline(self.stepped(forward), extend);
    }
    /// Moves the caret like Option+arrows in text: to the start of the token
    /// before it, or the end of the one after it. A fork counts as a token, and
    /// passing it keeps the branch.
    pub fn step_word(&mut self, forward: bool, extend: bool) {
        let empty = self.doc.get(self.caret.seg).text.is_empty();
        if self.at_start() || (self.inside == Inside::Mark && !empty) {
            self.step_bubble(forward);
            return;
        }
        let path = self.doc.path(self.tip);
        let Pos { seg, off } = self.caret;
        let i = path.iter().position(|s| *s == seg).expect("caret on path");
        let tokens = notation::parse(&self.doc.get(seg).text).tokens;
        let token = if forward {
            tokens.iter().find(|t| t.span.end > off)
        } else {
            tokens.iter().rev().find(|t| t.span.start < off)
        };
        if let Some(t) = token {
            let start = Pos {
                seg,
                off: t.span.start,
            };
            let end = Pos {
                seg,
                off: t.span.end,
            };
            let (opposite, to) = if forward { (start, end) } else { (end, start) };
            if extend && self.anchor.is_none() && t.span.start < off && off < t.span.end {
                self.set_selection(opposite, to);
            } else {
                self.move_caret(to, extend);
            }
            return;
        }
        let pos = if forward {
            path.get(i + 1)
                .map_or(self.caret, |&next| Pos { seg: next, off: 0 })
        } else if i > 0 {
            self.doc.end(path[i - 1])
        } else {
            Pos { seg, off: 0 }
        };
        self.move_caret(pos, extend);
    }
    /// Goes to the edge of the current text or bubble line. When already at
    /// that edge, repeating the command includes the whole line in the selection.
    pub fn step_line(&mut self, forward: bool, extend: bool) {
        let caret = (self.caret, self.inside);
        let bubble = self.bubble_of(caret).filter(|_| {
            self.inside != Inside::No || self.anchor.is_none() || self.bubble_selection()
        });
        let (start, end, at_edge) = if let Some(parent) = bubble {
            let n = self.doc.get(parent).children.len();
            let gap = self.gap_of(caret);
            (
                self.spot_at_gap(parent, 0),
                self.spot_at_gap(parent, n),
                gap == if forward { n } else { 0 },
            )
        } else {
            let Pos { seg, off } = self.caret;
            let text = &self.doc.get(seg).text;
            let start = text[..off].rfind('\n').map_or(0, |i| i + 1);
            let end = text[off..].find('\n').map_or(text.len(), |i| off + i);
            let spot = |off| (Pos { seg, off }, Inside::No);
            (
                spot(start),
                spot(end),
                off == if forward { end } else { start },
            )
        };
        let (opposite, to) = if forward { (start, end) } else { (end, start) };
        if let Some(anchor) = self.anchor {
            let a = self.anchor_offset();
            let lo = self.spot_offset(start).expect("line start on path");
            let hi = self.spot_offset(end).expect("line end on path");
            if a < lo || a > hi {
                // Keep the anchor on the other line. At the edge, include
                // this whole line by moving the caret away from the anchor.
                let to = if !at_edge {
                    to
                } else if a < lo {
                    end
                } else {
                    start
                };
                self.select_spots(anchor, to);
                return;
            }
        }
        if at_edge
            && (extend
                || self.anchor.is_none()
                || self.spot_offset(opposite) == Some(self.anchor_offset()))
        {
            self.select_spots(opposite, to);
        } else if extend {
            self.select_spots(self.anchor_spot(), to);
        } else {
            self.place(to);
        }
    }
    /// Like `step`, but past a whole run: the tokens up to a bracket,
    /// separator or line break.
    pub fn step_run(&mut self, forward: bool, extend: bool) {
        self.move_on_timeline(self.run_stop(forward), extend);
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
        if self.at_start() {
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
        self.move_on_timeline(self.segment_stop(forward), extend);
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
            if self.at_start() || off < end(seg).off {
                end(seg)
            } else {
                path.get(i + 1).map_or(self.caret, |&next| end(next))
            }
        } else if off > 0 && !self.at_start() {
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
        self.move_on_timeline(pos, extend);
    }

    /// Moves the caret between the sides of a bubble and its active branch:
    /// out of it to the start of the branch's line or the end of the parent's,
    /// and into it after the active `>`. Returns whether there's a bubble
    /// there.
    pub fn step_bubble(&mut self, forward: bool) -> bool {
        let Pos { seg, off } = self.caret;
        let path = self.doc.path(self.tip);
        let i = path.iter().position(|s| *s == seg).expect("caret on path");
        if self.in_bubble() && !(forward && self.doc.get(seg).text.is_empty()) {
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
        if self.anchor.is_some() || self.at_start() || to.seg != seg {
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

    /// Deletes the selection, or else one token. Right after a `>`, going
    /// backwards, that's the `>` and its branch, and at either side of a
    /// bubble, the bubble.
    pub fn delete(&mut self, forward: bool) {
        let before = self.snapshot();
        let (a, c) = (self.anchor_offset(), self.caret_offset());
        let Pos { seg, off } = self.caret;
        let path = self.doc.path(self.tip);
        let i = path.iter().position(|s| *s == seg).expect("caret on path");
        if a != c {
            self.remove(a.min(c), a.max(c), true);
        } else if self.at_start() {
            if !forward && c > 0 {
                self.remove(c - 1, c, false);
            }
        } else if self.at_mark() && !forward {
            self.remove(c - 1, c, false);
        } else if let Some((new, off)) = notation::delete(&self.doc.get(seg).text, off, forward) {
            self.doc.set_text(seg, new);
            self.caret.off = off;
        } else if forward {
            if let Some(&next) = path.get(i + 1) {
                self.join_into_parent(next);
            }
        } else {
            self.join_into_parent(seg);
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
                at += usize::from(at > 0);
            }
            let line = at;
            if s.parent.is_some() {
                at += self.doc.siblings(seg).len() + usize::from(!s.text.is_empty());
            }
            let start = at;
            at += s.text.len();
            Span {
                seg,
                bubble,
                line,
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
                if s.line > s.bubble {
                    out.push(OPEN);
                }
                let n = self.doc.siblings(s.seg).len();
                out.extend(std::iter::repeat_n(MARK, n));
                if s.start > s.line + n {
                    out.push(END);
                }
            }
            out.push_str(&self.doc.get(s.seg).text);
        }
        out
    }
    /// Offset in the text, if it's on the path, or the start of a branch of a
    /// bubble on it. A branch's start is after its `>`.
    pub fn offset_of(&self, pos: Pos) -> Option<usize> {
        self.spot_offset((pos, Inside::No))
    }
    /// Offset of a spot in the text, if it's there.
    fn spot_offset(&self, (pos, inside): Spot) -> Option<usize> {
        let spans = self.spans();
        if inside == Inside::Start || self.on_mark((pos, inside)) {
            let sibs = self.doc.siblings(pos.seg);
            let s = spans.iter().find(|s| sibs.contains(&s.seg))?;
            return Some(match inside {
                Inside::Start => s.line,
                _ => s.line + 1 + sibs.iter().position(|c| *c == pos.seg)?,
            });
        }
        let s = spans.iter().find(|s| s.seg == pos.seg)?;
        Some(s.start + pos.off)
    }
    pub fn caret_offset(&self) -> usize {
        self.spot_offset((self.caret, self.inside))
            .expect("on path")
    }
    /// The position at an offset in the text, and where it is on a bubble's
    /// line. Before a bubble's first mark, the position is the active branch's.
    pub fn at_offset(&self, off: usize) -> (Pos, Inside) {
        for b in self.bubbles() {
            if b.marks().contains(&off) {
                return match off - b.line {
                    0 => (Pos { seg: b.seg, off: 0 }, Inside::Start),
                    i => {
                        let seg = self.doc.siblings(b.seg)[i - 1];
                        (Pos { seg, off: 0 }, Inside::Mark)
                    }
                };
            }
        }
        for s in self.spans() {
            if off <= s.end {
                let off = off.saturating_sub(s.start);
                return (Pos { seg: s.seg, off }, Inside::No);
            }
        }
        (self.doc.end(self.tip), Inside::No)
    }
    /// Where each bubble is in the text.
    pub fn bubbles(&self) -> Vec<Bubble> {
        self.spans()[1..]
            .iter()
            .map(|s| {
                let sibs = self.doc.siblings(s.seg);
                Bubble {
                    range: s.bubble..s.start,
                    line: s.line,
                    n: sibs.len(),
                    seg: s.seg,
                    active: sibs.iter().position(|c| *c == s.seg).expect("sibling"),
                }
            })
            .collect()
    }

    /// Deletes a range of the text. Within a bubble's marks, that's their
    /// branches, but a lone `>` that isn't `selected` keeps its text. A bubble's
    /// line break, from outside of it, deletes it the same way, and from
    /// inside, nothing, but before its first `>`, a blank line before it.
    fn remove(&mut self, mut a: usize, mut b: usize, selected: bool) {
        let (at_start, inside) = (self.at_start(), self.in_bubble());
        for bubble in self.bubbles() {
            let (r, seg) = (bubble.range.clone(), bubble.seg);
            let touches = a < r.end && r.start < b;
            if !touches || (a <= r.start && r.end <= b) {
                continue;
            }
            let marks = bubble.line..bubble.line + bubble.n;
            if marks.start <= a && b <= marks.end {
                let sibs = self.doc.siblings(seg).to_vec();
                let range = a - marks.start..b - marks.start;
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
            if b - a == 1 && !selected {
                let parent = self.doc.get(seg).parent.expect("in a bubble");
                if a == r.start && r.start < bubble.line {
                    if !at_start {
                        self.join_into_parent(seg);
                    } else if let Some(t) = self.doc.get(parent).text.strip_suffix('\n') {
                        self.doc.set_text(parent, t.to_owned());
                    }
                    return;
                }
                if a == marks.end && b == r.end {
                    if !inside {
                        self.join_into_parent(seg);
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
        self.inside = Inside::No;
    }
    /// Types a character at the caret. A `>` outside of a comment makes a
    /// bubble, or at a branch's start, a branch after it. Anything else before
    /// a bubble's first mark goes on a line of its own before the bubble.
    fn type_char(&mut self, c: char) {
        let Pos { seg, off } = self.caret;
        let parent = self.doc.get(seg).parent;
        let at_start = self.at_start();
        if at_start && c != '>' {
            let p = parent.expect("in a bubble");
            let text = &self.doc.get(p).text;
            if c == '\n' || !text.is_empty() {
                self.doc.set_text(p, format!("{text}\n"));
            }
            if c != '\n' {
                self.go_to(self.doc.end(p));
                self.type_char(c);
            }
            return;
        }
        let sibling = at_start || self.at_mark();
        let new = match c {
            '>' if at_start => Some(self.doc.insert_child(parent.expect("in a bubble"), 0)),
            '>' if sibling => self.doc.add_sibling(seg),
            '>' if !notation::parse(&self.doc.get(seg).text[..off]).in_comment => {
                Some(self.doc.fork_at(seg, off))
            }
            _ => None,
        };
        if let Some(new) = new {
            if self.tip == seg || sibling {
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
        if self.at_start() {
            let parent = self.doc.get(self.caret.seg).parent.expect("in a bubble");
            let t = &self.doc.get(parent).text;
            if !t.is_empty() {
                self.doc.set_text(parent, format!("{t}\n"));
            }
            self.go_to(self.doc.end(parent));
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
    /// Returns false, doing nothing, if there's no selection of text.
    pub fn wrap(&mut self, open: &str, close: &str) -> bool {
        self.wrap_with(open, None, close)
    }
    /// Wraps the selection in `[ ]` for a commutator or conjugate, with its
    /// separator at the gap between tokens nearest `at`, but not in a group
    /// that the selection cuts. Returns false, doing nothing, if there's no
    /// selection of text.
    pub fn wrap_binary(&mut self, sep: char, at: Pos) -> bool {
        self.wrap_with("[", Some((sep, at)), "]")
    }
    fn wrap_with(&mut self, open: &str, sep: Option<(char, Pos)>, close: &str) -> bool {
        let Some((mut lo, mut hi, backward)) = self.text_selection() else {
            return false;
        };
        let before = self.snapshot();
        let sep = sep.map(|(c, at)| (c, self.separator_pos(at)));
        // From the end, so that earlier offsets still hold.
        self.insert_at(hi, close);
        if let Some((c, pos)) = sep {
            let (text, n) = notation::insert_separator(&self.doc.get(pos.seg).text, pos.off, c);
            self.doc.set_text(pos.seg, text);
            if pos.seg == hi.seg {
                hi.off += n;
            }
        }
        self.insert_at(lo, open);
        lo.off += open.len();
        if hi.seg == lo.seg {
            hi.off += open.len();
        }
        self.record(before);
        self.reselect(lo, hi, backward);
        true
    }
    fn insert_at(&mut self, pos: Pos, s: &str) {
        let mut text = self.doc.get(pos.seg).text.clone();
        text.insert_str(pos.off, s);
        self.doc.set_text(pos.seg, text);
    }
    /// Selects from `lo` to `hi`, with the caret at `lo` if `backward`.
    fn reselect(&mut self, lo: Pos, hi: Pos, backward: bool) {
        if backward {
            self.set_selection(hi, lo);
        } else {
            self.set_selection(lo, hi);
        }
    }
    /// The selection, if it's of text rather than `>`s: its start, its end, and
    /// whether the caret is at the start.
    fn text_selection(&self) -> Option<(Pos, Pos, bool)> {
        let Some((anchor, Inside::No)) = self.anchor.filter(|_| !self.bubble_selection()) else {
            return None;
        };
        let backward = self.caret_offset() < self.anchor_offset();
        Some(if backward {
            (self.caret, anchor, true)
        } else {
            (anchor, self.caret, false)
        })
    }
    /// The part of each segment that the selection of text has in it, in order.
    fn selected_ranges(&self) -> Vec<(SegId, Range<usize>)> {
        let Some((lo, hi, _)) = self.text_selection() else {
            return vec![];
        };
        let path = self.doc.path(self.tip);
        let index = |seg| path.iter().position(|s| *s == seg).expect("on path");
        path[index(lo.seg)..=index(hi.seg)]
            .iter()
            .map(|&seg| {
                let start = if seg == lo.seg { lo.off } else { 0 };
                let end = if seg == hi.seg {
                    hi.off
                } else {
                    self.doc.get(seg).text.len()
                };
                (seg, start..end)
            })
            .collect()
    }
    /// Where a separator goes in the selection for a position: the nearest
    /// place in it that `notation::separator_offset` allows.
    fn separator_pos(&self, at: Pos) -> Pos {
        let ranges = self.selected_ranges();
        let (seg, range) = match ranges.iter().find(|(s, _)| *s == at.seg) {
            Some(r) => r.clone(),
            // Off the selection: whichever end is nearer along the path.
            None => {
                let path = self.doc.path(self.tip);
                let index = |seg| path.iter().position(|s| *s == seg);
                let (first, last) = (&ranges[0], &ranges[ranges.len() - 1]);
                return match index(at.seg) {
                    Some(i) if Some(i) > index(last.0) => Pos {
                        seg: last.0,
                        off: last.1.end,
                    },
                    _ => Pos {
                        seg: first.0,
                        off: first.1.start,
                    },
                };
            }
        };
        let text = &self.doc.get(seg).text;
        let off =
            notation::separator_offset(text, range.clone(), at.off.clamp(range.start, range.end));
        Pos { seg, off }
    }
    /// Whether there's a selection of text, rather than of `>`s.
    pub fn selects_text(&self) -> bool {
        self.text_selection().is_some()
    }
    /// Whether an offset in the text is in the selection, or at either end.
    pub fn selection_contains(&self, off: usize) -> bool {
        let (a, c) = (self.anchor_offset(), self.caret_offset());
        a != c && a.min(c) <= off && off <= a.max(c)
    }
    /// Whether `expand` would do anything.
    pub fn can_expand(&self) -> bool {
        self.selected_ranges()
            .into_iter()
            .any(|(seg, r)| notation::expand(&self.doc.get(seg).text, r).is_some())
    }
    /// Writes out each commutator and conjugate that's all in the selection.
    pub fn expand(&mut self) {
        let Some((lo, mut hi, backward)) = self.text_selection() else {
            return;
        };
        let before = self.snapshot();
        for (seg, r) in self.selected_ranges() {
            if let Some((text, new)) = notation::expand(&self.doc.get(seg).text, r) {
                self.doc.set_text(seg, text);
                if seg == hi.seg {
                    hi.off = new.end;
                }
            }
        }
        self.record(before);
        self.reselect(lo, hi, backward);
    }
    /// Whether `invert` would do anything: the selection is in one segment,
    /// and has every group whose brackets it has.
    pub fn can_invert(&self) -> bool {
        match self.selected_ranges().as_slice() {
            [(seg, r)] => notation::invert(&self.doc.get(*seg).text, r.clone()).is_some(),
            _ => false,
        }
    }
    /// Replaces the selection with moves that undo it.
    pub fn invert(&mut self) {
        let Some((lo, mut hi, backward)) = self.text_selection() else {
            return;
        };
        if lo.seg != hi.seg {
            return;
        }
        let Some((text, new)) = notation::invert(&self.doc.get(lo.seg).text, lo.off..hi.off) else {
            return;
        };
        let before = self.snapshot();
        self.doc.set_text(lo.seg, text);
        hi.off = new.end;
        self.record(before);
        self.reselect(lo, hi, backward);
    }

    /// Twists at the caret. Before a bubble's first `>`, that's the end of
    /// the parent.
    pub fn twist(&mut self, mv: &str, inversion: Inversion) {
        let before = self.snapshot();
        if self.at_start() {
            let parent = self.doc.get(self.caret.seg).parent.expect("in a bubble");
            self.go_to(self.doc.end(parent));
        }
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
        let selected = self.anchor.is_some();
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
            self.place(self.at_offset(prefix));
        }
        self.anchor = None;
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

    /// Types a `>`, even in a comment or in twist mode: splits the text at the
    /// caret with a bubble, or at a branch's start, adds a branch after it.
    /// Right before a bubble (with only whitespace between), it adds a first
    /// branch, and right after one, a branch after the caret's.
    pub fn fork(&mut self) {
        let before = self.snapshot();
        if self.at_start() || self.at_mark() {
            self.type_char('>');
        } else if let Some((parent, index)) = self.beside_bubble() {
            let new = self.doc.insert_child(parent, index);
            self.show(new);
            self.enter(new);
        } else {
            let new = self.split_at_gap();
            self.enter(new);
        }
        self.record(before);
    }
    /// Right beside a bubble, as tokens go, the bubble and the index on that
    /// side of it: after the caret's branch, or first.
    fn beside_bubble(&self) -> Option<(SegId, usize)> {
        if self.at_start() {
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
            let sibs = self.doc.siblings(seg);
            Some((parent, 1 + sibs.iter().position(|s| *s == seg)?))
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
    /// branch, the same as Delete at the end of the line before it.
    pub fn join(&mut self) {
        let before = self.snapshot();
        self.join_into_parent(self.caret.seg);
        self.record(before);
    }
    /// Deletes a bubble's line. If the parent and the branch both have text,
    /// that leaves the line break before it between them.
    fn join_into_parent(&mut self, seg: SegId) {
        let Some(parent) = self.doc.get(seg).parent else {
            return;
        };
        let text = &self.doc.get(parent).text;
        if !text.is_empty() && !self.doc.get(seg).text.is_empty() {
            self.doc.set_text(parent, format!("{text}\n"));
        }
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

    /// Which gap between a bubble's marks a spot on its line is in.
    fn gap_of(&self, (pos, inside): Spot) -> usize {
        match inside {
            Inside::No | Inside::Mark => {
                1 + self
                    .doc
                    .siblings(pos.seg)
                    .iter()
                    .position(|s| *s == pos.seg)
                    .expect("is own sibling")
            }
            Inside::Start => 0,
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
    pub fn selected_branches(&self) -> Option<(SegId, Range<usize>)> {
        if !self.bubble_selection() {
            return None;
        }
        let anchor = self.anchor?;
        let parent = self.doc.get(self.caret.seg).parent?;
        let (a, c) = (self.gap_of(anchor), self.gap_of((self.caret, self.inside)));
        Some((parent, a.min(c)..a.max(c)))
    }
    /// The selection as it's copied, or `None` if there isn't one. Selected
    /// `>`s copy their branches whole, written as in a file. Across a bubble,
    /// only its active branch is, after a line with its `>`.
    pub fn copy(&self) -> Option<String> {
        let (a, c) = (self.anchor_offset(), self.caret_offset());
        if a == c {
            return None;
        }
        if let Some((parent, range)) = self.selected_branches() {
            let branches = &self.doc.get(parent).children[range];
            return Some(self.doc.branches_to_file(branches));
        }
        let (lo, hi) = (a.min(c), a.max(c));
        let mut out = String::new();
        for s in self.spans() {
            let text = &self.doc.get(s.seg).text;
            if s.start > s.bubble && lo <= s.bubble && s.start <= hi {
                out.push_str("\n>");
                if !text.is_empty() {
                    out.push('\n');
                }
            }
            let (from, to) = (lo.max(s.start), hi.min(s.end));
            if from < to {
                out.push_str(&text[from - s.start..to - s.start]);
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
        self.anchor = None;
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
        if self.at_start() {
            return (self.doc.get(seg).parent.expect("in a bubble"), 0);
        }
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

    // ---- moving and duplicating lines ----

    /// With the caret at a branch's start, that branch, or else the selected
    /// `>`s: the bubble, and their indices.
    fn branch_block(&self) -> Option<(SegId, Range<usize>)> {
        if self.anchor.is_some() {
            return self.selected_branches();
        }
        if !self.at_mark() {
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
    /// segment: it, and their extent without the last line break.
    fn line_block(&self) -> Option<(SegId, Range<usize>)> {
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
        Some((seg, start..end))
    }
    /// The lines of the text box: `None` for a bubble's, and each segment's
    /// lines, by their index on the path. An empty segment has none.
    fn lines(&self) -> Vec<(usize, Option<String>)> {
        let mut out = vec![];
        for (k, seg) in self.doc.path(self.tip).into_iter().enumerate() {
            if k > 0 {
                out.push((k, None));
            }
            let text = &self.doc.get(seg).text;
            if !text.is_empty() {
                out.extend(text.split('\n').map(|l| (k, Some(l.to_owned()))));
            }
        }
        out
    }
    /// Which line a spot is on, and where on it. An empty branch's start is
    /// at the end of its bubble's line.
    fn locate(&self, lines: &[(usize, Option<String>)], (pos, inside): Spot) -> (usize, usize) {
        let path = self.doc.path(self.tip);
        let k = path.iter().position(|s| *s == pos.seg).expect("on path");
        let first = lines.iter().position(|(j, _)| *j == k);
        let bubble = |at| (first.expect("a bubble's line"), at);
        let text = &self.doc.get(pos.seg).text;
        match inside {
            Inside::Start => bubble(0),
            Inside::Mark => bubble(1),
            _ if text.is_empty() && k > 0 => bubble(1),
            // An empty root has no line, so that's the first bubble's.
            _ if text.is_empty() => (0, 0),
            _ => {
                let line_start = text[..pos.off].rfind('\n').map_or(0, |i| i + 1);
                let first = first.expect("has lines") + usize::from(k > 0);
                (
                    first + text[..pos.off].matches('\n').count(),
                    pos.off - line_start,
                )
            }
        }
    }
    /// The spot on a line, where `lines` are the text's after moving them.
    fn spot_on_line(&self, lines: &[(usize, Option<String>)], (line, at): (usize, usize)) -> Spot {
        let path = self.doc.path(self.tip);
        let k = lines[..=line].iter().filter(|(_, l)| l.is_none()).count();
        let seg = path[k];
        if lines[line].1.is_none() {
            let inside = if at == 0 { Inside::Start } else { Inside::Mark };
            return (Pos { seg, off: 0 }, inside);
        }
        let text = &self.doc.get(seg).text;
        let before = lines[..line]
            .iter()
            .rev()
            .take_while(|(_, l)| l.is_some())
            .count();
        let start: usize = text.split('\n').take(before).map(|l| l.len() + 1).sum();
        (
            Pos {
                seg,
                off: (start + at).min(text.len()),
            },
            Inside::No,
        )
    }
    /// Option+Up or Down: moves the lines that the selection or caret is on
    /// past the next line, which can be a bubble's, so a line goes in or out
    /// of the active branch. At a branch's start, or with `>`s selected, it
    /// moves branches among their siblings instead.
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
        } else {
            self.move_text_lines(down);
        }
        self.record_keeping_selection(before);
    }
    fn move_text_lines(&mut self, down: bool) {
        let mut lines = self.lines();
        let caret = self.locate(&lines, (self.caret, self.inside));
        let anchor = self.anchor.map(|a| self.locate(&lines, a));
        let a = anchor.unwrap_or(caret);
        let (lo, hi) = (a.min(caret), a.max(caret));
        // A selection up to the start of a line doesn't take it in.
        let excluded = hi.1 == 0 && hi.0 > lo.0 && lines[hi.0].1.is_some();
        let block = lo.0..hi.0 + usize::from(!excluded);
        let other = if down {
            block.end
        } else {
            match block.start.checked_sub(1) {
                Some(i) => i,
                None => return,
            }
        };
        let bubbles = |r: Range<usize>| lines[r].iter().any(|(_, l)| l.is_none());
        if other >= lines.len() || (lines[other].1.is_none() && bubbles(block.clone())) {
            return;
        }
        let moved = |line: usize| {
            if block.contains(&line) {
                if down { line + 1 } else { line - 1 }
            } else if line == other {
                if down { block.start } else { block.end - 1 }
            } else {
                line
            }
        };
        if down {
            lines[block.start..=other].rotate_right(1);
        } else {
            lines[other..block.end].rotate_left(1);
        }
        // Each segment's text is the lines up to the next bubble's.
        let path = self.doc.path(self.tip);
        let mut texts = vec![vec![]; path.len()];
        let mut k = 0;
        for (_, l) in &lines {
            match l {
                Some(l) => texts[k].push(l.as_str()),
                None => k += 1,
            }
        }
        for (seg, t) in path.iter().zip(texts) {
            self.doc.set_text(*seg, t.join("\n"));
        }
        // An end at the start of the line after the block stays after it.
        let moved = |(line, at): (usize, usize)| {
            if (line, at) == (block.end, 0) {
                match lines.get(line + 1) {
                    _ if !down => (line - 1, 0),
                    Some(_) => (line + 1, 0),
                    // Or at the end, if it's last.
                    None => (line, lines[line].1.as_ref().map_or(1, String::len)),
                }
            } else {
                (moved(line), at)
            }
        };
        let caret = self.spot_on_line(&lines, moved(caret));
        let anchor = anchor.map(|a| self.spot_on_line(&lines, moved(a)));
        (self.caret, self.inside) = caret;
        self.anchor = anchor;
    }
    /// Shift+Option+Up or Down: duplicates the lines that the selection or
    /// caret is on, leaving it on the upper or lower copy. At a branch's
    /// start, or with `>`s selected, it duplicates branches.
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
        } else if let Some((seg, block)) = self.line_block() {
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
