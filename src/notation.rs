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
    },
    /// Right after a closing bracket, such as the `2'` in `[R, U]2'`. The
    /// state between the bracket and it is that of one pass.
    Multiplier,
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
        let mut start = i;
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
                let no_sep = frame.group.kind == GroupKind::Binary && frame.lhs.is_none();
                let parsed_mult = parse_multiplier(mult);
                let op = frame.closed().pow(parsed_mult.unwrap_or(1));
                let acc = stack.last_mut().map_or(&mut top, |f| &mut f.op);
                *acc = acc.then(&op);
                tokens.push(Token {
                    span: start..i,
                    kind: if no_sep || parsed_mult.is_none() {
                        TokenKind::BadClose
                    } else {
                        TokenKind::Close {
                            kind: frame.group.kind,
                            open: frame.group.span.start,
                        }
                    },
                });
                if mult.is_empty() {
                    continue;
                }
                start = i;
                i += mult.len();
                TokenKind::Multiplier
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

pub(crate) fn opener(s: &str) -> Option<(GroupKind, usize)> {
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
                ..
            } => Some(t.span.start),
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

/// Span of the whole group that a bracket or separator belongs to, with its
/// multiplier, or just the token's own span if it's anything else (or the
/// group never closes).
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
                let end = match tokens.get(i + 1) {
                    Some(m) if !is_spaced(m.kind) => m.span.end,
                    _ => t.span.end,
                };
                return tokens[open].span.start..end;
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
    Some(delete_span(
        text,
        deletion_span(text, &tokens, index),
        caret,
    ))
}

/// Deletes a span and the whitespace between it and the caret, leaving one
/// space between what's left where one is needed. Returns the new text and
/// caret.
pub fn delete_span(text: &str, mut span: Range<usize>, caret: usize) -> (String, usize) {
    // Include the whitespace up to the caret, unless it ends a comment.
    if !parse(&text[..caret]).in_comment {
        span = span.start.min(caret)..span.end.max(caret);
    }
    let (text, gap, _) = close_up(&text[..span.start], &text[span.end..]);
    (text, gap.start)
}

/// Puts two texts together, leaving at most one space between them, and at
/// least one if it's needed: a line break after a comment. Returns the text, the span between what's left of
/// each, and how much was dropped from the start of `after`.
pub fn close_up(mut before: &str, after: &str) -> (String, Range<usize>, usize) {
    let mut rest = after;
    let mut space = "";
    if parse(before).in_comment {
        // Or it would be part of the comment.
        if !rest.is_empty() && !rest.starts_with('\n') {
            rest = rest.trim_start_matches(' ');
            space = "\n";
        }
    } else if rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace() || ")],:".contains(c))
    {
        if let Some(b) = before.strip_suffix(' ') {
            before = b;
        } else if (before.is_empty() || before.ends_with(['\n', '(', '[']))
            && let Some(a) = rest.strip_prefix(' ')
        {
            rest = a;
        }
    } else if !before.is_empty()
        && !before.ends_with(|c: char| c.is_whitespace() || "([".contains(c))
    {
        space = " ";
    }
    let gap = before.len()..before.len() + space.len();
    (
        format!("{before}{space}{rest}"),
        gap,
        after.len() - rest.len(),
    )
}

/// What a double click (`Token`) or a triple click (`Run`) selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// A token, or the whole group of a bracket, or a separator's group's
    /// insides. In a comment, a word, as in any text.
    Token,
    /// Tokens up to the nearest bracket, separator or line break, or a
    /// bracket's group's insides, or a separator's whole group.
    Run,
}

/// A bracketed group: its whole span with any multiplier, its tokens' indices
/// (with the multiplier's), and the span between its brackets. An unclosed
/// group runs to the last token.
struct Group {
    outer: Range<usize>,
    members: Vec<usize>,
    inner: Range<usize>,
}

impl Group {
    /// The end of its closing bracket, before any multiplier.
    fn brackets_end(&self) -> usize {
        (self.inner.end + 1).min(self.outer.end)
    }
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
        } else if !is_spaced(t.kind) {
            // A multiplier: the group just closed is the last one out.
            let g = out.last_mut().expect("just closed");
            g.members.push(i);
            g.outer.end = t.span.end;
        }
    }
    out.extend(stack);
    out
}

/// Whether a token isn't a multiplier, which only ever comes right after the
/// bracket that closes its group.
fn is_spaced(kind: TokenKind) -> bool {
    kind != TokenKind::Multiplier
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
        // A separator is inside the group, so it's the insides first.
        let unit = match unit {
            _ if tokens[index].kind != TokenKind::Sep => unit,
            Unit::Token => Unit::Run,
            Unit::Run => Unit::Token,
        };
        return Some(match unit {
            // On the multiplier, that too.
            Unit::Token if off >= g.brackets_end() && g.outer.contains(&off) => g.outer.clone(),
            Unit::Token => g.outer.start..g.brackets_end(),
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
    match unit {
        Unit::Token if tokens[index].kind == TokenKind::Comment && span.contains(&off) => {
            Some(word_at(text, span, off))
        }
        Unit::Token => Some(span),
        Unit::Run => {
            let (lo, hi) = run_of(text, &tokens, &groups, index);
            Some(tokens[lo].span.start..tokens[hi].span.end)
        }
    }
}

/// The word at `off` within `within`: a run of letters and digits, or of
/// whitespace, or else one character.
fn word_at(text: &str, within: Range<usize>, off: usize) -> Range<usize> {
    let class = |c: char| {
        if c.is_alphanumeric() || c == '_' {
            1
        } else if c.is_whitespace() {
            2
        } else {
            0
        }
    };
    let c = text[off..].chars().next().expect("in a token");
    if class(c) == 0 {
        return off..off + c.len_utf8();
    }
    let start = text[within.start..off]
        .char_indices()
        .rev()
        .take_while(|(_, d)| class(*d) == class(c))
        .last()
        .map_or(off, |(i, _)| within.start + i);
    let end = text[off..within.end]
        .char_indices()
        .find(|(_, d)| class(*d) != class(c))
        .map_or(within.end, |(i, _)| off + i);
    start..end
}

/// Indices of the first and last tokens of the run with token `index` in it.
/// A bracket or separator is a run of its own.
fn run_of(text: &str, tokens: &[Token], groups: &[Group], index: usize) -> (usize, usize) {
    // Not a bracket, even a nested NISS one (an error).
    let plain = |i: usize| {
        let kind = matches!(
            tokens[i].kind,
            TokenKind::Move | TokenKind::Pause | TokenKind::Error | TokenKind::Comment
        );
        kind && !groups.iter().any(|g| g.members.contains(&i))
    };
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
    (lo, hi)
}

/// The gap that the caret goes to past the next run, or its start going
/// backwards: from a gap to the gap at the far end of the run of the token
/// next to it. Past a bracket or separator, that's one token.
pub fn run_gap(text: &str, gap: usize, forward: bool) -> usize {
    let tokens = parse(text).tokens;
    let groups = groups(text, &tokens);
    if forward {
        if gap >= tokens.len() {
            return gap;
        }
        run_of(text, &tokens, &groups, gap).1 + 1
    } else {
        match gap.checked_sub(1) {
            Some(i) => run_of(text, &tokens, &groups, i).0,
            None => 0,
        }
    }
}

/// Grows a range to take in the whole of any group whose bracket it covers,
/// unless it's within the group's brackets. That's without the multiplier,
/// unless the range has some of it.
pub fn close_groups(text: &str, mut r: Range<usize>) -> Range<usize> {
    let tokens = parse(text).tokens;
    let groups = groups(text, &tokens);
    loop {
        let grow = groups.iter().find(|g| {
            let overlaps = r.start < g.outer.end && g.outer.start < r.end;
            let inside = g.inner.start <= r.start && r.end <= g.inner.end;
            let covers = r.start <= g.outer.start && g.brackets_end() <= r.end;
            overlaps && !inside && !covers
        });
        let Some(g) = grow else { return r };
        let end = if r.end > g.brackets_end() {
            g.outer.end
        } else {
            g.brackets_end()
        };
        r = r.start.min(g.outer.start)..r.end.max(end);
    }
}

/// A token or bracketed group, as `invert` and `expand` see notation.
#[derive(Debug, Clone)]
enum Node {
    Move(String),
    Pause,
    Comment(String),
    Newline,
    Group {
        /// Its opening bracket, such as `^(`.
        open: String,
        lhs: Vec<Node>,
        /// For `[A, B]` and `[A: B]`, the separator and `B`.
        rhs: Option<(char, Vec<Node>)>,
        /// Its closing bracket and multiplier.
        close: String,
    },
}

/// The tokens in a range as a tree, or `None` if any bracket or separator
/// belongs to a group that isn't all in it, if the range cuts a token, or if
/// there's a token that isn't a move or a group.
fn tree(text: &str, range: Range<usize>) -> Option<Vec<Node>> {
    let tokens = parse(text).tokens;
    let tokens: Vec<_> = tokens
        .into_iter()
        .filter(|t| t.span.end > range.start && t.span.start < range.end)
        .collect();
    if tokens
        .iter()
        .any(|t| t.span.start < range.start || t.span.end > range.end)
    {
        return None;
    }
    let mut i = 0;
    let mut at = tokens.first().map_or(range.start, |t| t.span.start);
    let out = seq(text, &tokens, &mut i, &mut at)?;
    (i == tokens.len()).then_some(out)
}

/// The nodes up to a separator, a closing bracket, or the end.
fn seq(text: &str, tokens: &[Token], i: &mut usize, at: &mut usize) -> Option<Vec<Node>> {
    let mut out = vec![];
    while let Some(t) = tokens.get(*i) {
        let s = &text[t.span.clone()];
        let newlines = text[*at..t.span.start].matches('\n').count();
        out.extend(std::iter::repeat_n(Node::Newline, newlines));
        *at = t.span.start;
        if t.kind == TokenKind::Sep || s.starts_with([')', ']']) {
            break;
        }
        *i += 1;
        *at = t.span.end;
        out.push(match t.kind {
            TokenKind::Move => Node::Move(s.to_owned()),
            TokenKind::Pause => Node::Pause,
            TokenKind::Comment => Node::Comment(s.to_owned()),
            // An opener, or a nested NISS one, which is an error.
            TokenKind::Open | TokenKind::Error if opener(s).is_some() => {
                let lhs = seq(text, tokens, i, at)?;
                let mut next = tokens.get(*i)?;
                let rhs = if next.kind == TokenKind::Sep {
                    let sep = text[next.span.clone()].chars().next()?;
                    *i += 1;
                    *at = next.span.end;
                    let rhs = seq(text, tokens, i, at)?;
                    next = tokens.get(*i)?;
                    Some((sep, rhs))
                } else {
                    None
                };
                if !matches!(next.kind, TokenKind::Close { .. }) {
                    return None;
                }
                *i += 1;
                *at = next.span.end;
                let mut close = text[next.span.clone()].to_owned();
                if let Some(m) = tokens.get(*i)
                    && m.kind == TokenKind::Multiplier
                    && m.span.start == next.span.end
                {
                    close.push_str(&text[m.span.clone()]);
                    *i += 1;
                    *at = m.span.end;
                }
                Node::Group {
                    open: s.to_owned(),
                    lhs,
                    rhs,
                    close,
                }
            }
            _ => return None,
        });
    }
    Some(out)
}

/// Writes nodes out, a space between tokens, and a line break after a comment.
fn render(nodes: &[Node], out: &mut String) {
    let push = |out: &mut String, s: &str| {
        // Only a comment has a `//`, and it runs to the end of the line.
        if out.rsplit('\n').next().is_some_and(|l| l.contains("//")) {
            out.push('\n');
        } else {
            out.push_str(padding(out, s));
        }
        out.push_str(s);
    };
    for n in nodes {
        match n {
            Node::Move(s) | Node::Comment(s) => push(out, s),
            Node::Pause => push(out, "."),
            Node::Newline => {
                out.truncate(out.trim_end_matches(' ').len());
                out.push('\n');
            }
            Node::Group {
                open,
                lhs,
                rhs,
                close,
            } => {
                push(out, open);
                render(lhs, out);
                if let Some((sep, rhs)) = rhs {
                    push(out, &sep.to_string());
                    render(rhs, out);
                }
                push(out, close);
            }
        }
    }
}

/// Undoes a move: toggles its prime.
fn invert_move(s: &str) -> String {
    match s.strip_suffix('\'') {
        Some(s) => s.to_owned(),
        None => format!("{s}'"),
    }
}

/// Nodes that undo `nodes`. Lines go in reverse order, each keeping its
/// comment at its end.
fn invert_nodes(nodes: &[Node]) -> Vec<Node> {
    let mut out = vec![];
    for (k, line) in nodes.rsplit(|n| matches!(n, Node::Newline)).enumerate() {
        if k > 0 {
            out.push(Node::Newline);
        }
        let (comment, moves) = match line.split_last() {
            Some((c @ Node::Comment(_), rest)) => (Some(c.clone()), rest),
            _ => (None, line),
        };
        out.extend(moves.iter().rev().map(invert_node));
        out.extend(comment);
    }
    out
}
fn invert_node(n: &Node) -> Node {
    match n {
        Node::Move(s) => Node::Move(invert_move(s)),
        Node::Group {
            open,
            lhs,
            rhs,
            close,
        } => {
            // [A, B]' = [B, A], [A: B]' = [A: B'], and (A)n' = (A')n.
            let (lhs, rhs) = match rhs {
                Some((',', b)) => (b.clone(), Some((',', lhs.clone()))),
                Some((sep, b)) => (lhs.clone(), Some((*sep, invert_nodes(b)))),
                None => (invert_nodes(lhs), None),
            };
            Node::Group {
                open: open.clone(),
                lhs,
                rhs,
                close: close.clone(),
            }
        }
        n => n.clone(),
    }
}

/// Writes out every commutator and conjugate in `nodes`, and returns whether
/// there were any. Their multipliers too if `repeat`, and nested ones' always.
fn expand_nodes(nodes: Vec<Node>, repeat: bool) -> (Vec<Node>, bool) {
    let mut out = vec![];
    let mut any = false;
    for n in nodes {
        let Node::Group {
            open,
            lhs,
            rhs,
            close,
        } = n
        else {
            out.push(n);
            continue;
        };
        let (lhs, inner) = expand_nodes(lhs, true);
        let Some((sep, rhs)) = rhs else {
            any |= inner;
            out.push(Node::Group {
                open,
                lhs,
                rhs: None,
                close,
            });
            continue;
        };
        any = true;
        let (rhs, _) = expand_nodes(rhs, true);
        let trim = |v: Vec<Node>| {
            let is_line = |n: &Node| matches!(n, Node::Newline);
            let start = v.iter().position(|n| !is_line(n)).unwrap_or(v.len());
            let end = v.iter().rposition(|n| !is_line(n)).map_or(start, |i| i + 1);
            v[start..end].to_vec()
        };
        let mut body = trim(lhs.clone());
        body.extend(trim(rhs.clone()));
        body.extend(trim(invert_nodes(&lhs)));
        if sep == ',' {
            body.extend(trim(invert_nodes(&rhs)));
        }
        let mult = &close[1..];
        if mult.is_empty() {
            out.extend(body);
        } else if repeat {
            let n = parse_multiplier(mult).expect("closed groups have multipliers");
            let body = if n < 0 { invert_nodes(&body) } else { body };
            for _ in 0..n.unsigned_abs() {
                out.extend(body.iter().cloned());
            }
        } else {
            out.push(Node::Group {
                open: "(".to_owned(),
                lhs: body,
                rhs: None,
                close: format!("){mult}"),
            });
        }
    }
    (out, any)
}

/// Puts `nodes` in place of a span, with whatever spaces or line break that
/// keeps them separate from what's around them. Returns the new text and
/// where they went.
fn replace_nodes(text: &str, span: Range<usize>, nodes: &[Node]) -> (String, Range<usize>) {
    let mut new = String::new();
    render(nodes, &mut new);
    let (before, after) = (&text[..span.start], &text[span.end..]);
    let left = padding(before, &new);
    let right = if parse(&new).in_comment && !after.is_empty() && !after.starts_with('\n') {
        "\n"
    } else {
        padding(&new, after)
    };
    let start = span.start + left.len();
    let range = start..start + new.len();
    (format!("{before}{left}{new}{right}{after}"), range)
}

/// Replaces the tokens in a range with ones that undo them: the lines in
/// reverse order, each keeping its comment, and their moves in reverse order
/// with their primes toggled. Returns the new text and the new range, or
/// `None` if there are no tokens or a group isn't all in the range.
pub fn invert(text: &str, range: Range<usize>) -> Option<(String, Range<usize>)> {
    let tokens = parse(text).tokens;
    // Inverting `X` in `Xn` is the same as inverting `Xn`, so a range up to a
    // multiplier takes it in.
    let end = range.end;
    let range = match tokens.iter().position(|t| t.span.start == end) {
        Some(i)
            if tokens[i].kind == TokenKind::Multiplier
                && i > 0
                && matches!(tokens[i - 1].kind, TokenKind::Close { .. }) =>
        {
            range.start..tokens[i].span.end
        }
        _ => range,
    };
    let nodes = tree(text, range.clone())?;
    let inside: Vec<_> = tokens
        .iter()
        .filter(|t| range.start <= t.span.start && t.span.end <= range.end)
        .collect();
    let span = inside.first()?.span.start..inside.last()?.span.end;
    let (new, at) = replace_nodes(text, span.clone(), &invert_nodes(&nodes));
    Some((new, range.start..end + at.end - span.end))
}

/// Writes out each commutator and conjugate whose brackets are in a range:
/// `[A, B]` as `A B A' B'` and `[A: B]` as `A B A'`. A multiplier in the
/// range is written out too, by repeating it; one that isn't stays, on
/// parentheses. Returns the new text and the new range, or `None` if there
/// aren't any.
pub fn expand(text: &str, range: Range<usize>) -> Option<(String, Range<usize>)> {
    let tokens = parse(text).tokens;
    let groups = groups(text, &tokens);
    let within = |g: &&Group| range.start <= g.outer.start && g.brackets_end() <= range.end;
    let mut outer: Vec<Range<usize>> = groups
        .iter()
        .filter(within)
        .filter(|g| {
            !groups.iter().filter(within).any(|h| {
                h.outer != g.outer && h.outer.start <= g.outer.start && g.outer.end <= h.outer.end
            })
        })
        .map(|g| g.outer.clone())
        .collect();
    // From the end, so that the earlier spans still hold.
    outer.sort_by_key(|r| std::cmp::Reverse(r.start));
    let mut text = text.to_owned();
    let mut end = range.end;
    let mut any = false;
    for span in outer {
        let Some(nodes) = tree(&text, span.clone()) else {
            continue;
        };
        let (nodes, expanded) = expand_nodes(nodes, span.end <= range.end);
        if !expanded {
            continue;
        }
        let (new, _) = replace_nodes(&text, span, &nodes);
        end = end + new.len() - text.len();
        text = new;
        any = true;
    }
    any.then_some((text, range.start..end))
}

/// Where a separator goes in a range, nearest `off`: right after a token, or
/// at either end, but not in a group that isn't all in it or in a comment.
pub fn separator_offset(text: &str, range: Range<usize>, off: usize) -> usize {
    let p = parse(text);
    let groups = groups(text, &p.tokens);
    let ok = |c: usize| {
        let split = groups.iter().any(|g| {
            let around = g.outer.start <= range.start && range.end <= g.outer.end;
            g.outer.start < c && c < g.outer.end && !around
        });
        !split && !parse(&text[..c]).in_comment
    };
    let ends = p.tokens.iter().map(|t| t.span.end);
    let candidates = [range.start, range.end]
        .into_iter()
        .chain(ends.filter(|e| range.contains(e)))
        .filter(|c| ok(*c));
    candidates
        .min_by_key(|c| c.abs_diff(off))
        .unwrap_or(range.start)
}

/// Inserts a separator at `at`, with a space after it where one is needed.
/// Returns the new text and how much longer it is.
pub fn insert_separator(text: &str, at: usize, sep: char) -> (String, usize) {
    let (new, _) = insert_padded(text, at, &sep.to_string());
    let n = new.len() - text.len();
    (new, n)
}
