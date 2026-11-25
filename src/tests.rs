use crate::doc::{Doc, Pos};
use crate::notation::{Inversion::*, *};
use crate::puzzle::Perm;
use crate::session::Session;

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
        (0..self.below(max + 1)).map(|_| BITS[self.below(BITS.len())]).collect()
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
        let want = parse(&text[..c]).op.then(&Op::new(parse_move(mv).unwrap(), inv));
        assert_eq!(parse(&new[..nc]).op, want, "{text:?} at {c} -> {new:?} at {nc}");
    }
}

#[test]
fn cut_and_fork_preserve_state() {
    let mut rng = Rng(77);
    for _ in 0..100_000 {
        let text = rng.text(14);
        let c = rng.off(&text);
        let k = cut(&text, c);
        let msg = format!("{text:?} at {c}: {k:?}");
        // Cutting through a token is the user's choice and changes the rest.
        if c == 0 || c == text.len() || text[c..].starts_with(char::is_whitespace) {
            let both = parse(&k.left).op.then(&parse(&k.right).op);
            assert_eq!(both, parse(&text).op, "{msg}");
        }
        let mut s = Session::new(Doc::new(&text));
        s.go_to(Pos { seg: s.doc().root(), off: c });
        let before = s.eval(s.caret());
        s.fork();
        s.doc().check();
        // At the end of the segment, forking keeps the whole text.
        if !k.right.trim().is_empty() {
            assert_eq!(s.eval(s.caret()), before, "{msg}");
        }
    }
}

#[test]
fn fork_examples() {
    let mut s = Session::new(Doc::new("F2 [R: U D] L"));
    s.go_to(Pos { seg: s.doc().root(), off: 8 });
    s.fork();
    s.twist("F", Normal);
    assert_eq!(s.doc().to_file(), "F2 \n>\n  [R: U D] L\n>\n  R U F\n");
    let mut s = Session::new(Doc::new("^(A B)"));
    s.go_to(Pos { seg: s.doc().root(), off: 3 });
    s.split();
    assert_eq!(s.doc().to_file(), "^(A)\n>\n  ^( B)\n");
    s.undo();
    assert_eq!(s.doc().to_file(), "^(A B)\n");
}

#[test]
fn forking_beside_a_fork() {
    // Right beside it, on either side, adds a branch to it.
    for (text, at) in [("R>U", 1), ("R>U", 2), ("R >U", 1), ("R> U", 3), ("R>", 2), ("R>", 1)] {
        let mut s = Session::new(Doc::new(text));
        s.go_to(s.doc().path_pos(s.tip(), at));
        s.fork();
        assert_eq!(s.doc().get(s.doc().root()).children.len(), 2, "{text:?} at {at}");
        assert_eq!(s.doc().ids().count(), 3, "{text:?} at {at}");
    }
    // But not from another line.
    for (text, at, want) in [("R\n>U", 1, "R>\n>U"), ("R>\nU", 3, "R>\n>U"), ("R>\n", 3, "R>\n>")] {
        let mut s = Session::new(Doc::new(text));
        s.go_to(s.doc().path_pos(s.tip(), at));
        s.fork();
        s.cycle_sibling(-1);
        let mut tip = s.tip();
        while let Some(c) = s.doc().get(tip).children.first() {
            tip = *c;
        }
        assert_eq!(s.doc().path_text(tip), want);
        assert_eq!(s.doc().get(s.doc().root()).children.len(), if at == 1 { 2 } else { 1 });
    }
}

/// Edits like an editor would: replace a range of the path text.
fn replace(s: &mut Session, r: std::ops::Range<usize>, with: &str) {
    let mut t = s.doc().path_text(s.tip());
    t.replace_range(r.clone(), with);
    s.edit(&t, r.start + with.len());
    s.doc().check();
}

#[test]
fn editing_forks() {
    let mut s = Session::new(Doc::new("R U"));
    replace(&mut s, 1..1, ">");
    assert_eq!(s.doc().to_file(), "R\n>\n   U\n");
    assert_eq!(s.eval(s.caret()).0, st("R"));
    replace(&mut s, 1..2, "");
    assert_eq!(s.doc().to_file(), "R U\n");
    // Its children join its siblings, in its place.
    let file = "R\n>\n  A\n>\n  U\n  >\n    F\n  >\n    D\n>\n  B\n";
    let mut s = Session::new(Doc::from_file(file).unwrap());
    let u = s.doc().ids().find(|id| s.doc().get(*id).text == "U").unwrap();
    s.select(u);
    replace(&mut s, 1..2, "");
    assert_eq!(s.doc().to_file(), "RU\n>\n  A\n>\n  F\n>\n  D\n>\n  B\n");
    assert_eq!(s.caret().off, 1);
    s.undo();
    assert_eq!(s.doc().to_file(), file);
    // By tokens, it keeps them separate.
    for (seg, off, forward) in [(u, 0, false), (s.doc().root(), 1, true)] {
        let mut s = s.clone();
        s.go_to(Pos { seg, off });
        s.delete(forward);
        assert!(s.doc().to_file().starts_with("R U\n>\n  A\n>\n  F\n"));
    }
    // Empty segments.
    let mut s = Session::new(Doc::new("R"));
    replace(&mut s, 1..1, ">>\nF");
    assert_eq!(s.doc().path(s.tip()).len(), 3);
    assert_eq!(s.doc().path_text(s.tip()), "R>>\nF");
}

#[test]
fn forks_in_comments() {
    let mut s = Session::new(Doc::new("R // a > b\nU>F"));
    assert_eq!(s.doc().path(s.tip()).len(), 2);
    // Commenting out a fork, and uncommenting it.
    replace(&mut s, 12..12, "//");
    assert_eq!(s.doc().path(s.tip()).len(), 1);
    replace(&mut s, 12..14, "");
    assert_eq!(s.doc().path(s.tip()).len(), 2);
    replace(&mut s, 2..4, "");
    assert_eq!(s.doc().path_text(s.tip()), "R  a > b\nU>F");
    assert_eq!(s.doc().path(s.tip()).len(), 3);
    // It can comment out several.
    let mut s = Session::new(Doc::new("R>U>F\n>D"));
    replace(&mut s, 1..1, "//");
    assert_eq!(s.doc().path_text(s.tip()), "R//>U>F\n>D");
    assert_eq!(s.doc().path(s.tip()).len(), 2);
    // Forking from a comment goes after it.
    let mut s = Session::new(Doc::new("R // hi"));
    s.fork();
    s.doc().check();
    assert_eq!(s.doc().path_text(s.tip()), "R // hi\n>");
    // Which isn't an extra line in a file.
    let file = "R // hi\n>\n\n";
    assert_eq!(s.doc().to_file(), file);
    assert_eq!(Doc::from_file(file).unwrap(), *s.doc());
    for text in ["R // hi\n\n", "R\n", "// a\n\n\n"] {
        let mut s = Session::new(Doc::new(text));
        s.fork();
        assert_eq!(Doc::from_file(&s.doc().to_file()).unwrap(), *s.doc());
    }
}

#[test]
fn cut_and_paste_forks() {
    // Any stretch of a path without other branches.
    let file = "R U\n>\n  F B\n  >\n    D L\n";
    let s = Session::new(Doc::from_file(file).unwrap());
    let text = s.doc().path_text(s.tip());
    for (a, b, cut) in [(1, 5, "R B\n>\n  D L\n"), (1, 9, "R L\n"), (0, text.len(), "\n")] {
        let mut s = s.clone();
        replace(&mut s, a..b, "");
        assert_eq!(s.doc().to_file(), cut);
        replace(&mut s, a..a, &text[a..b]);
        assert_eq!(s.doc().to_file(), file);
    }
    // A pasted fork splits: the branches now come after it.
    let mut s = Session::new(Doc::from_file("R\n>\n  F\n>\n  D\n").unwrap());
    replace(&mut s, 1..1, ">X");
    assert_eq!(s.doc().to_file(), "R\n>\n  X\n  >\n    F\n  >\n    D\n");
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
            let text = s.doc().path_text(s.tip());
            match rng.below(10) {
                0..4 => {
                    let (a, b) = (rng.off(&text), rng.off(&text));
                    let (a, b) = (a.min(b), if rng.below(2) == 0 { a.min(b) } else { a.max(b) });
                    let mut ins = rng.text(4);
                    if rng.below(3) == 0 {
                        ins.push_str([">", "\n>\n", "//"][rng.below(3)]);
                    }
                    let mut want = text.clone();
                    want.replace_range(a..b, &ins);
                    replace(&mut s, a..b, &ins);
                    // Edits are literal, except for ending a comment.
                    let got = s.doc().path_text(s.tip());
                    assert_eq!(got.strip_suffix('\n').filter(|t| *t == want).unwrap_or(&got), want);
                    assert_eq!(s.doc().path_offset(s.tip(), s.caret()), Some(a + ins.len()));
                }
                4 => s.split(),
                5 => s.fork(),
                6 => s.join(),
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
            let d = s.doc().path_offset(s.tip(), s.caret()).unwrap();
            assert_eq!(s.doc().path_pos(s.tip(), d), s.caret());
            let file = s.doc().to_file();
            assert_eq!(Doc::from_file(&file).unwrap().to_file(), file);
            let reread = Session::new(Doc::new(&text));
            assert_eq!(reread.doc().path_text(reread.tip()), text);
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
        let Some((new, nc)) = delete(&text, c, rng.below(2) == 0) else { continue };
        assert!(new.is_char_boundary(nc));
        // Unmatched brackets can become matched.
        if !p.open.is_empty() || p.tokens.iter().any(|t| t.kind == TokenKind::Stray) {
            continue;
        }
        let words = |s: &str| -> Vec<String> {
            parse(s).tokens.iter().map(|t| s[t.span.clone()].to_owned()).collect()
        };
        let (old, new_words) = (words(&text), words(&new));
        let head = old.iter().zip(&new_words).take_while(|(a, b)| a == b).count();
        let gone = old.len() - new_words.len();
        assert!(gone > 0, "{text:?} at {c} -> {new:?}");
        assert!((0..=head).any(|i| old[i + gone..] == new_words[i..]), "{text:?} at {c} -> {new:?}");
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
