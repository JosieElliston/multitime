//! Selecting by meaning: Shift+Option+arrows. Notation is a tree of atoms
//! (moves, pauses, comments, multipliers) and groups, and a semantic
//! selection is a run of siblings in it. Brackets and separators aren't
//! atoms: they're how a group is written. A group's children are its sides,
//! and each side is a sequence of atoms and groups. A multiplier is an atom
//! after its group.
//!
//! The tree spans a whole path: each segment's top level, one after another.
//! Groups don't span segments, since each segment parses on its own.
//!
//! There's no history: everything comes from a selection's two ends. See
//! `Model::join`, `Model::grow` and `Model::shrink`.

use crate::notation::{self, TokenKind};
use std::ops::Range;

/// A position on a path: which segment along it, and an offset in its text.
pub type Pt = (usize, usize);

/// A list of siblings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// The path's top level.
    Top,
    /// A side of a group: the group's node, and which side.
    Side(usize, usize),
    /// The sides of a group.
    Sides(usize),
}

/// The siblings `lo..hi` of a container, or the gap at `lo` if it's empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sel {
    pub c: Container,
    pub lo: usize,
    pub hi: usize,
}

#[derive(Debug)]
struct Node {
    seg: usize,
    span: Range<usize>,
    /// The lines that it starts and ends on.
    lines: (usize, usize),
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    /// Whether it's part of a run of moves: a move, pause, comment or error.
    Atom { plain: bool },
    Group {
        /// Between its brackets. An unclosed group's runs to the end.
        inner: Range<usize>,
        sides: Vec<Side>,
    },
}

#[derive(Debug)]
struct Side {
    /// Between its delimiters: brackets or separators.
    region: Range<usize>,
    items: Vec<usize>,
}

pub struct Model {
    texts: Vec<String>,
    nodes: Vec<Node>,
    top: Vec<usize>,
    /// The container that each node is in, and where.
    parent: Vec<(Container, usize)>,
}

impl Model {
    pub fn new(texts: &[&str]) -> Self {
        let mut m = Self {
            texts: texts.iter().map(|t| (*t).to_owned()).collect(),
            nodes: vec![],
            top: vec![],
            parent: vec![],
        };
        for (seg, text) in texts.iter().enumerate() {
            let tokens = notation::parse(text).tokens;
            let mut i = 0;
            let items = m.sequence(seg, text, &tokens, &mut i);
            debug_assert_eq!(i, tokens.len(), "only a group's end stops a sequence");
            m.top.extend(items);
        }
        for (k, &n) in m.top.iter().enumerate() {
            m.parent[n] = (Container::Top, k);
        }
        m
    }

    /// Nodes up to a separator or a closing bracket of an enclosing group.
    fn sequence(
        &mut self,
        seg: usize,
        text: &str,
        tokens: &[notation::Token],
        i: &mut usize,
    ) -> Vec<usize> {
        let line = |off: usize| text[..off].matches('\n').count();
        let mut out = vec![];
        while let Some(t) = tokens.get(*i) {
            let s = &text[t.span.clone()];
            let closes = s.starts_with([')', ']']) && t.kind != TokenKind::Stray;
            if t.kind == TokenKind::Sep || closes {
                break;
            }
            *i += 1;
            let node = self.nodes.len();
            // An opener, or a nested NISS one, which is an error.
            let opens = notation::opener(s).is_some();
            let (span, kind) = if opens {
                self.nodes.push(Node {
                    seg,
                    span: t.span.clone(),
                    lines: (0, 0),
                    kind: Kind::Atom { plain: false },
                });
                self.parent.push((Container::Top, 0));
                let mut sides = vec![];
                let mut start = t.span.end;
                let end = loop {
                    let items = self.sequence(seg, text, tokens, i);
                    let k = sides.len();
                    for (j, &n) in items.iter().enumerate() {
                        self.parent[n] = (Container::Side(node, k), j);
                    }
                    match tokens.get(*i) {
                        Some(d) if d.kind == TokenKind::Sep => {
                            sides.push(Side {
                                region: start..d.span.start,
                                items,
                            });
                            start = d.span.end;
                            *i += 1;
                        }
                        Some(d) => {
                            sides.push(Side {
                                region: start..d.span.start,
                                items,
                            });
                            *i += 1;
                            break Some(d.span.clone());
                        }
                        None => {
                            sides.push(Side {
                                region: start..text.len(),
                                items,
                            });
                            break None;
                        }
                    }
                };
                let inner = t.span.end..end.as_ref().map_or(text.len(), |e| e.start);
                let last = tokens[..*i].last().map_or(t.span.end, |l| l.span.end);
                let span_end = end.map_or(last, |e| e.end);
                (t.span.start..span_end, Kind::Group { inner, sides })
            } else {
                let plain = matches!(
                    t.kind,
                    TokenKind::Move | TokenKind::Pause | TokenKind::Comment | TokenKind::Error
                );
                self.nodes.push(Node {
                    seg,
                    span: 0..0,
                    lines: (0, 0),
                    kind: Kind::Atom { plain },
                });
                self.parent.push((Container::Top, 0));
                (t.span.clone(), Kind::Atom { plain })
            };
            self.nodes[node].lines = (line(span.start), line(span.end));
            self.nodes[node].span = span;
            self.nodes[node].kind = kind;
            out.push(node);
        }
        out
    }

    fn items(&self, c: Container) -> &[usize] {
        match c {
            Container::Top => &self.top,
            Container::Side(g, k) => &self.sides(g)[k].items,
            Container::Sides(_) => unreachable!("sides aren't nodes"),
        }
    }
    fn sides(&self, g: usize) -> &[Side] {
        match &self.nodes[g].kind {
            Kind::Group { sides, .. } => sides,
            Kind::Atom { .. } => unreachable!("not a group"),
        }
    }
    fn len(&self, c: Container) -> usize {
        match c {
            Container::Sides(g) => self.sides(g).len(),
            c => self.items(c).len(),
        }
    }
    fn node_span(&self, n: usize) -> (Pt, Pt) {
        let node = &self.nodes[n];
        ((node.seg, node.span.start), (node.seg, node.span.end))
    }
    /// The ends of the `i`th item.
    fn item_span(&self, c: Container, i: usize) -> (Pt, Pt) {
        match c {
            Container::Sides(g) => {
                let side = &self.sides(g)[i];
                let seg = self.nodes[g].seg;
                match (side.items.first(), side.items.last()) {
                    (Some(&a), Some(&b)) => (self.node_span(a).0, self.node_span(b).1),
                    _ => ((seg, side.region.start), (seg, side.region.start)),
                }
            }
            c => self.node_span(self.items(c)[i]),
        }
    }
    /// Where the caret goes for the gap at `i`: after the item before it, or
    /// at the start of the container.
    fn gap(&self, c: Container, i: usize) -> Pt {
        if i > 0 {
            return self.item_span(c, i - 1).1;
        }
        match c {
            Container::Top => (0, 0),
            Container::Side(g, k) => (self.nodes[g].seg, self.sides(g)[k].region.start),
            Container::Sides(g) => (self.nodes[g].seg, self.sides(g)[0].region.start),
        }
    }
    /// The ends of a selection. An empty one's are both at its gap.
    pub fn span(&self, s: Sel) -> (Pt, Pt) {
        if s.lo == s.hi {
            let p = self.gap(s.c, s.lo);
            (p, p)
        } else {
            (self.item_span(s.c, s.lo).0, self.item_span(s.c, s.hi - 1).1)
        }
    }

    /// A range without the whitespace and segment ends at its edges.
    pub fn trim(&self, mut a: Pt, mut b: Pt) -> (Pt, Pt) {
        let at = |p: Pt| self.texts[p.0][p.1..].chars().next();
        let before = |p: Pt| self.texts[p.0][..p.1].chars().next_back();
        while a < b {
            match at(a) {
                None => a = (a.0 + 1, 0),
                Some(c) if c.is_whitespace() => a.1 += c.len_utf8(),
                Some(_) => break,
            }
        }
        while a < b {
            match before(b) {
                None => b = (b.0 - 1, self.texts[b.0 - 1].len()),
                Some(c) if c.is_whitespace() => b.1 -= c.len_utf8(),
                Some(_) => break,
            }
        }
        (a, b.max(a))
    }

    /// The smallest semantic selection that covers a range, which should be
    /// trimmed. An empty range partway through an atom is that atom, and one
    /// partway through a bracket is its group.
    pub fn join(&self, a: Pt, b: Pt) -> Sel {
        let mut c = Container::Top;
        loop {
            let n = self.len(c);
            let overlaps = |i: usize| {
                let (s, e) = self.item_span(c, i);
                if a == b {
                    s < a && a < e
                } else {
                    s < b && a < e
                }
            };
            let over: Vec<usize> = (0..n).filter(|&i| overlaps(i)).collect();
            match over[..] {
                [] => {
                    let k = (0..n).filter(|&i| self.item_span(c, i).1 <= a).count();
                    return Sel { c, lo: k, hi: k };
                }
                [i] if let Some(g) = self.group_at(c, i)
                    && let Kind::Group { inner, sides } = &self.nodes[g].kind
                    && a.0 == self.nodes[g].seg
                    && b.0 == a.0
                    && inner.start <= a.1
                    && b.1 <= inner.end =>
                {
                    let side = |p: usize| {
                        sides
                            .iter()
                            .position(|s| s.region.start <= p && p <= s.region.end)
                            .unwrap_or(0)
                    };
                    let (sa, sb) = (side(a.1), side(b.1));
                    if sa == sb {
                        c = Container::Side(g, sa);
                    } else {
                        return Sel {
                            c: Container::Sides(g),
                            lo: sa,
                            hi: sb + 1,
                        };
                    }
                }
                _ => {
                    return Sel {
                        c,
                        lo: over[0],
                        hi: over[over.len() - 1] + 1,
                    };
                }
            }
        }
    }

    /// The group that's the `i`th item, if it is one.
    fn group_at(&self, c: Container, i: usize) -> Option<usize> {
        if let Container::Sides(_) = c {
            return None;
        }
        let n = self.items(c)[i];
        matches!(self.nodes[n].kind, Kind::Group { .. }).then_some(n)
    }
    /// Whether the `i`th and `j`th items are in one run of moves: both plain
    /// atoms, on one line.
    fn joined(&self, c: Container, i: usize, j: usize) -> bool {
        if let Container::Sides(_) = c {
            return false;
        }
        let (a, b) = (&self.nodes[self.items(c)[i]], &self.nodes[self.items(c)[j]]);
        let plain = |n: &Node| matches!(n.kind, Kind::Atom { plain: true });
        plain(a) && plain(b) && a.seg == b.seg && a.lines.1 == b.lines.0
    }

    /// Adds the next sibling on one side. With none, takes all of the
    /// siblings, and with all of them, becomes their parent. Skips steps that
    /// don't change the selected text.
    /// With `runs`, a run of moves on one line is one sibling, and a selection
    /// that starts partway through one goes to its end.
    pub fn grow(&self, mut s: Sel, forward: bool, runs: bool) -> Sel {
        let before = self.span(s);
        while let Some(next) = self.grow_once(s, forward, runs) {
            s = next;
            if self.span(s) != before {
                break;
            }
        }
        s
    }
    fn grow_once(&self, s: Sel, forward: bool, runs: bool) -> Option<Sel> {
        let n = self.len(s.c);
        if forward && s.hi < n {
            let mut hi = s.hi + 1;
            while runs && hi < n && self.joined(s.c, hi - 1, hi) {
                hi += 1;
            }
            return Some(Sel { hi, ..s });
        }
        if !forward && s.lo > 0 {
            let mut lo = s.lo - 1;
            while runs && lo > 0 && self.joined(s.c, lo - 1, lo) {
                lo -= 1;
            }
            return Some(Sel { lo, ..s });
        }
        if s.lo > 0 || s.hi < n {
            return Some(Sel { lo: 0, hi: n, ..s });
        }
        match s.c {
            Container::Top => None,
            Container::Side(g, _) => Some(Sel {
                c: Container::Sides(g),
                lo: 0,
                hi: self.sides(g).len(),
            }),
            Container::Sides(g) => {
                let (c, i) = self.parent[g];
                Some(Sel {
                    c,
                    lo: i,
                    hi: i + 1,
                })
            }
        }
    }

    /// Undoes a `grow` toward the head, which is at the end if `head_forward`:
    /// removes the sibling there, or if it's the only one and a group, goes
    /// into it. Undoes it exactly unless that grow took all of the siblings
    /// from partway through them: then this removes one from the head's end.
    pub fn shrink(&self, mut s: Sel, head_forward: bool, runs: bool) -> Sel {
        // One side is the same as its items.
        if let Container::Sides(g) = s.c
            && s.hi - s.lo == 1
        {
            let c = Container::Side(g, s.lo);
            s = Sel {
                c,
                lo: 0,
                hi: self.len(c),
            };
        }
        if s.hi - s.lo == 1
            && let Some(g) = self.group_at(s.c, s.lo)
        {
            let n = self.sides(g).len();
            let c = if n == 1 {
                Container::Side(g, 0)
            } else {
                Container::Sides(g)
            };
            return Sel {
                c,
                lo: 0,
                hi: self.len(c),
            };
        }
        if s.lo == s.hi {
            return s;
        }
        if head_forward {
            s.hi -= 1;
            while runs && s.hi > s.lo && self.joined(s.c, s.hi - 1, s.hi) {
                s.hi -= 1;
            }
        } else {
            s.lo += 1;
            while runs && s.lo < s.hi && self.joined(s.c, s.lo - 1, s.lo) {
                s.lo += 1;
            }
        }
        s
    }
}
