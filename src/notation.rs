//! Lenient notation parser and evaluator. Any string is playable: an unclosed
//! group is one we are still inside, and errors play nothing.
//!
//! `//` comments run to the end of the line. `^(...)` is NISS. It is only legal
//! at top level; nested NISS is an error and plays nothing.

use crate::puzzle::{self, Perm};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inversion {
    Normal,
    Inverse,
}

impl Inversion {
    pub fn flip(self) -> Self {
        match self {
            Self::Normal => Self::Inverse,
            Self::Inverse => Self::Normal,
        }
    }
}

/// Maps a state to `left * state * right`. Normal moves multiply on the right
/// and NISS moves on the left, so the two kinds commute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Op {
    pub left: Perm,
    pub right: Perm,
}

impl Op {
    pub const ID: Self = Self {
        left: Perm::ID,
        right: Perm::ID,
    };

    pub fn new(perm: Perm, inversion: Inversion) -> Self {
        match inversion {
            Inversion::Normal => Self {
                left: Perm::ID,
                right: perm,
            },
            Inversion::Inverse => Self {
                left: perm.inv(),
                right: Perm::ID,
            },
        }
    }
    pub fn then(&self, next: &Self) -> Self {
        Self {
            left: next.left.then(&self.left),
            right: self.right.then(&next.right),
        }
    }
    pub fn pow(&self, n: i32) -> Self {
        Self {
            left: self.left.pow(n),
            right: self.right.pow(n),
        }
    }
    pub fn apply(&self, state: &Perm) -> Perm {
        self.left.then(state).then(&self.right)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    /// `(`, `!(` or `&(`
    Plain,
    /// `^(`
    Niss,
    /// `[A, B]` or `[A: B]`
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Move,
    Pause,
    Comment,
    Open,
    Sep,
    Close {
        kind: GroupKind,
        open: usize,
        has_multiplier: bool,
    },
    /// Ends a group that is malformed, such as `[R]`. Plays its contents.
    BadClose,
    /// Closing bracket or separator that doesn't belong to any group.
    Stray,
    Error,
}

impl TokenKind {
    pub fn is_error(self) -> bool {
        matches!(self, Self::BadClose | Self::Stray | Self::Error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub span: Range<usize>,
    pub kind: TokenKind,
}

/// Group that was still open at the end of the text.
#[derive(Debug, Clone)]
pub struct OpenGroup {
    pub kind: GroupKind,
    /// Span of the opening token.
    pub span: Range<usize>,
    pub sep: Option<Range<usize>>,
    /// Illegal nested NISS, or inside of one.
    pub dead: bool,
}

#[derive(Debug, Clone)]
pub struct Parsed {
    pub op: Op,
    pub tokens: Vec<Token>,
    pub open: Vec<OpenGroup>,
    /// The text ends inside of a comment.
    pub in_comment: bool,
}

impl Parsed {
    /// Inversion at the end of the text.
    pub fn inversion(&self) -> Inversion {
        match self.open.first() {
            Some(g) if g.kind == GroupKind::Niss => Inversion::Inverse,
            _ => Inversion::Normal,
        }
    }
}

struct Frame {
    group: OpenGroup,
    /// Contents before the separator.
    lhs: Option<(char, Op)>,
    op: Op,
}

impl Frame {
    /// What has been played so far, if the group is never closed.
    fn played(&self) -> Op {
        if self.group.dead {
            return Op::ID;
        }
        let op = match &self.lhs {
            Some((_, a)) => a.then(&self.op),
            None => self.op,
        };
        self.nissed(op)
    }
    fn closed(&self) -> Op {
        if self.group.dead {
            return Op::ID;
        }
        let op = match &self.lhs {
            Some((c, a)) => {
                let out = a.then(&self.op).then(&a.pow(-1));
                if *c == ',' {
                    out.then(&self.op.pow(-1))
                } else {
                    out
                }
            }
            None => self.op,
        };
        self.nissed(op)
    }
    fn nissed(&self, op: Op) -> Op {
        if self.group.kind == GroupKind::Niss {
            // Contents are never NISS, so `left` is the identity.
            Op::new(op.right, Inversion::Inverse)
        } else {
            op
        }
    }
}

fn parse_multiplier(s: &str) -> Option<i32> {
    let (digits, neg) = match s.strip_suffix('\'') {
        Some(d) => (d, true),
        None => (s, false),
    };
    let n: i32 = if digits.is_empty() {
        1
    } else if digits.bytes().all(|b| b.is_ascii_digit()) {
        digits.parse().ok()?
    } else {
        return None;
    };
    Some(if neg { -n } else { n })
}

/// Parses a move or rotation such as `R2'`, `Rw`, `2-3R`, `M`, `x` or `@R`.
pub fn parse_move(s: &str) -> Option<Perm> {
    let (rotation, s) = match s.strip_prefix('@') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let family_at = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (prefix, rest) = s.split_at(family_at);
    let mut family = rest.chars().next()?;
    let mut rest = &rest[1..];
    let mut wide = false;
    let face = match family {
        'r' | 'l' | 'u' | 'd' | 'f' | 'b' => {
            wide = true;
            family = family.to_ascii_uppercase();
            family
        }
        'M' => 'L',
        'E' => 'D',
        'S' => 'F',
        'x' => 'R',
        'y' => 'U',
        'z' => 'F',
        c => c,
    };
    if !wide
        && puzzle::face_normal(family).is_some()
        && let Some(r) = rest.strip_prefix('w')
    {
        wide = true;
        rest = r;
    }
    let mult = parse_multiplier(rest)?;
    let is_face = puzzle::face_normal(family).is_some();
    let layer = |s: &str| -> Option<u32> {
        s.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| s.parse().ok())?
    };
    let (lo, hi) = if rotation {
        if !prefix.is_empty() || wide || !is_face {
            return None;
        }
        (1, 3)
    } else if !is_face {
        if !prefix.is_empty() {
            return None;
        }
        if "xyz".contains(family) {
            (1, 3)
        } else {
            (2, 2)
        }
    } else if prefix.is_empty() {
        (1, if wide { 2 } else { 1 })
    } else if let Some((a, b)) = prefix.split_once('-') {
        (layer(a)?, layer(b)?)
    } else {
        let n = layer(prefix)?;
        (if wide { 1 } else { n }, n)
    };
    Some(puzzle::turn(face, lo, hi)?.pow(mult))
}

pub fn parse(text: &str) -> Parsed {
    let mut tokens: Vec<Token> = vec![];
    let mut top = Op::ID;
    let mut stack: Vec<Frame> = vec![];
    let mut in_comment = false;
    let mut i = 0;

    while i < text.len() {
        let rest = &text[i..];
        let c = rest.chars().next().expect("in bounds");
        let start = i;
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        let kind = if rest.starts_with("//") {
            let len = rest.find('\n').unwrap_or(rest.len());
            i += len;
            in_comment = i == text.len();
            TokenKind::Comment
        } else if let Some((kind, len)) = opener(rest) {
            i += len;
            let in_dead = stack.last().is_some_and(|f| f.group.dead);
            let nested_niss = kind == GroupKind::Niss && !stack.is_empty();
            let dead = in_dead || nested_niss;
            stack.push(Frame {
                group: OpenGroup {
                    kind,
                    span: start..i,
                    sep: None,
                    dead,
                },
                lhs: None,
                op: Op::ID,
            });
            if nested_niss {
                TokenKind::Error
            } else {
                TokenKind::Open
            }
        } else if matches!(c, ')' | ']') {
            i += 1;
            let matches_top = stack
                .last()
                .is_some_and(|f| (f.group.kind == GroupKind::Binary) == (c == ']'));
            if matches_top {
                let frame = stack.pop().expect("checked");
                let mult_len = text[i..]
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(text.len() - i);
                let prime = text[i + mult_len..].starts_with('\'') as usize;
                let mult = &text[i..i + mult_len + prime];
                i += mult.len();
                let no_sep = frame.group.kind == GroupKind::Binary && frame.lhs.is_none();
                let parsed_mult = parse_multiplier(mult);
                let op = frame.closed().pow(parsed_mult.unwrap_or(1));
                let acc = stack.last_mut().map_or(&mut top, |f| &mut f.op);
                *acc = acc.then(&op);
                if no_sep || parsed_mult.is_none() {
                    TokenKind::BadClose
                } else {
                    TokenKind::Close {
                        kind: frame.group.kind,
                        open: frame.group.span.start,
                        has_multiplier: !mult.is_empty(),
                    }
                }
            } else {
                TokenKind::Stray
            }
        } else if matches!(c, ',' | ':') {
            i += 1;
            match stack.last_mut() {
                Some(f) if f.group.kind == GroupKind::Binary && f.lhs.is_none() => {
                    f.lhs = Some((c, std::mem::replace(&mut f.op, Op::ID)));
                    f.group.sep = Some(start..i);
                    TokenKind::Sep
                }
                _ => TokenKind::Stray,
            }
        } else {
            // Word: at least one char, up to whitespace, punctuation, or `//`.
            i += c.len_utf8();
            while let Some(c) = text[i..].chars().next() {
                let r = &text[i..];
                if c.is_whitespace() || "()[],:^!&".contains(c) || r.starts_with("//") {
                    break;
                }
                i += c.len_utf8();
            }
            let word = &text[start..i];
            if word == "." {
                TokenKind::Pause
            } else if let Some(perm) = parse_move(word) {
                let acc = stack.last_mut().map_or(&mut top, |f| &mut f.op);
                *acc = acc.then(&Op::new(perm, Inversion::Normal));
                TokenKind::Move
            } else {
                TokenKind::Error
            }
        };
        tokens.push(Token {
            span: start..i,
            kind,
        });
    }

    let mut op = top;
    // An inner unclosed group plays after its parent's contents so far. Only
    // the outermost can be NISS, so fold from the inside out.
    let mut inner = Op::ID;
    let open = stack.iter().map(|f| f.group.clone()).collect();
    while let Some(mut f) = stack.pop() {
        f.op = f.op.then(&inner);
        inner = f.played();
    }
    op = op.then(&inner);
    Parsed {
        op,
        tokens,
        open,
        in_comment,
    }
}

fn opener(s: &str) -> Option<(GroupKind, usize)> {
    Some(match s.as_bytes() {
        [b'[', ..] => (GroupKind::Binary, 1),
        [b'(', ..] => (GroupKind::Plain, 1),
        [b'!' | b'&', b'(', ..] => (GroupKind::Plain, 2),
        [b'^', b'(', ..] => (GroupKind::Niss, 2),
        _ => return None,
    })
}

/// Space needed between two texts so that their tokens stay separate.
pub fn padding(before: &str, after: &str) -> &'static str {
    let left = before
        .chars()
        .next_back()
        .is_some_and(|c| !c.is_whitespace() && !"([".contains(c));
    let right = after
        .chars()
        .next()
        .is_some_and(|c| !c.is_whitespace() && !")],:".contains(c));
    if left && right { " " } else { "" }
}

/// Inserts `s` at `at`, adding spaces so it stays a separate token. Returns the
/// new text and the position right after `s`.
fn insert_padded(text: &str, at: usize, s: &str) -> (String, usize) {
    let (before, after) = text.split_at(at);
    let left = padding(before, s);
    let right = padding(s, after);
    let caret = at + left.len() + s.len();
    (format!("{before}{left}{s}{right}{after}"), caret)
}

/// Performs a twist at the caret. The state before the new caret is exactly the
/// state before the old caret with the twist applied. Returns (text, caret).
pub fn twist(text: &str, caret: usize, mv: &str, inversion: Inversion) -> (String, usize) {
    let p = parse(&text[..caret]);

    // Normal and NISS moves commute, so an opposite-inversion twist can go
    // before the top-level group. Illegal regions play nothing, so twists go
    // before them as well.
    let blocking = if p.inversion() != inversion {
        p.open.first()
    } else {
        p.open.iter().find(|g| g.dead)
    };
    let at = match blocking {
        Some(g) => g.span.start,
        None if p.in_comment => text[caret..].find('\n').map_or(text.len(), |n| caret + n),
        None => caret,
    };
    let newline = blocking.is_none() && p.in_comment;

    let before = parse(&text[..at]);
    let (new, end) = if inversion == Inversion::Inverse && before.open.is_empty() {
        // Extend a `^()` directly before the insertion point.
        let close = before.tokens.last().and_then(|t| match t.kind {
            TokenKind::Close {
                kind: GroupKind::Niss,
                has_multiplier: false,
                ..
            } => Some(t.span.end - 1),
            _ => None,
        });
        let multiplied = text[at..].starts_with(|c: char| c.is_ascii_digit() || c == '\'');
        match close {
            Some(close) if !multiplied => {
                let (new, _) = insert_padded(text, close, mv);
                let end = at + (new.len() - text.len());
                (new, end)
            }
            _ if newline => splice_line(text, at, &format!("^({mv})")),
            _ => insert_padded(text, at, &format!("^({mv})")),
        }
    } else if newline {
        splice_line(text, at, mv)
    } else {
        insert_padded(text, at, mv)
    };
    let caret = if blocking.is_some() {
        caret + (new.len() - text.len())
    } else {
        end
    };
    (new, caret)
}

fn splice_line(text: &str, at: usize, s: &str) -> (String, usize) {
    let new = format!("{}\n{s}{}", &text[..at], &text[at..]);
    (new, at + 1 + s.len())
}

/// Which gap between tokens a position is in: the number of tokens before it.
/// All of the whitespace between two tokens is one gap. The second value is
/// whether it's actually partway through the next token.
pub fn gap(text: &str, off: usize) -> (usize, bool) {
    let tokens = parse(text).tokens;
    let n = tokens.iter().take_while(|t| t.span.end <= off).count();
    (n, tokens.get(n).is_some_and(|t| t.span.start < off))
}

/// Canonical position of a gap: right after the token before it.
pub fn gap_offset(text: &str, gap: usize) -> usize {
    match gap.checked_sub(1) {
        Some(i) => parse(text).tokens[i].span.end,
        None => 0,
    }
}

/// Span of the whole group that a bracket or separator belongs to, or just the
/// token's own span if it's anything else (or the group never closes).
fn deletion_span(text: &str, tokens: &[Token], index: usize) -> Range<usize> {
    // Index of the opener, and indices of the group's tokens so far.
    let mut stack: Vec<(usize, Vec<usize>)> = vec![];
    for (i, t) in tokens.iter().enumerate() {
        let s = &text[t.span.clone()];
        if opener(s).is_some() {
            stack.push((i, vec![i]));
        } else if t.kind == TokenKind::Sep {
            stack.last_mut().expect("in a group").1.push(i);
        } else if s.starts_with([')', ']']) && t.kind != TokenKind::Stray {
            let (open, members) = stack.pop().expect("closes a group");
            if i == index || members.contains(&index) {
                return tokens[open].span.start..t.span.end;
            }
        }
    }
    tokens[index].span.clone()
}

/// Deletes the token before or after the caret, or the one that the caret is
/// partway through. Brackets and separators delete their whole group, since
/// otherwise the group would swallow what's after it. Returns the new text and
/// caret, or `None` if there's nothing to delete.
pub fn delete(text: &str, caret: usize, forward: bool) -> Option<(String, usize)> {
    let tokens = parse(text).tokens;
    let index = if forward {
        tokens.iter().position(|t| t.span.end > caret)
    } else {
        tokens.iter().rposition(|t| t.span.start < caret)
    }?;
    let mut span = deletion_span(text, &tokens, index);
    // Include the whitespace up to the caret, unless it ends a comment.
    if !parse(&text[..caret]).in_comment {
        span = span.start.min(caret)..span.end.max(caret);
    }
    let (mut before, mut after) = (&text[..span.start], &text[span.end..]);

    // Leave at most one space behind, and at least one if it's needed.
    let mut space = "";
    if after.is_empty() || after.starts_with(|c: char| c.is_whitespace() || ")],:".contains(c)) {
        if let Some(b) = before.strip_suffix(' ') {
            before = b;
        } else if (before.is_empty() || before.ends_with(['\n', '(', '[']))
            && let Some(a) = after.strip_prefix(' ')
        {
            after = a;
        }
    } else if !before.is_empty()
        && !before.ends_with(|c: char| c.is_whitespace() || "([".contains(c))
    {
        space = " ";
    }
    Some((format!("{before}{space}{after}"), before.len()))
}

/// What a double click (`Token`) or a triple click (`Run`) selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// A token, or the whole group of a bracket or separator.
    Token,
    /// Tokens up to the nearest bracket, separator, comment or line break, or a
    /// group's insides.
    Run,
}

/// A bracketed group: its whole span, its tokens' indices, and the span between
/// its brackets. An unclosed group runs to the last token.
struct Group {
    outer: Range<usize>,
    members: Vec<usize>,
    inner: Range<usize>,
}

fn groups(text: &str, tokens: &[Token]) -> Vec<Group> {
    let mut out = vec![];
    let mut stack: Vec<Group> = vec![];
    let last = tokens.last().map_or(0, |t| t.span.end);
    for (i, t) in tokens.iter().enumerate() {
        let s = &text[t.span.clone()];
        if opener(s).is_some() {
            let span = t.span.clone();
            stack.push(Group {
                outer: span.start..last,
                members: vec![i],
                inner: span.end..last,
            });
        } else if t.kind == TokenKind::Sep {
            stack.last_mut().expect("in a group").members.push(i);
        } else if s.starts_with([')', ']']) && t.kind != TokenKind::Stray {
            let mut g = stack.pop().expect("closes a group");
            g.members.push(i);
            g.outer.end = t.span.end;
            g.inner.end = t.span.start;
            out.push(g);
        }
    }
    out.extend(stack);
    out
}

/// The unit at `off`, for a click on the character starting there. Whitespace
/// counts as the nearest token on its line. `None` if there isn't one.
pub fn unit_at(text: &str, off: usize, unit: Unit) -> Option<Range<usize>> {
    let tokens = parse(text).tokens;
    let line = |o: usize| text[..o].matches('\n').count();
    let index = tokens
        .iter()
        .position(|t| t.span.contains(&off))
        .or_else(|| {
            let dist = |t: &Token| {
                t.span
                    .start
                    .saturating_sub(off)
                    .max(off.saturating_sub(t.span.end - 1))
            };
            let on_line = tokens.iter().enumerate().filter(|(_, t)| {
                line(t.span.start) == line(off)
                    || (t.kind == TokenKind::Comment && line(t.span.end) == line(off))
            });
            on_line.min_by_key(|(_, t)| dist(t)).map(|(i, _)| i)
        })?;
    let groups = groups(text, &tokens);
    if let Some(g) = groups.iter().find(|g| g.members.contains(&index)) {
        return Some(match unit {
            Unit::Token => g.outer.clone(),
            Unit::Run => {
                // Just the tokens, without the whitespace inside the brackets.
                let inside: Vec<_> = tokens
                    .iter()
                    .filter(|t| g.inner.start <= t.span.start && t.span.end <= g.inner.end)
                    .collect();
                match (inside.first(), inside.last()) {
                    (Some(a), Some(b)) => a.span.start..b.span.end,
                    _ => g.inner.start..g.inner.start,
                }
            }
        });
    }
    let span = tokens[index].span.clone();
    if unit == Unit::Token {
        return Some(span);
    }
    // Not a bracket, even a nested NISS one (an error), nor a comment.
    let plain = |i: usize| {
        let kind = matches!(
            tokens[i].kind,
            TokenKind::Move | TokenKind::Pause | TokenKind::Error
        );
        kind && !groups.iter().any(|g| g.members.contains(&i))
    };
    if !plain(index) {
        return Some(span);
    }
    let joined = |a: usize, b: usize| {
        plain(a) && plain(b) && !text[tokens[a].span.end..tokens[b].span.start].contains('\n')
    };
    let (mut lo, mut hi) = (index, index);
    while lo > 0 && joined(lo - 1, lo) {
        lo -= 1;
    }
    while hi + 1 < tokens.len() && joined(hi, hi + 1) {
        hi += 1;
    }
    Some(tokens[lo].span.start..tokens[hi].span.end)
}

/// Grows a range to take in the whole of any group whose bracket it covers,
/// unless it's within the group's brackets.
pub fn close_groups(text: &str, mut r: Range<usize>) -> Range<usize> {
    let tokens = parse(text).tokens;
    let groups = groups(text, &tokens);
    loop {
        let grow = groups.iter().find(|g| {
            let overlaps = r.start < g.outer.end && g.outer.start < r.end;
            let inside = g.inner.start <= r.start && r.end <= g.inner.end;
            let covers = r.start <= g.outer.start && g.outer.end <= r.end;
            overlaps && !inside && !covers
        });
        let Some(g) = grow else { return r };
        r = r.start.min(g.outer.start)..r.end.max(g.outer.end);
    }
}
