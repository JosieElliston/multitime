use crate::doc::{Doc, Pos};
use crate::notation::{Inversion::*, *};
use crate::puzzle::Perm;
use crate::session::{END, Inside, MARK, OPEN, Session};

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
    fn text(&mut self, max: usize) -> String {
        const BITS: &[&str] = &[
            "R", "U", "F2", "D'", "Rw", "x", "2", "'", "(", ")", "[", "]", ":", ",", "^(", "^",
            "!(", "// hi", "\n", " ", " ", " ", " ", ".", "é", "/", "M", ")2", "&(",
        ];
        (0..self.below(max + 1))
            .map(|_| BITS[self.below(BITS.len())])
            .collect()
    }
    /// Well-formed notation, with groups nested up to `depth`.
    fn notation(&mut self, depth: usize, top: bool) -> String {
        let mut out = String::new();
        for _ in 0..self.below(5) {
            let item = match self.below(if depth == 0 { 3 } else { 9 }) {
                0 => ["R", "U2", "F'", "Rw2'", "x", "M", "."][self.below(7)].to_owned(),
                1 => "// c\n".to_owned(),
                2 => "\n".to_owned(),
                3 | 4 => {
                    let sep = [",", ":"][self.below(2)];
                    let (a, b) = (
                        self.notation(depth - 1, false),
                        self.notation(depth - 1, false),
                    );
                    format!("[{a}{sep} {b}]{}", ["", "2", "'"][self.below(3)])
                }
                5 if top => format!("^({})", self.notation(depth - 1, false)),
                _ => format!(
                    "({}){}",
                    self.notation(depth - 1, false),
                    ["", "3", "2'"][self.below(3)]
                ),
            };
            out.push_str(&item);
            out.push(' ');
        }
        out
    }
    fn off(&mut self, s: &str) -> usize {
        loop {
            let o = self.below(s.len() + 1);
            if s.is_char_boundary(o) {
                return o;
            }
        }
    }
}

fn st(s: &str) -> Perm {
    parse(s).op.apply(&Perm::ID)
}
fn tw(s: &str, mv: &str, i: Inversion) -> String {
    let c = s.find('|').unwrap();
    let (mut t, c) = twist(&s.replace('|', ""), c, mv, i);
    t.insert(c, '|');
    t
}

#[test]
fn moves() {
    assert!(st("(R U R' U')6").0 == Perm::ID.0);
    assert!(st("(R U)105") == Perm::ID && st("(R U)35") != Perm::ID);
    assert_eq!(st("Rw"), st("x L"));
    assert_eq!(st("r"), st("2Rw"));
    assert_eq!(st("M"), st("x' L' R"));
    assert_eq!(st("E"), st("y' U D'"));
    assert_eq!(st("S"), st("z F' B"));
    assert_eq!(st("@R"), st("x"));
    // T perm has order 2, and sune order 6.
    assert_eq!(st("(R U R' U' R' F R2 U' R' U' R U R' F')2"), Perm::ID);
    assert_eq!(st("(R U R' U R U2 R')6"), Perm::ID);
    assert!(st("x y").is_solved() && !st("R").is_solved());
    assert_eq!(st("x U"), st("F x")); // x takes F to U
}

#[test]
fn groups() {
    assert_eq!(st("[R, U]"), st("R U R' U'"));
    assert_eq!(st("[R: U D]2'"), st("R D' U' D' U' R'"));
    assert_eq!(st("[R: U"), st("R U"));
    assert_eq!(st("R U // F\nD"), st("R U D"));
    assert_eq!(st("(((R)999999)999999)4"), Perm::ID);
    assert_eq!(st("R ?? U"), st("R U"));
}

#[test]
fn niss() {
    assert_eq!(st("R ^(U) ^(F)"), st("R ^(U F)"));
    assert_eq!(st("R ^(U F) D"), st("F' U' R D"));
    assert_eq!(st("R ^(U F"), st("F' U' R"));
    // Expanding NISS needs a correction if the normal part ends rotated.
    let (s, n) = ("F2 B", "B' F2 U x");
    assert!(st(&format!("{s} {n} ^(U)")).is_solved());
    assert!(!st(&format!("{s} {n} U'")).is_solved());
    assert!(st(&format!("{s} {n} x' U'")).is_solved());
}

#[test]
fn twist_examples() {
    assert_eq!(tw("^(U| F)", "R", Normal), "R ^(U| F)");
    assert_eq!(tw("[R: U|]", "D", Inverse), "^(D) [R: U|]");
    assert_eq!(tw("R|", "D", Inverse), "R ^(D)|");
    assert_eq!(tw("R ^(D)|", "F", Inverse), "R ^(D F)|");
    assert_eq!(tw("[R: U|]", "D", Normal), "[R: U D|]");
    assert_eq!(tw("R|2", "U", Normal), "R U| 2");
    assert_eq!(tw("R // e|o\nF", "U", Normal), "R // eo\nU|\nF");
}

#[test]
fn twist_invariant() {
    let mut rng = Rng(0x9e3779b97f4a7c15);
    for _ in 0..100_000 {
        let text = rng.text(14);
        let c = rng.off(&text);
        let inv = [Normal, Inverse][rng.below(2)];
        let mv = ["R", "U2", "F'"][rng.below(3)];
        let (new, nc) = twist(&text, c, mv, inv);
        let want = parse(&text[..c])
            .op
            .then(&Op::new(parse_move(mv).unwrap(), inv));
        assert_eq!(
            parse(&new[..nc]).op,
            want,
            "{text:?} at {c} -> {new:?} at {nc}"
        );
    }
}

#[test]
fn forking_beside_a_bubble() {
    let full = Session::new(Doc::from_file("R // hi\n>\n   U\n").unwrap());
    assert_eq!(show(&full), "R // hi\n[^>]\n U|");
    // Before it, even partway through a comment.
    for off in [6, 7] {
        let mut s = full.clone();
        s.set_caret_offset(off);
        s.fork();
        assert_eq!(show(&s), "R // hi\n[^>|>]");
    }
    // After it, even after whitespace.
    for off in [10, 11] {
        let mut s = full.clone();
        s.set_caret_offset(off);
        s.fork();
        assert_eq!(show(&s), "R // hi\n[>^>|]");
    }
}

#[test]
fn forking_is_typing_a_fork() {
    let mut s = Session::new(Doc::new("R U // hi"));
    s.go_to(Pos {
        seg: s.doc().root(),
        off: 1,
    });
    // Without the spaces around it.
    s.fork();
    assert_eq!(show(&s), "R\n[^>|]\nU // hi");
    s.fork();
    assert_eq!(show(&s), "R\n[>^>|]");
    s.undo();
    s.undo();
    // Partway through a token, or a comment, it's after it.
    let mut s = Session::new(Doc::new("R U2' F"));
    s.go_to(Pos {
        seg: s.doc().root(),
        off: 3,
    });
    s.fork();
    assert_eq!(show(&s), "R U2'\n[^>|]\nF");
    let mut s = Session::new(Doc::new("R U // hi\nF"));
    s.go_to(Pos {
        seg: s.doc().root(),
        off: 7,
    });
    s.fork();
    assert_eq!(show(&s), "R U // hi\n[^>|]\nF");
    // Joining deletes the bubble's line, from anywhere in the segment.
    for (off, inside, want) in [(1, false, "R U // hi\nF|"), (0, true, "R U // hi\n|F")] {
        let mut s = s.clone();
        if inside {
            s.enter(s.caret().seg);
        } else {
            s.go_to(Pos {
                seg: s.caret().seg,
                off,
            });
        }
        s.join();
        assert_eq!(show(&s), want);
    }
    let mut s = Session::new(Doc::new("R>U"));
    s.join();
    assert_eq!(show(&s), "R\nU|");
}

/// Edits like an editor would: replace a range of the text.
fn replace(s: &mut Session, r: std::ops::Range<usize>, with: &str) {
    let mut t = s.text();
    t.replace_range(r.clone(), with);
    s.edit(&t, r.start + with.len());
    s.doc().check();
}
fn typ(s: &mut Session, text: &str) {
    for c in text.chars() {
        let at = s.caret_offset();
        replace(s, at..at, &c.to_string());
    }
}
fn backspace(s: &mut Session) {
    let at = s.caret_offset();
    replace(s, at - 1..at, "");
}
/// The text with each bubble in brackets on its line, `^` before each active
/// mark, and the caret, and `«»` around the selection if there is one.
fn show(s: &Session) -> String {
    let mut out = String::new();
    let bubbles = s.bubbles();
    let (a, c) = (s.anchor_offset(), s.caret_offset());
    let text = s.text();
    for i in 0..=text.len() {
        if !text.is_char_boundary(i) {
            continue;
        }
        let bubble = bubbles.iter().find(|b| b.marks().contains(&i));
        if bubble.is_some_and(|b| b.line == i) {
            out.push('[');
        }
        if i == c {
            out.push('|');
        }
        if a != c && i == a.min(c) {
            out.push('«');
        }
        if a != c && i == a.max(c) {
            out.push('»');
        }
        if let Some(b) = bubble {
            if i == b.line + b.n {
                out.push(']');
            } else if i == b.line + b.active {
                out.push('^');
            }
        }
        let Some(ch) = text[i..].chars().next() else {
            break;
        };
        out.push_str(match ch {
            MARK => ">",
            OPEN | END => "\n",
            _ => &text[i..i + ch.len_utf8()],
        });
    }
    out
}
/// The offset of the `>` of a branch, after it.
fn mark_offset(s: &Session, seg: crate::doc::SegId) -> usize {
    let mut s = s.clone();
    s.enter(seg);
    s.caret_offset()
}

#[test]
fn bubbles() {
    let mut s = Session::new(Doc::new("R U"));
    s.go_to(Pos {
        seg: s.doc().root(),
        off: 1,
    });
    typ(&mut s, ">");
    assert_eq!(show(&s), "R\n[^>|]\nU");
    typ(&mut s, ">");
    assert_eq!(show(&s), "R\n[>^>|]");
    // Typing continues after the bubble.
    typ(&mut s, "F>D");
    assert_eq!(show(&s), "R\n[>^>]\nF\n[^>]\nD|");
    assert_eq!(s.doc().to_file(), "R\n>\n  U\n>\n  F\n  >\n    D\n");
    let full = s.clone();
    let root = s.doc().root();
    let (u, f) = (s.doc().get(root).children[0], s.doc().get(root).children[1]);

    // Going through it makes the branch that it's after active, and remembers
    // the active branch of each.
    s.enter(u);
    assert_eq!(show(&s), "R\n[^>|>]\nU");
    s.cycle_sibling(1);
    assert_eq!(show(&s), "R\n[>^>|]\nF\n[^>]\nD");
    // Out of it, and in again.
    assert!(s.step_bubble(true) && !s.step_bubble(true));
    assert_eq!(show(&s), "R\n[>^>]\n|F\n[^>]\nD");
    assert!(s.step_bubble(false) && s.step_bubble(false) && !s.step_bubble(false));
    assert_eq!(show(&s), "R|\n[>^>]\nF\n[^>]\nD");
    // Switching is at the bubble before the caret, which there isn't.
    s.cycle_sibling(1);
    assert_eq!(show(&s), "R|\n[>^>]\nF\n[^>]\nD");
    // Before the first mark, the caret keeps the active branch.
    s.set_caret_offset(2);
    assert_eq!(
        (show(&s).as_str(), s.inside()),
        ("R\n[|>^>]\nF\n[^>]\nD", Inside::Start)
    );
    s.cycle_sibling(1);
    assert_eq!(show(&s), "R\n[|^>>]\nU");

    // Backspace in it deletes a branch.
    let mut s = full.clone();
    s.enter(f);
    backspace(&mut s);
    assert_eq!(show(&s), "R\n[^>|]\nU");
    // The only one keeps its text, and the line break before the bubble.
    backspace(&mut s);
    assert_eq!(show(&s), "R\n|U");
    // Each is its own undo step.
    s.undo();
    assert_eq!(show(&s), "R\n[^>|]\nU");
    s.undo();
    assert_eq!(s.doc(), full.doc());
    // The first one leaves the caret before the first mark.
    let mut s = full.clone();
    s.enter(u);
    backspace(&mut s);
    assert_eq!(show(&s), "R\n[|^>]\nF\n[^>]\nD");
    // Backspace there does nothing.
    backspace(&mut s);
    assert_eq!(show(&s), "R\n[|^>]\nF\n[^>]\nD");
    s.undo();
    // Delete, in and at the end of it.
    s.enter(u);
    let o = s.caret_offset();
    replace(&mut s, o..o + 1, "");
    assert_eq!(show(&s), "R\n[^>|]\nU");
    replace(&mut s, o..o + 1, "");
    assert_eq!(show(&s), "R\n[^>|]\nU");
    // A line break from after or before it deletes the bubble, keeping the
    // active branch.
    for (off, r, want) in [(5, 4..5, "R\n|F\n[^>]\nD"), (1, 1..2, "R|\nF\n[^>]\nD")] {
        let mut s = full.clone();
        s.set_caret_offset(off);
        replace(&mut s, r, "");
        assert_eq!(show(&s), want);
        s.undo();
        assert_eq!(s.doc(), full.doc());
    }
    // Everything.
    let mut s = full.clone();
    let n = s.text().len();
    replace(&mut s, 0..n, "");
    assert_eq!(s.doc().to_file(), "\n");
    // Forking in it.
    let mut s = full.clone();
    s.enter(u);
    typ(&mut s, ">");
    assert_eq!(show(&s), "R\n[>^>|>]");
    // By tokens, joining leaves the line break.
    let mut s = Session::new(Doc::new("R>U"));
    s.set_caret_offset(4);
    s.delete(false);
    assert_eq!(s.doc().to_file(), "R\nU\n");
}

#[test]
fn typing_before_the_first_mark() {
    let mut s = Session::new(Doc::from_file("R U\n>\n  F\n>\n  D\n").unwrap());
    s.set_caret_offset(4);
    assert_eq!(s.inside(), Inside::Start);
    // Text goes on a line of its own before the bubble.
    typ(&mut s, "x y");
    assert_eq!(show(&s), "R U\nx y|\n[^>>]\nF");
    // A line break, too, and the caret stays.
    s.set_caret_offset(8);
    typ(&mut s, "\n");
    assert_eq!(show(&s), "R U\nx y\n\n[|^>>]\nF");
    // A `>` is a first branch.
    typ(&mut s, ">");
    assert_eq!(show(&s), "R U\nx y\n\n[^>|>>]");
}

#[test]
fn bubbles_and_comments() {
    let mut s = Session::new(Doc::new("// a > b"));
    assert_eq!(show(&s), "// a > b|");
    typ(&mut s, ">");
    assert_eq!(show(&s), "// a > b>|");
    // A comment ends at a bubble.
    let mut s = Session::new(Doc::from_file("// a > b>\n>\n  R\n").unwrap());
    assert_eq!(show(&s), "// a > b>\n[^>]\nR|");
    // Uncommenting a `>` makes a bubble, in place of the space around it.
    replace(&mut s, 0..2, "");
    assert_eq!(show(&s), "| a\n[^>]\nb\n[^>]\n[^>]\nR");
}

#[test]
fn copying_across_bubbles() {
    // Only the active branch, with its `>` on a line of its own.
    let mut s = Session::new(Doc::from_file("R // hi\n>\n  F\n>\n  D\n").unwrap());
    s.select_offsets(0, s.text().len(), None);
    assert_eq!(s.copy().as_deref(), Some("R // hi\n>\nF"));
    // Cut and pasted back, it's the same, but for the other branch.
    let s = Session::new(Doc::from_file("R U\n>\n  F B\n  >\n\n    >\n      D L\n").unwrap());
    let n = s.text().len();
    let file = s.doc().to_file();
    for a in 0..n {
        for b in a..=n {
            if s.at_offset(a).1 != Inside::No || s.at_offset(b).1 != Inside::No {
                continue;
            }
            let mut s = s.clone();
            s.select_offsets(a, b, None);
            let Some(clip) = s.cut() else { continue };
            s.paste(&clip);
            s.doc().check();
            assert_eq!(s.doc().to_file(), file, "{clip:?}");
        }
    }
    // The text after a pasted bubble becomes its branch.
    let mut s = Session::new(Doc::new("A B"));
    s.set_caret_offset(1);
    s.paste("X>Y");
    assert_eq!(show(&s), "AX\n[^>]\nY| B");
    s.undo();
    assert_eq!(show(&s), "A| B");
}

#[test]
fn copying_branches() {
    let file = "R\n>\n  F\n  >\n    G\n>\n  D L\n";
    let s = Session::new(Doc::from_file(file).unwrap());
    let root = s.doc().root();
    let (f, d) = (s.doc().get(root).children[0], s.doc().get(root).children[1]);
    let (start, after_f) = (mark_offset(&s, f) - 1, mark_offset(&s, f));
    // Whole subtrees, like a file.
    let mut s1 = s.clone();
    s1.select_offsets(start, after_f, None);
    assert_eq!(show(&s1), "R\n[«^>|»>]\nF\n[^>]\nG");
    let branch = s1.copy().unwrap();
    assert_eq!(branch, ">\n  F\n  >\n    G\n");
    let mut all = s.clone();
    all.select_offsets(start, after_f + 1, None);
    assert_eq!(all.copy().unwrap(), ">\n  F\n  >\n    G\n>\n  D L\n");
    // A selection that starts in a bubble stays in it.
    let mut s2 = s.clone();
    s2.select_offsets(after_f, 0, None);
    assert_eq!(show(&s2), "R\n[|«^>»>]\nF\n[^>]\nG");
    let mut s2 = s.clone();
    s2.select_offsets(after_f, s2.text().len(), None);
    assert_eq!(show(&s2), "R\n[>«^>|»]\nD L");

    // Pasted at the end of a leaf, they're its branches.
    let mut s3 = s.clone();
    s3.go_to(s3.doc().end(d));
    s3.paste(&branch);
    assert_eq!(
        s3.doc().to_file(),
        "R\n>\n  F\n  >\n    G\n>\n  D L\n  >\n    F\n    >\n      G\n"
    );
    assert_eq!(show(&s3), "R\n[>^>]\nD L\n[^>|]\nF\n[^>]\nG");
    // Mid-text, the text after the caret is the first branch, and the caret
    // is after the last pasted `>`.
    let mut s4 = s.clone();
    s4.go_to(Pos { seg: d, off: 1 });
    s4.paste(&branch);
    assert_eq!(show(&s4), "R\n[>^>]\nD\n[>^>|]\nF\n[^>]\nG");
    assert_eq!(s4.doc().get(s4.doc().get(d).children[0]).text, "L");
    // In a bubble, after the mark before the caret, or before the first.
    let mut s5 = s.clone();
    s5.enter(f);
    s5.paste(">\n  X\n>\n  Y\n");
    assert_eq!(show(&s5), "R\n[>>^>|>]\nY");
    let mut s6 = s.clone();
    s6.set_caret_offset(start);
    s6.paste(">\n  X\n");
    assert_eq!(show(&s6), "R\n[^>|>>]\nX");
    // Beside a bubble, like Cmd+Enter.
    let mut s7 = s.clone();
    s7.go_to(s7.doc().end(root));
    s7.paste(">X");
    assert_eq!(show(&s7), "R\n[^>|>>]\nX");
    // Pasting over selected branches replaces them.
    let mut s8 = s1.clone();
    s8.paste(">\n  X\n");
    assert_eq!(s8.doc().to_file(), "R\n>\n  X\n>\n  D L\n");
    // Cutting them all leaves the parent a leaf.
    let mut s9 = all.clone();
    assert!(s9.cut().is_some());
    assert_eq!(
        (s9.doc().to_file().as_str(), show(&s9).as_str()),
        ("R\n", "R|")
    );
    s9.undo();
    assert_eq!(s9.doc().to_file(), file);
}

#[test]
fn moving_lines() {
    let mut s = Session::new(Doc::from_file("A\nB\n>\n  C\n  D\n>\n  E\n").unwrap());
    let root = s.doc().root();
    s.go_to(Pos { seg: root, off: 0 });
    s.move_lines(true);
    assert_eq!(show(&s), "B\n|A\n[^>>]\nC\nD");
    // Past the bubble, into the active branch only.
    s.move_lines(true);
    assert_eq!(show(&s), "B\n[^>>]\n|A\nC\nD");
    assert_eq!(s.doc().to_file(), "B\n>\n  A\n  C\n  D\n>\n  E\n");
    // And back, into the parent.
    s.move_lines(false);
    assert_eq!(show(&s), "B\n|A\n[^>>]\nC\nD");
    s.go_to(Pos { seg: root, off: 3 });
    // The last line of a segment leaves it without a line, so bubbles can
    // be next to each other.
    s.move_lines(true);
    s.go_to(Pos { seg: root, off: 1 });
    s.move_lines(true);
    assert_eq!(show(&s), "[^>>]\nB|\nA\nC\nD");
    s.move_lines(false);
    assert_eq!(show(&s), "B|\n[^>>]\nA\nC\nD");
    let mut s = Session::new(Doc::from_file("R\n>\n  F\n  >\n    D\n").unwrap());
    s.move_lines(false);
    assert_eq!(show(&s), "R\n[^>]\nF\nD|\n[^>]");
    s.go_to(s.doc().end(s.doc().root()));
    s.move_lines(true);
    s.move_lines(true);
    assert_eq!(show(&s), "[^>]\nF\nR|\nD\n[^>]");
    // Across bubbles, the bubbles' lines move with the rest.
    let mut s = Session::new(Doc::from_file("A\n>\n  B\n  C\n").unwrap());
    let b = s.tip();
    s.set_selection(Pos { seg: root, off: 0 }, Pos { seg: b, off: 1 });
    s.move_lines(true);
    assert_eq!(show(&s), "C\n«A\n[^>]\nB|»");
    s.move_lines(false);
    assert_eq!(show(&s), "«A\n[^>]\nB|»\nC");
    // But not past another bubble's line.
    let mut s = Session::new(Doc::from_file("A\n>\n  >\n    B\n").unwrap());
    s.set_caret_offset(2);
    s.move_lines(true);
    assert_eq!(show(&s), "A\n[|^>]\n[^>]\nB");
    // Several lines, by the selection.
    let mut s = Session::new(Doc::new("A\nB\nC\nD"));
    let root = s.doc().root();
    s.set_selection(Pos { seg: root, off: 2 }, Pos { seg: root, off: 4 });
    s.move_lines(false);
    assert_eq!(show(&s), "«B\n|»A\nC\nD");
    // Up to the start of a line doesn't take that line in.
    s.move_lines(true);
    assert_eq!(show(&s), "A\n«B\n|»C\nD");
    s.undo();
    s.undo();
    assert_eq!(s.doc().get(root).text, "A\nB\nC\nD");
    // At the ends, nothing.
    s.go_to(Pos { seg: root, off: 0 });
    s.move_lines(false);
    assert_eq!(show(&s), "|A\nB\nC\nD");
}

#[test]
fn moving_a_bubble_line() {
    let s = Session::new(Doc::from_file("A\nB\n>\n  C\n  D\n>\n  E\n").unwrap());
    let mut s1 = s.clone();
    s1.set_caret_offset(4);
    assert_eq!(s1.inside(), Inside::Start);
    s1.move_lines(false);
    assert_eq!(show(&s1), "A\n[|^>>]\nB\nC\nD");
    s1.move_lines(true);
    s1.move_lines(true);
    assert_eq!(show(&s1), "A\nB\nC\n[|^>>]\nD");
}

#[test]
fn moving_branches() {
    let s = Session::new(Doc::from_file("R\n>\n  A\n>\n  B\n>\n  C\n").unwrap());
    let root = s.doc().root();
    let kids = s.doc().get(root).children.clone();
    let texts = |s: &Session| {
        let d = s.doc();
        d.get(d.root())
            .children
            .iter()
            .map(|c| d.get(*c).text.clone())
            .collect::<Vec<_>>()
            .join("")
    };
    let mut s1 = s.clone();
    s1.enter(kids[0]);
    s1.move_lines(true);
    assert_eq!(
        (texts(&s1), show(&s1)),
        ("BAC".into(), "R\n[>^>|>]\nA".into())
    );
    s1.move_lines(true);
    s1.move_lines(true);
    assert_eq!(texts(&s1), "BCA");
    s1.move_lines(false);
    assert_eq!(texts(&s1), "BAC");
    // Several, by the selection.
    let mut s2 = s.clone();
    let start = mark_offset(&s2, kids[0]) - 1;
    s2.select_offsets(start, start + 2, None);
    s2.move_lines(true);
    assert_eq!(
        (texts(&s2), show(&s2)),
        ("CAB".into(), "R\n[>«>^>|»]\nB".into())
    );
    // Duplicating, with its branches, leaves the caret on the copy.
    let mut s3 = Session::new(Doc::from_file("R\n>\n  A\n  >\n    X\n>\n  B\n").unwrap());
    let root = s3.doc().root();
    let a = s3.doc().get(root).children[0];
    s3.enter(a);
    s3.duplicate_lines(true);
    assert_eq!(
        s3.doc().to_file(),
        "R\n>\n  A\n  >\n    X\n>\n  A\n  >\n    X\n>\n  B\n"
    );
    assert_eq!(s3.caret().seg, s3.doc().get(root).children[1]);
    s3.duplicate_lines(false);
    assert_eq!(s3.doc().get(root).children.len(), 4);
    assert_eq!(s3.caret().seg, s3.doc().get(root).children[1]);
}

#[test]
fn duplicating_lines() {
    let mut s = Session::new(Doc::new("A\nB C\nD"));
    let root = s.doc().root();
    s.go_to(Pos { seg: root, off: 3 });
    s.duplicate_lines(true);
    assert_eq!(show(&s), "A\nB C\nB| C\nD");
    s.duplicate_lines(false);
    assert_eq!(show(&s), "A\nB C\nB| C\nB C\nD");
    let mut s = Session::new(Doc::new("A\nB"));
    let root = s.doc().root();
    s.set_selection(Pos { seg: root, off: 0 }, Pos { seg: root, off: 3 });
    s.duplicate_lines(true);
    assert_eq!(show(&s), "A\nB\n«A\nB|»");
}

#[test]
fn files() {
    let text = "R' U'\n>\n  // cross\n  R F\n  >\n    [R: U2]\n>\n  ^(U F')\n";
    assert_eq!(Doc::from_file(text).unwrap().to_file(), text);
    assert!(Doc::from_file("R\n    >\n").is_err());
}

#[test]
fn random_editing() {
    let mut rng = Rng(4242);
    for _ in 0..3000 {
        let mut s = Session::new(Doc::default());
        for _ in 0..30 {
            let text = s.text();
            match rng.below(11) {
                0..4 => {
                    let (a, b) = (rng.off(&text), rng.off(&text));
                    let (a, b) = (
                        a.min(b),
                        if rng.below(2) == 0 {
                            a.min(b)
                        } else {
                            a.max(b)
                        },
                    );
                    if rng.below(2) == 0 {
                        s.set_caret_offset(a);
                    }
                    let text = s.text();
                    let (a, b) = (a.min(text.len()), b.min(text.len()));
                    if !text.is_char_boundary(a) || !text.is_char_boundary(b) {
                        continue;
                    }
                    let mut ins = rng.text(4);
                    if rng.below(3) == 0 {
                        ins.push_str([">", "\n>\n", "//", ">>"][rng.below(4)]);
                    }
                    let mut want = text.clone();
                    want.replace_range(a..b, &ins);
                    replace(&mut s, a..b, &ins);
                    // Away from bubbles, edits are literal, but for the line
                    // breaks around them, which are only between lines.
                    if !text[a..b].contains([OPEN, MARK, END])
                        && !want.contains('>')
                        && !s.text().contains('>')
                        && !text[..a].ends_with([OPEN, MARK, END])
                        && !text[a..].starts_with(MARK)
                    {
                        let strip = |t: &str| t.replace([OPEN, END], "");
                        let new = s.text();
                        assert_eq!(strip(&new), strip(&want));
                        assert_eq!(
                            strip(&new[..s.caret_offset()]),
                            strip(&want[..a + ins.len()])
                        );
                    }
                }
                10 => {
                    s.step_bubble(rng.below(2) == 0);
                    s.cycle_sibling(1);
                    let text = s.text();
                    let (a, b) = (rng.off(&text), rng.off(&text));
                    s.select_offsets(a, b, None);
                    match rng.below(6) {
                        0 => {
                            if let Some(clip) = s.cut() {
                                s.paste(&clip);
                            }
                        }
                        1 => s.paste([">\n  F\n", "R\n>\nU", ">"][rng.below(3)]),
                        2 => s.move_lines(rng.below(2) == 0),
                        3 => s.duplicate_lines(rng.below(2) == 0),
                        _ => {}
                    }
                }
                4 => typ(&mut s, ">"),
                5 => typ(&mut s, ">>"),
                6 => {
                    s.step_bubble(rng.below(2) == 0);
                }
                7 => s.twist("R", Normal),
                8 => {
                    s.step(rng.below(2) == 0, rng.below(2) == 0);
                    s.delete(rng.below(2) == 0);
                }
                _ => {
                    let ids: Vec<_> = s.doc().ids().collect();
                    s.select(ids[rng.below(ids.len())]);
                }
            }
            s.doc().check();
            assert!(s.doc().path(s.tip()).contains(&s.caret().seg));
            s.eval(s.caret());
            assert!(s.doc().get(s.tip()).children.is_empty());
            let mut t = s.clone();
            t.set_caret_offset(s.caret_offset());
            assert_eq!(t.caret_offset(), s.caret_offset());
            let file = s.doc().to_file();
            assert_eq!(Doc::from_file(&file).unwrap().to_file(), file);
        }
        let end = s.doc().to_file();
        for _ in 0..40 {
            s.undo();
        }
        assert_eq!(s.doc().to_file(), "\n");
        for _ in 0..40 {
            s.redo();
        }
        assert_eq!(s.doc().to_file(), end);
    }
}

fn del(s: &str, forward: bool) -> String {
    let c = s.find('|').unwrap();
    let (mut t, c) = delete(&s.replace('|', ""), c, forward).unwrap();
    t.insert(c, '|');
    t
}

#[test]
fn delete_examples() {
    for (s, want) in [
        ("R U2|", "R|"),
        ("R U| F", "R| F"),
        ("R U |F", "R |F"),
        ("R |U", "|U"),
        ("R U|2 F", "R| F"),
        ("R [U: F]|2 D", "R| D"),
        ("R [U:| F]2 D", "R| D"),
        ("R ^(|U) D", "R| D"),
        ("[R|: U]", "[|: U]"),
        ("R (|U", "R |U"),
        ("R(U)|F", "R| F"),
        ("R // hi\n|F", "R |F"),
        ("^(R)|", "|"),
    ] {
        assert_eq!(del(s, false), want, "{s}");
    }
    assert_eq!(del("R| U F", true), "R| F");
    assert_eq!(del("R |(U F) D", true), "R| D");
    assert_eq!(del("// hi|\nR F", true), "// hi\n|F");
    assert!(delete("R", 0, false).is_none() && delete("R ", 1, true).is_none());
}

#[test]
fn delete_leaves_other_tokens() {
    let mut rng = Rng(31337);
    for _ in 0..100_000 {
        let text = rng.text(14);
        let c = rng.off(&text);
        let p = parse(&text);
        let Some((new, nc)) = delete(&text, c, rng.below(2) == 0) else {
            continue;
        };
        assert!(new.is_char_boundary(nc));
        // Unmatched brackets can become matched.
        if !p.open.is_empty() || p.tokens.iter().any(|t| t.kind == TokenKind::Stray) {
            continue;
        }
        let words = |s: &str| -> Vec<String> {
            parse(s)
                .tokens
                .iter()
                .map(|t| s[t.span.clone()].to_owned())
                .collect()
        };
        let (old, new_words) = (words(&text), words(&new));
        let head = old
            .iter()
            .zip(&new_words)
            .take_while(|(a, b)| a == b)
            .count();
        let gone = old.len() - new_words.len();
        assert!(gone > 0, "{text:?} at {c} -> {new:?}");
        assert!(
            (0..=head).any(|i| old[i + gone..] == new_words[i..]),
            "{text:?} at {c} -> {new:?}"
        );
    }
}

#[test]
fn gaps() {
    let mut rng = Rng(555);
    for _ in 0..20_000 {
        let text = rng.text(14);
        let n = parse(&text).tokens.len();
        for g in 0..=n {
            assert_eq!(gap(&text, gap_offset(&text, g)), (g, false), "{text:?}");
        }
        // Stepping visits each gap once, both ways.
        let mut s = Session::new(Doc::new(&text));
        for (forward, want) in [(false, 0), (true, n)] {
            for _ in 0..n {
                s.step(forward, false);
            }
            assert_eq!(gap(&text, s.caret().off), (want, false), "{text:?}");
        }
    }
}

/// The text with the unit at `|` in brackets `«»`.
fn unit(s: &str, u: Unit) -> String {
    let c = s.find('|').unwrap();
    let t = s.replace('|', "");
    match unit_at(&t, c, u) {
        Some(r) => format!("{}«{}»{}", &t[..r.start], &t[r.clone()], &t[r.end..]),
        None => t,
    }
}

#[test]
fn unit_examples() {
    use Unit::*;
    let cases = [
        ("R |U2' F", Token, "R «U2'» F"),
        ("R U2|' F", Token, "R «U2'» F"),
        ("|2-3Rw2 F", Token, "«2-3Rw2» F"),
        // Whitespace is the nearest token on its line.
        ("R | U", Token, "R  «U»"),
        ("R\n|\nU", Token, "R\n\nU"),
        // In a comment, a word, as in any text.
        ("R |// a b\nU", Token, "R «/»/ a b\nU"),
        ("R // a |bc\nU", Token, "R // a «bc»\nU"),
        ("R // a| bc\nU", Token, "R // a« »bc\nU"),
        ("R// a b\n|U", Token, "R// a b\n«U»"),
        // A bracket or separator is its group.
        ("x |[R, U] y", Token, "x «[R, U]» y"),
        // But a separator is inside its group, so it's the insides first.
        ("x [R|, U] y", Token, "x [«R, U»] y"),
        ("x [R|, U] y", Run, "x «[R, U]» y"),
        ("x [R |: U]2 y", Run, "x «[R : U]»2 y"),
        ("x [R, U|] y", Token, "x «[R, U]» y"),
        ("x [R, U]|2 y", Token, "x «[R, U]2» y"),
        ("|^(R U", Token, "«^(R U»"),
        ("x [|R, U] y", Token, "x [«R», U] y"),
        // Runs end at brackets, separators and line breaks.
        ("x [R |U2, F D] y", Run, "x [«R U2», F D] y"),
        ("x [R U2, F |D] y", Run, "x [R U2, «F D»] y"),
        ("R U\nF |D // c\nB", Run, "R U\n«F D // c»\nB"),
        ("R U\nF D // |c\nB", Run, "R U\n«F D // c»\nB"),
        ("[R: U] // |c", Run, "[R: U] «// c»"),
        ("R (U |F) D", Run, "R («U F») D"),
        // A bracket's run is the group's insides.
        ("R |( U F ) D", Run, "R ( «U F» ) D"),
        ("x |[R U, F D] y", Run, "x [«R U, F D»] y"),
        ("|// c", Run, "«// c»"),
    ];
    for (input, u, want) in cases {
        assert_eq!(unit(input, u), want, "{input:?} {u:?}");
    }
}

#[test]
fn close_groups_examples() {
    let t = "x [R, (U F)] y";
    let close = |a: &str, b: &str| {
        let r = close_groups(t, t.find(a).unwrap()..t.find(b).unwrap() + b.len());
        t[r].to_owned()
    };
    assert_eq!(close("x", "R"), "x [R, (U F)]");
    assert_eq!(close("R", "U"), "R, (U F)");
    assert_eq!(close("R", ","), "R,");
    assert_eq!(close("F", "y"), "[R, (U F)] y");
    assert_eq!(close("U", "F"), "U F");
    assert_eq!(close("F)", "F)]"), "[R, (U F)]");
}

#[test]
fn units_hold() {
    let mut rng = Rng(777);
    for _ in 0..20_000 {
        let text = rng.text(14);
        let off = rng.off(&text);
        for u in [Unit::Token, Unit::Run] {
            if let Some(r) = unit_at(&text, off, u) {
                assert!(text.get(r.clone()).is_some(), "{text:?} {off} {u:?}");
                let c = close_groups(&text, r.clone());
                assert_eq!(c, r, "units are closed: {text:?} {off} {u:?}");
            }
        }
        let (a, b) = (rng.off(&text), rng.off(&text));
        let r = a.min(b)..a.max(b);
        let c = close_groups(&text, r.clone());
        assert!(
            c.start <= r.start && r.end <= c.end && text.get(c.clone()).is_some(),
            "{text:?}"
        );
        assert_eq!(close_groups(&text, c.clone()), c, "{text:?}");
    }
}

#[test]
fn selecting_units_across_bubbles() {
    let mut s = Session::new(Doc::new("R [U, F] >D B> L"));
    let root = s.doc().root();
    // From `R`, a drag to the `F` takes in its group.
    let r = s.unit_at(Pos { seg: root, off: 0 }, Unit::Token).unwrap();
    let f = s.unit_at(Pos { seg: root, off: 6 }, Unit::Token).unwrap();
    s.select_units(r, f);
    assert_eq!(s.anchor().off, 0);
    assert_eq!(s.caret().off, 8);
    // To `L`, two bubbles on.
    let l_off = s.text().find('L').unwrap();
    let l = s.unit_at_offset(l_off, Unit::Token).unwrap();
    s.select_units(r, l);
    assert_eq!(s.offset_of(s.anchor()), Some(0));
    assert_eq!(s.caret_offset(), s.text().len());
    // Backwards from `B`: the caret goes at the start.
    let b = s
        .unit_at_offset(s.text().find('B').unwrap(), Unit::Token)
        .unwrap();
    let u = s
        .unit_at_offset(s.text().find('U').unwrap(), Unit::Token)
        .unwrap();
    s.select_units(b, u);
    assert_eq!(s.caret_offset(), 2);
    assert_eq!(
        s.offset_of(s.anchor()),
        Some(s.text().find('B').unwrap() + 1)
    );
    // A bubble's unit is the bubble.
    let (start, end) = s
        .unit_at_offset(s.bubbles()[0].range.start, Unit::Token)
        .unwrap();
    let seg = s.bubbles()[0].seg;
    assert_eq!((s.offset_of(start), end), (Some(8), Pos { seg, off: 0 }));
}

#[test]
fn stepping_by_runs_and_segments() {
    let text = "R U [F, D B] L\nB' D2";
    let mut s = Session::new(Doc::new(text));
    let root = s.doc().root();
    s.go_to(Pos { seg: root, off: 0 });
    let stops = [3, 5, 6, 7, 11, 12, 14, text.len()];
    for &want in &stops {
        s.step_run(true, false);
        assert_eq!(s.caret().off, want);
    }
    for &want in stops.iter().rev().skip(1).chain(&[0]) {
        s.step_run(false, false);
        assert_eq!(s.caret().off, want);
    }
    // From partway through a token, to the start of its run.
    s.go_to(Pos { seg: root, off: 16 });
    s.step_run(false, false);
    assert_eq!(s.caret().off, 14);

    // Segments, across bubbles.
    let mut s = Session::new(Doc::new("R U >F D> B"));
    let segs = s.doc().path(s.tip());
    s.go_to(Pos {
        seg: segs[1],
        off: 1,
    });
    s.step_segment(true, false);
    assert_eq!(
        s.caret(),
        Pos {
            seg: segs[1],
            off: 3
        }
    );
    s.step_segment(true, false);
    assert_eq!(
        s.caret(),
        Pos {
            seg: segs[2],
            off: 1
        }
    );
    s.step_segment(true, false);
    assert_eq!(
        s.caret(),
        Pos {
            seg: segs[2],
            off: 1
        }
    );
    s.step_segment(false, true);
    assert_eq!(
        s.caret(),
        Pos {
            seg: segs[2],
            off: 0
        }
    );
    s.step_segment(false, true);
    assert_eq!(
        s.caret(),
        Pos {
            seg: segs[1],
            off: 0
        }
    );
    assert_eq!(
        s.anchor(),
        Pos {
            seg: segs[2],
            off: 1
        }
    );
    s.step_segment(false, false);
    s.step_segment(false, false);
    assert_eq!(s.caret(), Pos { seg: root, off: 0 });
}

#[test]
fn stepping_by_runs_visits_gaps() {
    let mut rng = Rng(999);
    for _ in 0..5_000 {
        let text = rng.text(14);
        let n = parse(&text).tokens.len();
        let mut s = Session::new(Doc::new(&text));
        let root = s.doc().root();
        s.go_to(Pos {
            seg: root,
            off: rng.off(&text),
        });
        for (forward, want) in [(true, n), (false, 0)] {
            let mut last = s.caret();
            for _ in 0..=n {
                s.step_run(forward, false);
                assert!(!gap(&text, s.caret().off).1, "{text:?}");
                if s.caret() == last {
                    break;
                }
                last = s.caret();
            }
            assert_eq!(gap(&text, s.caret().off), (want, false), "{text:?}");
        }
    }
}

#[test]
fn deleting_by_runs_and_segments() {
    let mut s = Session::new(Doc::new("R U [F, D B] L\nB' D2"));
    let root = s.doc().root();
    let text = |s: &Session| s.doc().get(root).text.clone();
    s.delete_to(false, false);
    assert_eq!(text(&s), "R U [F, D B] L\n");
    s.go_to(Pos { seg: root, off: 12 });
    // A bracket takes its group with it.
    s.delete_to(false, false);
    assert_eq!((text(&s).as_str(), s.caret().off), ("R U L\n", 3));
    s.delete_to(true, false);
    assert_eq!((text(&s).as_str(), s.caret().off), ("R U\n", 3));
    s.go_to(Pos { seg: root, off: 0 });
    s.delete_to(true, true);
    assert_eq!(text(&s), "\n");
    s.undo();
    assert_eq!(text(&s), "R U\n");

    // Backwards keeps the line break before the run.
    let mut s = Session::new(Doc::new("X\nR U"));
    s.delete_to(false, false);
    assert_eq!(s.doc().get(s.doc().root()).text, "X\n");

    // From a segment's start, it deletes the fork, like Backspace.
    let mut s = Session::new(Doc::new("R U >F D"));
    let segs = s.doc().path(s.tip());
    s.delete_to(false, true);
    assert_eq!(s.doc().get(segs[1]).text, "");
    s.delete_to(false, true);
    assert_eq!(s.doc().to_file(), "R U\n");
}

#[test]
fn joining_leaves_a_line_break() {
    // Only between two lines.
    for (file, want) in [
        ("R U\n>\n  F D\n", "R U\nF D\n"),
        ("R U \n>\n   F D\n", "R U \n F D\n"),
        ("R U \n>\n", "R U \n"),
        ("(R\n>\n  )\n", "(R\n)\n"),
        ("R U // c\n>\n  F\n", "R U // c\nF\n"),
    ] {
        let mut s = Session::new(Doc::from_file(file).unwrap());
        s.join();
        assert_eq!(s.doc().to_file(), want, "{file:?}");
        s.doc().check();
    }
    // The caret stays with its token, here before the `D`.
    let mut s = Session::new(Doc::from_file("R U\n>\n  F D\n").unwrap());
    s.go_to(Pos {
        seg: s.tip(),
        off: 2,
    });
    s.join();
    assert_eq!(s.caret().off, 6);
    // And at the end of the parent, deleting forward.
    let mut s = Session::new(Doc::from_file("R U\n>\n  F D\n").unwrap());
    let root = s.doc().root();
    s.go_to(Pos { seg: root, off: 3 });
    s.delete(true);
    assert_eq!(
        (s.doc().get(root).text.as_str(), s.caret().off),
        ("R U\nF D", 3)
    );
}

#[test]
fn fork_then_join_keeps_the_moves() {
    let mut rng = Rng(31337);
    for _ in 0..10_000 {
        let text = rng.text(12);
        let off = rng.off(&text);
        // Forking between a group and its multiplier is a user error, like a
        // twist there: the multiplier is left on its own.
        if parse(&text)
            .tokens
            .iter()
            .any(|t| t.kind == TokenKind::Multiplier && t.span.start == off)
        {
            continue;
        }
        let mut s = Session::new(Doc::new(&text));
        s.go_to(Pos {
            seg: s.doc().root(),
            off,
        });
        s.fork();
        let (forked, inside) = (s.caret().seg, s.inside());
        assert_eq!(inside, Inside::Mark, "{text:?}");
        assert!(
            s.doc().get(forked).text.trim_start() == s.doc().get(forked).text,
            "{text:?}"
        );
        s.join();
        s.doc().check();
        let joined = &s.doc().get(s.doc().root()).text;
        let tokens = |t: &str| {
            let p = parse(t);
            (
                p.tokens
                    .iter()
                    .map(|k| t[k.span.clone()].to_owned())
                    .collect::<Vec<_>>(),
                p.op,
            )
        };
        let (a, b) = (tokens(&text), tokens(joined));
        assert_eq!(a.0, b.0, "{text:?} -> {joined:?}");
        assert!(a.1 == b.1, "{text:?} -> {joined:?}");
    }
}

#[test]
fn brackets_wrap_the_selection() {
    let mut s = Session::new(Doc::new("R U F"));
    let root = s.doc().root();
    let at = |off| Pos { seg: root, off };
    s.set_selection(at(2), at(5));
    assert!(s.wrap("(", ")"));
    assert_eq!(s.doc().get(root).text, "R (U F)");
    assert_eq!((s.anchor(), s.caret()), (at(3), at(6)));
    // Backward, and again, keeping the direction.
    s.set_selection(at(6), at(3));
    assert!(s.wrap("[", "]"));
    assert_eq!(s.doc().get(root).text, "R ([U F])");
    assert_eq!((s.anchor(), s.caret()), (at(7), at(4)));
    // Undo and redo keep the selection.
    s.undo();
    assert_eq!(s.doc().get(root).text, "R (U F)");
    assert_eq!((s.anchor(), s.caret()), (at(6), at(3)));
    s.redo();
    assert_eq!((s.anchor(), s.caret()), (at(7), at(4)));
    s.undo();
    // `^` wraps for NISS.
    assert!(s.wrap("^(", ")"));
    assert_eq!(s.doc().get(root).text, "R (^(U F))");
    assert_eq!((s.anchor(), s.caret()), (at(8), at(5)));
    s.undo();
    // So does undoing typing over a selection.
    s.set_selection(at(3), at(6));
    let mut t = s.text();
    t.replace_range(3..6, "x");
    s.edit(&t, 4);
    s.undo();
    assert_eq!((s.anchor(), s.caret()), (at(3), at(6)));
    // Without a selection, it does nothing.
    s.go_to(at(0));
    assert!(!s.wrap("(", ")"));
    assert_eq!(s.doc().get(root).text, "R (U F)");
}

#[test]
fn branch_starts() {
    // Empty segments have no line, so bubbles can be on adjacent lines.
    let s = Session::new(Doc::from_file("R\n>\n  >\n    F\n>\n").unwrap());
    assert_eq!(show(&s), "R\n[^>>]\n[^>]\nF|");
    let full = Session::new(Doc::from_file("R\n>\n  A\n>\n  B\n").unwrap());
    let root = full.doc().root();
    let a = full.doc().get(root).children[0];
    // Backspace at a branch's start, in twist mode too, deletes its `>`.
    let mut s = full.clone();
    s.enter(a);
    s.delete(false);
    assert_eq!(show(&s), "R\n[|^>]\nB");
    s.delete(false);
    assert_eq!(show(&s), "R\n[|^>]\nB");
    // The only one keeps its text.
    s.step(true, false);
    s.delete(false);
    assert_eq!(show(&s), "R\n|B");
    // Cmd+Enter there adds a branch after it.
    let mut s = full.clone();
    s.enter(a);
    s.fork();
    assert_eq!(show(&s), "R\n[>^>|>]");
    // Stepping by tokens, as on the timeline, a branch's start is after its
    // `>`.
    let mut s = full.clone();
    s.go_to(s.doc().end(root));
    s.step(true, false);
    assert_eq!(show(&s), "R\n[^>|>]\nA");
    s.move_lines(true);
    assert_eq!(show(&s), "R\n[>^>|]\nA");
    // Down onto a bubble's line goes after the active `>`.
    let mut s = full.clone();
    s.enter(a);
    let o = s.caret_offset();
    assert_eq!(s.key_offset(o - 1, true), o);
    // A selection from there stays on its line.
    s.select_offsets(o, o + 3, Some(true));
    assert_eq!(show(&s), "R\n[>«^>|»]\nB");
    // The start of a branch's first line isn't after its `>`, so there,
    // Option+Down moves the line.
    let mut s = Session::new(Doc::from_file("R\n>\n  A\n  B\n").unwrap());
    s.go_to(Pos {
        seg: s.tip(),
        off: 0,
    });
    assert_eq!(show(&s), "R\n[^>]\n|A\nB");
    s.move_lines(true);
    assert_eq!(show(&s), "R\n[^>]\nB\n|A");
    s.move_lines(false);
    s.move_lines(false);
    assert_eq!(show(&s), "R\n|A\n[^>]\nB");
    s.move_lines(true);
    assert_eq!(show(&s), "R\n[^>]\n|A\nB");
}

fn inv(s: &str) -> Option<String> {
    invert(s, 0..s.len()).map(|(t, _)| t)
}
fn exp(s: &str) -> Option<String> {
    expand(s, 0..s.len()).map(|(t, _)| t)
}

#[test]
fn inverting() {
    assert_eq!(inv("R U2 F'").unwrap(), "F U2' R'");
    assert_eq!(inv("[R, U] [F: D]2").unwrap(), "[F: D']2 [U, R]");
    assert_eq!(inv("^(R U) (F .)3'").unwrap(), "(. F')3' ^(U' R')");
    // Lines go in reverse order, each keeping its comment.
    assert_eq!(inv("R U // a\n\nF // b").unwrap(), "F' // b\n\nU' R' // a");
    // A group that isn't all in it, a cut token, or nothing, can't be.
    assert_eq!(invert("(R U) F", 1..7), None);
    assert_eq!(invert("[R, U]", 1..5), None);
    assert_eq!(invert("R2 U", 1..4), None);
    assert_eq!(invert("R  ", 1..3), None);
    assert_eq!(inv("R Q"), None);
    // A range up to a multiplier takes it in, but keeps its end.
    assert_eq!(
        invert("[R, U]2 F", 0..6),
        Some(("[U, R]2 F".to_owned(), 0..6))
    );
    assert_eq!(invert("(R U)2", 1..4), Some(("(U' R')2".to_owned(), 1..6)));
    // Within a group, and keeping the whitespace around it.
    assert_eq!(
        invert("(R U) F", 1..4),
        Some(("(U' R') F".to_owned(), 1..6))
    );
    assert_eq!(invert("F R U ", 1..6), Some(("F U' R' ".to_owned(), 1..8)));
}

#[test]
fn inverting_undoes() {
    let mut rng = Rng(7);
    let mut n = 0;
    for i in 0..4000 {
        let s = if i % 2 == 0 {
            rng.text(12)
        } else {
            rng.notation(3, true)
        };
        let Some(t) = inv(&s) else { continue };
        n += 1;
        let (a, b) = (parse(&s).op, parse(&t).op);
        assert_eq!(b.left, a.left.inv(), "{s:?} -> {t:?}");
        assert_eq!(b.right, a.right.inv(), "{s:?} -> {t:?}");
        assert_eq!(inv(&t).map(|u| parse(&u).op), Some(a), "{s:?} -> {t:?}");
    }
    assert!(n > 200, "{n}");
}

#[test]
fn expanding() {
    assert_eq!(exp("[R, U]").unwrap(), "R U R' U'");
    assert_eq!(exp("[R: U]").unwrap(), "R U R'");
    // A multiplier in the range is written out, and one that isn't stays.
    assert_eq!(exp("[R: U]2").unwrap(), "R U R' R U R'");
    assert_eq!(exp("[R, U]'").unwrap(), "U R U' R'");
    assert_eq!(exp("[R, [U, F]2]0").unwrap(), "");
    assert_eq!(
        expand("F [R U: [D, B]2]2' L", 2..16),
        Some(("F (R U D B D' B' D B D' B' U' R')2' L".to_owned(), 2..33))
    );
    assert_eq!(exp("(R [U, D])").unwrap(), "(R U D U' D')");
    assert_eq!(exp("R[U,F]D").unwrap(), "R U F U' F' D");
    assert_eq!(exp("R (U)"), None);
    // Only groups all in the range.
    assert_eq!(
        expand("[R, U] [F: D]", 0..9),
        Some(("R U R' U' [F: D]".to_owned(), 0..12))
    );
    assert_eq!(
        expand("[R, U] [F: D]", 1..13),
        Some(("[R, U] F D F'".to_owned(), 1..13))
    );
    // A comment at the end of a side keeps its line break.
    assert_eq!(exp("[R // a\n, U] F").unwrap(), "R // a\nU R' // a\nU' F");
}

#[test]
fn expanding_keeps_the_state() {
    let mut rng = Rng(11);
    let mut n = 0;
    for i in 0..4000 {
        let s = if i % 2 == 0 {
            rng.text(12)
        } else {
            rng.notation(3, true)
        };
        let Some(t) = exp(&s) else { continue };
        n += 1;
        assert_eq!(parse(&t).op, parse(&s).op, "{s:?} -> {t:?}");
    }
    assert!(n > 500, "{n}");
}

#[test]
fn commutators_from_the_selection() {
    let mut s = Session::new(Doc::new("F R U D"));
    let root = s.doc().root();
    let at = |off| Pos { seg: root, off };
    // The separator goes at the gap nearest the click.
    s.set_selection(at(2), at(7));
    assert!(s.wrap_binary(',', at(5)));
    assert_eq!(s.doc().get(root).text, "F [R U, D]");
    assert_eq!((s.anchor(), s.caret()), (at(3), at(9)));
    s.undo();
    s.set_selection(at(7), at(2));
    assert!(s.wrap_binary(':', at(3)));
    assert_eq!(s.doc().get(root).text, "F [R: U D]");
    assert_eq!((s.anchor(), s.caret()), (at(9), at(3)));
    s.undo();
    // Not in a group that the selection has all of, nor off the selection.
    s.replace(Doc::new("F (R U) D"));
    let root = s.doc().root();
    let at = |off| Pos { seg: root, off };
    s.set_selection(at(2), at(9));
    assert!(s.wrap_binary(',', at(5)));
    assert_eq!(s.doc().get(root).text, "F [(R U), D]");
    s.undo();
    s.set_selection(at(2), at(9));
    assert!(s.wrap_binary(',', at(0)));
    assert_eq!(s.doc().get(root).text, "F [, (R U) D]");
    // Without a selection, nothing.
    s.go_to(at(0));
    assert!(!s.wrap_binary(',', at(0)));
}

#[test]
fn fmc_tools_across_forks() {
    let mut s = Session::new(Doc::from_file("R [U, F]\n>\n  [D: B] L\n").unwrap());
    let root = s.doc().root();
    let child = s.doc().get(root).children[0];
    s.set_selection(Pos { seg: root, off: 2 }, Pos { seg: child, off: 6 });
    assert!(s.can_expand() && !s.can_invert());
    s.expand();
    assert_eq!(s.doc().to_file(), "R U F U' F'\n>\n  D B D' L\n");
    assert_eq!(s.anchor(), Pos { seg: root, off: 2 });
    assert_eq!(s.caret(), Pos { seg: child, off: 6 });
    assert!(!s.can_expand());
    // A commutator's separator can go in another segment.
    s.set_selection(Pos { seg: root, off: 0 }, Pos { seg: child, off: 3 });
    assert!(s.wrap_binary(',', Pos { seg: child, off: 1 }));
    assert_eq!(s.doc().to_file(), "[R U F U' F'\n>\n  D, B] D' L\n");
    // Inverting in one segment.
    s.set_selection(
        Pos { seg: child, off: 6 },
        Pos {
            seg: child,
            off: 10,
        },
    );
    assert!(s.can_invert());
    s.invert();
    assert_eq!(s.doc().to_file(), "[R U F U' F'\n>\n  D, B] L' D\n");
}

#[test]
fn double_click_leaves_out_the_multiplier() {
    let t = "R [U, F]2' D";
    // On a bracket, the group without its multiplier, and on that, all of it.
    for off in [2, 7] {
        assert_eq!(unit_at(t, off, Unit::Token), Some(2..8), "{off}");
    }
    assert_eq!(unit_at(t, 5, Unit::Run), Some(2..8));
    for off in [8, 9] {
        assert_eq!(unit_at(t, off, Unit::Token), Some(2..10), "{off}");
    }
    // Covering the brackets is enough to keep a range from growing.
    assert_eq!(close_groups(t, 2..8), 2..8);
    assert_eq!(close_groups(t, 4..9), 2..10);
    // A range up to the closing bracket leaves the multiplier out.
    assert_eq!(close_groups(t, 6..8), 2..8);
}

#[test]
fn multipliers_are_tokens() {
    let t = "R [U, F]2' D";
    let words: Vec<_> = parse(t).tokens.iter().map(|k| &t[k.span.clone()]).collect();
    assert_eq!(words, ["R", "[", "U", ",", "F", "]", "2'", "D"]);
    // Between the bracket and the multiplier is the state after one pass.
    let st = |s: &str| parse(s).op.right;
    assert_eq!(st(&t[..8]), st("R [U, F]"));
    // Deleting the multiplier leaves the group, and deleting a bracket takes
    // the multiplier too.
    assert_eq!(del("R [U, F]2'| D", false), "R [U, F]| D");
    assert_eq!(del("R [U, F]|2' D", false), "R| D");
    // A twist there is a user error: it splits them, as in plain text.
    let (new, _) = twist("[U, F]2", 6, "R", Normal);
    assert_eq!(new, "[U, F] R 2");
    assert!(
        parse(&new)
            .tokens
            .iter()
            .any(|t| t.kind == TokenKind::Error)
    );
}

/// The text with the selection in `«»`, or the caret as `|`. Bubbles are `>`.
fn shown(s: &Session) -> String {
    let t: String = s
        .text()
        .chars()
        .map(|c| match c {
            MARK => '>',
            OPEN | END => '\n',
            c => c,
        })
        .collect();
    let (a, c) = (s.anchor_offset(), s.caret_offset());
    let (lo, hi) = (a.min(c), a.max(c));
    if lo == hi {
        format!("{}|{}", &t[..lo], &t[lo..])
    } else {
        format!("{}«{}»{}", &t[..lo], &t[lo..hi], &t[hi..])
    }
}

/// A session with the caret at the `|` in `text`.
fn at_caret(text: &str) -> Session {
    let off = text.find('|').unwrap();
    let mut s = Session::new(Doc::new(&text.replace('|', "")));
    s.set_caret_offset(off);
    s
}

/// What each press of Shift+Option+an arrow selects.
fn presses(s: &mut Session, keys: &str, runs: bool) -> Vec<String> {
    keys.chars()
        .map(|k| {
            s.select_by_meaning(k == '>', runs);
            shown(s)
        })
        .collect::<Vec<_>>()
}

#[test]
fn selecting_by_meaning() {
    let mut s = at_caret("[U|' R : F]2' U' (R' L)2");
    let grown = [
        "[«U'» R : F]2' U' (R' L)2",
        "[«U' R» : F]2' U' (R' L)2",
        "[«U' R : F»]2' U' (R' L)2",
        "«[U' R : F]»2' U' (R' L)2",
        "«[U' R : F]2'» U' (R' L)2",
        "«[U' R : F]2' U'» (R' L)2",
        "«[U' R : F]2' U' (R' L)»2",
        "«[U' R : F]2' U' (R' L)2»",
    ];
    assert_eq!(presses(&mut s, ">>>>>>>>", false), grown);
    // At the end, nothing more.
    assert_eq!(presses(&mut s, ">", false), [grown[7]]);
    // Toward the anchor, each step undoes one.
    let mut back: Vec<_> = grown[..7].iter().rev().map(|s| s.to_string()).collect();
    back.push("[|U' R : F]2' U' (R' L)2".to_owned());
    assert_eq!(presses(&mut s, "<<<<<<<<", false), back);

    // The first press always grows, either way. With no sibling that way,
    // it's all of the siblings, which can grow the other way too, and then
    // the parent.
    let mut s = at_caret("[U|' R : F]2' U' (R' L)2");
    assert_eq!(
        presses(&mut s, "<<<<>", false),
        [
            "[«U'» R : F]2' U' (R' L)2",
            "[«U' R» : F]2' U' (R' L)2",
            "[«U' R : F»]2' U' (R' L)2",
            "«[U' R : F]»2' U' (R' L)2",
            "[«U' R : F»]2' U' (R' L)2",
        ]
    );
    // From the middle of the right side.
    let mut s = at_caret("[U' : R |F D]");
    assert_eq!(
        presses(&mut s, ">>>>", false),
        [
            "[U' : R «F» D]",
            "[U' : R «F D»]",
            "[U' : «R F D»]",
            "[«U' : R F D»]",
        ]
    );
}

#[test]
fn selecting_by_meaning_ignores_line_breaks() {
    let mut s = at_caret("|R\n[U,\nF]\n// c\nD");
    assert_eq!(
        presses(&mut s, ">>>>", false),
        [
            "«R»\n[U,\nF]\n// c\nD",
            "«R\n[U,\nF]»\n// c\nD",
            "«R\n[U,\nF]\n// c»\nD",
            "«R\n[U,\nF]\n// c\nD»",
        ]
    );
    let mut s = at_caret("[U,\n|F]");
    assert_eq!(presses(&mut s, "<<", false), ["[U,\n«F»]", "[«U,\nF»]"]);
}

#[test]
fn selecting_by_meaning_snaps_first() {
    // A selection that isn't semantic snaps to the smallest one that covers
    // it, whichever way.
    let mut s = Session::new(Doc::new("R [U F, D] B"));
    let root = s.doc().root();
    s.set_selection(Pos { seg: root, off: 1 }, Pos { seg: root, off: 6 });
    assert_eq!(presses(&mut s, "<", false), ["R «[U F, D]» B"]);
    s.set_selection(Pos { seg: root, off: 5 }, Pos { seg: root, off: 8 });
    assert_eq!(
        presses(&mut s, ">>", false),
        ["R [«U F, D»] B", "R «[U F, D]» B"]
    );
    // Whitespace at the edges doesn't count.
    s.set_selection(Pos { seg: root, off: 0 }, Pos { seg: root, off: 2 });
    assert_eq!(presses(&mut s, ">", false), ["«R [U F, D]» B"]);
    // A click's selection has no direction.
    let u = s.unit_at(Pos { seg: root, off: 3 }, Unit::Token).unwrap();
    s.select_units(u, u);
    assert_eq!(presses(&mut s, "<", false), ["R [«U F», D] B"]);
}

#[test]
fn selecting_by_meaning_crosses_bubbles() {
    let mut s = Session::new(Doc::new("R U >F D> L"));
    let root = s.doc().root();
    s.go_to(Pos { seg: root, off: 2 });
    assert_eq!(
        presses(&mut s, ">>>>", false),
        [
            "R «U»\n>\nF D\n>\nL",
            "R «U\n>\nF» D\n>\nL",
            "R «U\n>\nF D»\n>\nL",
            "R «U\n>\nF D\n>\nL»",
        ]
    );
    assert_eq!(
        presses(&mut s, "<<", false),
        ["R «U\n>\nF D»\n>\nL", "R «U\n>\nF» D\n>\nL"]
    );
}

#[test]
fn selecting_by_meaning_by_runs() {
    let mut s = at_caret("R |U F [D, B] L\nB' D2");
    assert_eq!(
        presses(&mut s, ">>>>>><", true),
        [
            "R «U F» [D, B] L\nB' D2",
            "R «U F [D, B]» L\nB' D2",
            "R «U F [D, B] L»\nB' D2",
            "R «U F [D, B] L\nB' D2»",
            // At the end of the path, all of it.
            "«R U F [D, B] L\nB' D2»",
            "«R U F [D, B] L\nB' D2»",
            "«R U F [D, B] L»\nB' D2",
        ]
    );
}

#[test]
fn selections_by_meaning_stay_semantic() {
    let mut rng = Rng(4242);
    for _ in 0..3_000 {
        let text = rng.text(14);
        let mut s = Session::new(Doc::new(&text));
        s.set_caret_offset(rng.off(&text));
        let runs = rng.below(2) == 0;
        for _ in 0..8 {
            s.select_by_meaning(rng.below(2) == 0, runs);
            let (a, c) = (s.anchor_offset(), s.caret_offset());
            let t = s.text();
            assert!(t.is_char_boundary(a) && t.is_char_boundary(c), "{text:?}");
            // Already semantic, so the next press doesn't just snap. Keep
            // going the same way and it's a superset.
            let (lo, hi) = (a.min(c), a.max(c));
            if lo == hi {
                continue;
            }
            let mut t2 = s.clone();
            t2.select_by_meaning(c >= a, runs);
            let (a2, c2) = (t2.anchor_offset(), t2.caret_offset());
            assert!(
                a2.min(c2) <= lo && hi <= a2.max(c2),
                "{text:?}: {lo}..{hi} grew to {}..{}",
                a2.min(c2),
                a2.max(c2)
            );
        }
    }
}

#[test]
fn selecting_by_meaning_in_a_bubble() {
    let doc = Doc::from_file("R U\n>\n  D\n>\n  F\n>\n  L\n").unwrap();
    let b = Session::new(doc.clone()).bubbles()[0].clone();
    // In text, Shift+Option+arrows on a bubble's line are Shift+Cmd+arrows.
    for off in b.marks() {
        for keys in ["<", ">", "<<", ">>", "<>", "><", ">>>"] {
            let mut s = Session::new(doc.clone());
            s.set_caret_offset(off);
            let mut t = s.clone();
            for k in keys.chars() {
                s.select_by_meaning(k == '>', false);
                t.step_line(k == '>', true);
            }
            assert_eq!(
                (s.anchor_offset(), s.caret_offset()),
                (t.anchor_offset(), t.caret_offset()),
                "{off} {keys}"
            );
        }
    }
    // In twist mode, right after a `>` is the branch's start.
    let mut s = Session::new(doc.clone());
    s.set_caret_offset(b.line + 2);
    s.select_by_meaning(true, true);
    assert_eq!(shown(&s), "R U\n>>>\n«F»");

    // Shift+arrow at the end of a bubble's line selects all of it, and
    // elsewhere doesn't.
    for (off, forward) in [(b.line + b.n, true), (b.line, false)] {
        let mut s = Session::new(doc.clone());
        s.set_caret_offset(off);
        assert!(s.select_bubble_at_edge(forward));
        let (a, c) = (s.anchor_offset(), s.caret_offset());
        assert_eq!((a.min(c), a.max(c)), (b.line, b.line + b.n), "{off}");
        assert_eq!(c, off);
    }
    for (off, forward) in [(b.line + 1, true), (b.line + 1, false), (b.line, true)] {
        let mut s = Session::new(doc.clone());
        s.set_caret_offset(off);
        assert!(!s.select_bubble_at_edge(forward), "{off} {forward}");
    }
}
