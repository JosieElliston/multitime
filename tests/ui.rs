//! Drives the real UI headlessly. Screenshots go to target/ui-*.png.

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::{Harness, kittest::Queryable};
use multitime::{doc::Doc, session::Inside, ui::App};

const FILE: &str = "R' U' F D2 L
>
  // cross
  R F D2 B
  >
    // F2L pairs 1+2
    [R: U2] [F: U']
  >
    ^(U F') R
>
  // cross, inverse
  ^(U F')
>
";

fn harness(file: &str) -> Harness<'static, App> {
    let app = App::new(Doc::from_file(file).unwrap(), "/dev/null".into());
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_accessibility_check(false)
        // Fast enough for double clicks.
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run_steps(3);
    h
}
fn click(h: &mut Harness<'_, App>, pos: Pos2, button: PointerButton) {
    h.event(Event::PointerMoved(pos));
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(3);
}
fn key(h: &mut Harness<'_, App>, m: Modifiers, k: Key) {
    h.key_press_modifiers(m, k);
    h.run_steps(3);
}
fn typ(h: &mut Harness<'_, App>, s: &str) {
    for c in s.chars() {
        if c == '\n' {
            key(h, Modifiers::NONE, Key::Enter);
        } else {
            h.event(Event::Text(c.to_string()));
            h.run_steps(2);
        }
    }
}
fn shot(h: &mut Harness<'_, App>, name: &str) {
    h.run_steps(2);
    let path = format!("{}/target/ui-{name}.png", env!("CARGO_MANIFEST_DIR"));
    h.render().unwrap().save(path).unwrap();
}
fn file(h: &Harness<'_, App>) -> String {
    h.state().session.doc().to_file()
}

#[test]
fn looks() {
    let mut h = harness(FILE);
    shot(&mut h, "start");
    let root = h.state().session.doc().root();
    h.state_mut().session.select(root);
    shot(&mut h, "root");
}

#[test]
fn typing_and_forks() {
    let mut h = harness("");
    click(&mut h, pos2(100.0, 45.0), PointerButton::Primary);
    typ(&mut h, "R U>F");
    assert_eq!(file(&h), "R U\n>\n  F\n");
    // Arrow keys go through the bubble both ways.
    for _ in 0..3 {
        key(&mut h, Modifiers::NONE, Key::ArrowLeft);
    }
    typ(&mut h, "2");
    assert_eq!(file(&h), "R U2\n>\n  F\n");
    for _ in 0..2 {
        key(&mut h, Modifiers::NONE, Key::ArrowRight);
    }
    typ(&mut h, "x ");
    assert_eq!(file(&h), "R U2\n>\n  x F\n");
    typ(&mut h, ">>B");
    assert_eq!(file(&h), "R U2\n>\n  x\n  >\n    F\n  >\n    B\n");
    for _ in 0..10 {
        key(&mut h, Modifiers::COMMAND, Key::Z);
    }
    assert_eq!(file(&h), "\n");
}

#[test]
fn twisting() {
    let mut h = harness("R");
    // Sticker clicks on both views; the second is detached on inverse.
    click(&mut h, pos2(990.0, 200.0), PointerButton::Secondary);
    let after = file(&h);
    assert!(after.starts_with("R ") && after.len() > 2, "{after}");
    click(&mut h, pos2(990.0, 460.0), PointerButton::Secondary);
    assert!(file(&h).contains("^("), "{}", file(&h));
    // Keyboard goes to the last-clicked view.
    key(&mut h, Modifiers::NONE, Key::F);
    assert!(file(&h).contains("U')"), "{}", file(&h));
    key(&mut h, Modifiers::COMMAND, Key::Z);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(file(&h), after);
    shot(&mut h, "twist");
}

#[test]
fn hyperspeedcube_keys() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_accessibility_check(false)
        // Fast enough for double clicks.
        .with_step_dt(1.0 / 60.0)
        .build_ui_state(
            |ui, app: &mut App| app.show(ui),
            App::new(Doc::default(), "/dev/null".into()),
        );
    h.run_steps(3);
    // A different keyboard layout can report W at the physical comma position.
    h.event(Event::Key {
        key: Key::W,
        physical_key: Some(Key::Comma),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::SHIFT,
    });
    h.run_steps(3);
    key(&mut h, Modifiers::SHIFT, Key::Num3);
    key(&mut h, Modifiers::NONE, Key::Semicolon);
    assert_eq!(file(&h), "Dw' M y\n");
    // Backspace is by tokens, with Option by runs and with Cmd by segments,
    // and the text box's other deletions do nothing.
    key(&mut h, Modifiers::NONE, Key::Backspace);
    assert_eq!(file(&h), "Dw' M\n");
    key(&mut h, Modifiers::COMMAND, Key::Z);
    for m in [Modifiers::ALT, Modifiers::COMMAND] {
        key(&mut h, m, Key::Delete);
        assert_eq!(file(&h), "Dw' M y\n");
        key(&mut h, m, Key::Backspace);
        assert_eq!(file(&h), "\n");
        key(&mut h, Modifiers::COMMAND, Key::Z);
    }
    for k in [Key::H, Key::K, Key::U, Key::W] {
        key(&mut h, Modifiers::CTRL, k);
    }
    assert_eq!(file(&h), "Dw' M y\n");
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(file(&h), "Dw' M\n");
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    assert_eq!(file(&h), "Dw' M y\n");

    click(&mut h, pos2(100.0, 45.0), PointerButton::Primary);
    key(&mut h, Modifiers::NONE, Key::End);
    typ(&mut h, " R2");
    key(&mut h, Modifiers::NONE, Key::Backspace);
    assert_eq!(file(&h), "Dw' M y R\n");
}

#[test]
fn undo_while_previewing() {
    let mut h = harness("R U F");
    for _ in 0..2 {
        key(&mut h, Modifiers::NONE, Key::Escape); // to the text box
    }
    typ(&mut h, ">"); // an empty child
    // Its start: the root is 3 cells long, then the fork.
    let pos = pos2(36.0 + 12.0 * 4.0, 625.0);
    h.event(Event::PointerMoved(pos));
    h.run_steps(3);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(file(&h), "R U F\n");
}

#[test]
fn clicking_text_moves_caret() {
    let mut h = harness("R U F D B L\nR2 U2");
    for (pos, want) in [
        (pos2(50.0, 50.0), 3),
        (pos2(24.0, 67.0), 12),
        (pos2(86.0, 50.0), 7),
    ] {
        click(&mut h, pos, PointerButton::Primary);
        assert!(
            h.state().session.caret().off.abs_diff(want) <= 1,
            "{:?}",
            h.state().session.caret()
        );
    }
    typ(&mut h, "x");
    assert!(file(&h).find('x').unwrap().abs_diff(7) <= 1, "{}", file(&h));
}

#[test]
fn escape_cycles_focus() {
    let mut h = harness("R");
    // J is U and H is F. Starts on view 1; then view 2, text, view 1.
    key(&mut h, Modifiers::NONE, Key::J);
    assert_eq!(file(&h), "R U\n");
    key(&mut h, Modifiers::NONE, Key::Escape);
    key(&mut h, Modifiers::NONE, Key::H);
    assert_eq!(file(&h), "R U ^(F)\n");
    for _ in 0..2 {
        key(&mut h, Modifiers::NONE, Key::Escape);
        typ(&mut h, " x");
        key(&mut h, Modifiers::NONE, Key::Escape);
        key(&mut h, Modifiers::NONE, Key::J);
        key(&mut h, Modifiers::NONE, Key::ArrowLeft);
        key(&mut h, Modifiers::NONE, Key::ArrowRight);
        key(&mut h, Modifiers::NONE, Key::Escape);
        key(&mut h, Modifiers::NONE, Key::H);
    }
    assert_eq!(file(&h), "R U ^(F) x U ^(F) x U ^(F)\n");
}

#[test]
fn selecting_across_forks() {
    // Deleting a selection joins its forks.
    let mut h = harness("R U F\n>\n  D B\n>\n  L\n");
    // Drag from after `R U` up to after `D`, pausing on the fork.
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    h.event(Event::PointerMoved(pos2(50.0, 50.0)));
    h.event(button(pos2(50.0, 50.0), true));
    h.run_steps(1);
    for pos in [pos2(40.0, 67.0), pos2(31.0, 84.0)] {
        h.event(Event::PointerMoved(pos));
        h.run_steps(4);
    }
    h.event(button(pos2(31.0, 84.0), false));
    h.run_steps(2);
    key(&mut h, Modifiers::NONE, Key::Backspace);
    let f = file(&h);
    assert_eq!(f, "R U B\n"); // the other branch was in the bubble
}

#[test]
fn text_box_fills_panel() {
    let mut h = harness("R U\n>\n  F\n>\n  D\n");
    shot(&mut h, "fill");
    click(&mut h, pos2(500.0, 560.0), PointerButton::Primary);
    typ(&mut h, "x");
    assert_eq!(file(&h), "R U\n>\n  Fx\n>\n  D\n");

    let mut h = harness(&"R\n".repeat(100));
    shot(&mut h, "scrolled");
    // It started scrolled to the caret, so this is far from the first line.
    click(&mut h, pos2(100.0, 100.0), PointerButton::Primary);
    assert!(h.state().session.caret().off > 100);
}

fn copied(h: &mut Harness<'_, App>) -> String {
    h.event(Event::Copy);
    h.run_steps(1);
    let commands = &h.output().platform_output.commands;
    commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn selecting_on_the_timeline() {
    // Cells are 12 wide. The gaps around R U are at 36, 48 and 60, and the ones
    // around F B are at 72, 84 and 96.
    let mut h = harness("R U\n>\n  F B\n>\n  D // é\n");
    let y = 625.0;
    // Shift+click extends from the caret, across the fork.
    click(&mut h, pos2(48.0, y), PointerButton::Primary);
    assert_eq!(copied(&mut h), "");
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    click(&mut h, pos2(84.0, y), PointerButton::Primary);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    shot(&mut h, "select");
    assert_eq!(copied(&mut h), " U\n>\nF");

    // Drag, without Shift.
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    h.event(Event::PointerMoved(pos2(96.0, y)));
    h.event(button(pos2(96.0, y), true));
    h.run_steps(1);
    for x in [90.0, 80.0, 60.0] {
        h.event(Event::PointerMoved(pos2(x, y)));
        h.run_steps(2);
    }
    h.event(button(pos2(60.0, y), false));
    h.run_steps(2);
    shot(&mut h, "drag");
    assert_eq!(copied(&mut h), "\n>\nF B");
    assert_eq!(file(&h), "R U\n>\n  F B\n>\n  D // é\n");
}

#[test]
fn selection_drawn_across_rows() {
    let mut h = harness("R U\n>\n  F B\n>\n  D [R: U2] L\n");
    click(&mut h, pos2(120.0, 651.0), PointerButton::Primary);
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    click(&mut h, pos2(48.0, 625.0), PointerButton::Primary);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.event(Event::PointerMoved(pos2(400.0, 750.0)));
    shot(&mut h, "rows");
    assert_eq!(copied(&mut h), " U\n>\nD [R:");
}

#[test]
fn timeline_leaves_focus_alone() {
    let select = |h: &mut Harness<'_, App>| {
        click(h, pos2(48.0, 625.0), PointerButton::Primary);
        h.event(Event::ModifiersChanged(Modifiers::SHIFT));
        click(h, pos2(84.0, 625.0), PointerButton::Primary);
        h.event(Event::ModifiersChanged(Modifiers::NONE));
        h.run_steps(2);
        assert_eq!(copied(h), " U\n>\nF");
    };
    // In a view: keys still twist, which inserts and deselects.
    let mut h = harness("R U\n>\n  F B\n");
    select(&mut h);
    h.event(Event::PointerMoved(pos2(400.0, 750.0)));
    shot(&mut h, "unfocused");
    key(&mut h, Modifiers::NONE, Key::J);
    assert_eq!(file(&h), "R U\n>\n  F U B\n");
    assert_eq!(copied(&mut h), "");
    // Escape into the text box (view 2, then it) keeps the selection.
    key(&mut h, Modifiers::COMMAND, Key::Z);
    select(&mut h);
    key(&mut h, Modifiers::NONE, Key::Escape);
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert_eq!(copied(&mut h), " U\n>\nF");
    // In the text box: typing replaces it.
    select(&mut h);
    typ(&mut h, "x");
    assert_eq!(file(&h), "Rx B\n");
}

#[test]
fn clipboard_while_twisting() {
    let mut h = harness("R U\n>\n  F B\n");
    click(&mut h, pos2(48.0, 625.0), PointerButton::Primary);
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    click(&mut h, pos2(84.0, 625.0), PointerButton::Primary);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
    let clip = copied(&mut h);
    h.event(Event::Cut);
    h.run_steps(3);
    assert_eq!(file(&h), "R B\n");
    h.event(Event::Paste(clip));
    h.run_steps(3);
    assert_eq!(file(&h), "R U\n>\n  F B\n");
    // Each is its own undo step.
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(file(&h), "R B\n");
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    assert_eq!(file(&h), "R U\n>\n  F B\n");
    // Pasting text without the fork.
    key(&mut h, Modifiers::SHIFT, Key::ArrowLeft);
    key(&mut h, Modifiers::SHIFT, Key::ArrowLeft);
    key(&mut h, Modifiers::SHIFT, Key::ArrowLeft);
    assert_eq!(copied(&mut h), " U\n>\nF");
    h.event(Event::Cut);
    h.run_steps(3);
    assert_eq!(file(&h), "R B\n");
    h.event(Event::Paste(" U\nF".to_owned()));
    h.run_steps(3);
    assert_eq!(file(&h), "R U\nF B\n");
    // Typing doesn't type, but shifted arrows select and twists still twist.
    typ(&mut h, "q \n");
    assert_eq!(file(&h), "R U\nF B\n");
    key(&mut h, Modifiers::SHIFT, Key::ArrowLeft);
    assert_eq!(copied(&mut h), "\nF");
    key(&mut h, Modifiers::NONE, Key::ArrowRight);
    key(&mut h, Modifiers::NONE, Key::J);
    assert_eq!(file(&h), "R U\nF U B\n");
    // Toolbar and Tab don't disturb it.
    key(&mut h, Modifiers::NONE, Key::Tab);
    key(&mut h, Modifiers::NONE, Key::J);
    assert_eq!(file(&h), "R U\nF U U B\n");
}

#[test]
fn editing_by_tokens() {
    let mut h = harness("R U2\n>\n  F' [R: U2] B2\n");
    key(&mut h, Modifiers::SHIFT, Key::ArrowLeft);
    assert_eq!(copied(&mut h), " B2");
    key(&mut h, Modifiers::SHIFT, Key::ArrowRight);
    assert_eq!(copied(&mut h), "");
    key(&mut h, Modifiers::NONE, Key::Backspace);
    key(&mut h, Modifiers::NONE, Key::Backspace);
    assert_eq!(file(&h), "R U2\n>\n  F'\n");
    key(&mut h, Modifiers::NONE, Key::Backspace);
    assert_eq!(file(&h), "R U2\n>\n\n");
    key(&mut h, Modifiers::NONE, Key::Backspace);
    assert_eq!(file(&h), "R U2\n");
    // Each is its own undo step.
    key(&mut h, Modifiers::COMMAND, Key::Z);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(file(&h), "R U2\n>\n  F'\n");
    key(&mut h, Modifiers::NONE, Key::Home);
    key(&mut h, Modifiers::NONE, Key::Delete);
    assert_eq!(file(&h), "U2\n>\n  F'\n");
    // A selection is deleted as is.
    key(&mut h, Modifiers::SHIFT, Key::End);
    key(&mut h, Modifiers::NONE, Key::Backspace);
    assert_eq!(file(&h), "\n");
    // Select all is in both modes.
    key(&mut h, Modifiers::NONE, Key::J);
    key(&mut h, Modifiers::COMMAND, Key::A);
    assert_eq!(copied(&mut h), "U");
}

#[test]
fn typing_a_fork() {
    let start = |file| {
        let mut h = harness(file);
        for _ in 0..2 {
            key(&mut h, Modifiers::NONE, Key::Escape); // to the text box
        }
        key(&mut h, Modifiers::NONE, Key::ArrowLeft);
        h
    };
    // At a branch's start, it's a new branch after it.
    let mut h = start("R U\n>\n  F\n>\n  D\n");
    typ(&mut h, ">x");
    assert_eq!(file(&h), "R U\n>\n  F\n>\n  x\n>\n  D\n");
    // Ordinary text in a comment, which can end at a bubble.
    let mut h = start("R U\n>\n  F\n");
    key(&mut h, Modifiers::NONE, Key::ArrowLeft);
    key(&mut h, Modifiers::NONE, Key::ArrowLeft);
    typ(&mut h, " // >");
    assert_eq!(file(&h), "R U // >\n>\n  F\n");
    // In the middle of one.
    let mut h = start("R U\n");
    typ(&mut h, ">");
    assert_eq!(file(&h), "R\n>\n  U\n");
}

#[test]
fn dragging_into_a_branch() {
    // R ends at 48, then the branches start at 60.
    let mut h = harness("R\n>\n  F B\n>\n  D\n>\n  L U\n");
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let start = pos2(36.0, 625.0);
    h.event(Event::PointerMoved(start));
    h.event(button(start, true));
    h.run_steps(1);
    // Far past the end of each row, and then back within the root.
    for (to, want) in [
        (pos2(300.0, 680.0), "R\n>\nL U"),
        (pos2(300.0, 650.0), "R\n>\nD"),
        (pos2(300.0, 625.0), "R\n>\nF B"),
        (pos2(47.0, 675.0), "R"),
        (pos2(300.0, 655.0), "R\n>\nD"),
    ] {
        h.event(Event::PointerMoved(to));
        h.run_steps(2);
        assert_eq!(copied(&mut h), want, "{to:?}");
    }
    h.event(button(pos2(300.0, 655.0), false));
    h.run_steps(2);
    // Starting in another branch activates it.
    h.event(Event::PointerMoved(pos2(96.0, 677.0)));
    h.event(button(pos2(96.0, 677.0), true));
    h.run_steps(1);
    h.event(Event::PointerMoved(pos2(36.0, 640.0)));
    h.run_steps(2);
    assert_eq!(copied(&mut h).split_once("\n>").unwrap().0, "R");
    assert_eq!(
        h.state().session.doc().get(h.state().session.tip()).text,
        "L U"
    );
    h.event(button(pos2(36.0, 640.0), false));
    h.run_steps(2);
    h.event(Event::PointerMoved(pos2(84.0, 677.0)));
    h.run_steps(3);
    shot(&mut h, "ghost");
}

#[test]
fn hovering_text() {
    let mut h = harness("R U F D B L");
    h.event(Event::PointerMoved(pos2(50.0, 50.0)));
    h.run_steps(3);
    shot(&mut h, "hover-text");
    assert!(h.query_all_by_label("preview").count() > 0);
    // Keys show the real caret until the pointer moves again.
    key(&mut h, Modifiers::NONE, Key::J);
    assert_eq!(h.query_all_by_label("preview").count(), 0);
    h.event(Event::PointerMoved(pos2(60.0, 50.0)));
    h.run_steps(3);
    assert!(h.query_all_by_label("preview").count() > 0);
}

#[test]
fn bubbles() {
    use Inside::*;
    let mut h = harness("R U F\n>\n  D B L\n>\n  L U\n");
    let caret = |h: &Harness<'_, App>| {
        let s = &h.state().session;
        (
            s.doc().get(s.caret().seg).text.clone(),
            s.caret().off,
            s.inside(),
        )
    };
    let at = |text: &str, off, inside| (text.to_owned(), off, inside);
    click(&mut h, pos2(500.0, 560.0), PointerButton::Primary);
    assert_eq!(caret(&h), at("D B L", 5, No));
    // By tokens, the bubble is one, and passing it keeps the branch.
    for want in [4, 2, 0] {
        key(&mut h, Modifiers::ALT, Key::ArrowLeft);
        assert_eq!(caret(&h), at("D B L", want, No));
    }
    key(&mut h, Modifiers::ALT, Key::ArrowLeft);
    assert_eq!(caret(&h), at("R U F", 5, No));
    key(&mut h, Modifiers::ALT, Key::ArrowLeft);
    assert_eq!(caret(&h), at("R U F", 4, No));
    key(&mut h, Modifiers::ALT, Key::ArrowRight);
    assert_eq!(caret(&h), at("R U F", 5, No));
    key(&mut h, Modifiers::ALT, Key::ArrowRight);
    assert_eq!(caret(&h), at("D B L", 0, No));
    // By characters, the caret goes after each mark, making its branch
    // active, and before the first. After a mark is its branch's start.
    key(&mut h, Modifiers::NONE, Key::ArrowLeft);
    assert_eq!(caret(&h), at("D B L", 0, Start));
    key(&mut h, Modifiers::NONE, Key::ArrowLeft);
    assert_eq!(caret(&h), at("R U F", 5, No));
    key(&mut h, Modifiers::NONE, Key::ArrowRight);
    assert_eq!(caret(&h), at("D B L", 0, Start));
    for text in ["D B L", "L U"] {
        key(&mut h, Modifiers::NONE, Key::ArrowRight);
        assert_eq!(caret(&h), at(text, 0, No));
    }
    shot(&mut h, "inside");
    // Then past the first character, since the start of the line is the
    // branch's start too.
    key(&mut h, Modifiers::NONE, Key::ArrowRight);
    assert_eq!(caret(&h), at("L U", 1, No));
    key(&mut h, Modifiers::NONE, Key::ArrowLeft);
    assert_eq!(caret(&h), at("L U", 0, No));
    key(&mut h, Modifiers::NONE, Key::ArrowRight);
    // Selecting goes over it, and copies only the active branch.
    key(&mut h, Modifiers::SHIFT, Key::ArrowLeft);
    assert_eq!(copied(&mut h), "L");
    key(&mut h, Modifiers::SHIFT, Key::ArrowLeft);
    assert_eq!(caret(&h), at("R U F", 5, No));
    shot(&mut h, "inline");
    assert_eq!(copied(&mut h), "\n>\nL");
    // And back.
    key(&mut h, Modifiers::SHIFT, Key::ArrowRight);
    assert_eq!(caret(&h), at("L U", 0, No));
    assert_eq!(copied(&mut h), "L");
    // Selecting the marks, from before the first.
    click(&mut h, pos2(30.0, 67.0), PointerButton::Primary);
    key(&mut h, Modifiers::NONE, Key::ArrowLeft);
    assert_eq!(caret(&h), at("D B L", 0, Start));
    key(&mut h, Modifiers::SHIFT, Key::ArrowRight);
    assert_eq!(copied(&mut h), ">\n  D B L\n");
    // It doesn't leave the bubble.
    for _ in 0..3 {
        key(&mut h, Modifiers::SHIFT, Key::ArrowRight);
    }
    assert_eq!(copied(&mut h), ">\n  D B L\n>\n  L U\n");
    shot(&mut h, "marks");
    // Clicking a mark, and Backspace after it.
    click(&mut h, pos2(30.0, 67.0), PointerButton::Primary);
    assert_eq!(caret(&h), at("D B L", 0, No));
    key(&mut h, Modifiers::NONE, Key::Backspace);
    assert_eq!(file(&h), "R U F\n>\n  L U\n");
    assert_eq!(caret(&h), at("L U", 0, Start));
    key(&mut h, Modifiers::NONE, Key::ArrowRight);
    key(&mut h, Modifiers::NONE, Key::Backspace);
    assert_eq!(file(&h), "R U F\nL U\n");
    key(&mut h, Modifiers::COMMAND, Key::A);
    key(&mut h, Modifiers::NONE, Key::Backspace);
    assert_eq!(file(&h), "\n");
    typ(&mut h, "R>>F");
    assert_eq!(file(&h), "R\n>\n\n>\n  F\n");
}

#[test]
fn up_and_down_keep_the_branch() {
    let mut h = harness("R U F\n>\n  B D L R U F D L\n>\n  L\n");
    let tip = h.state().session.tip();
    let b = h
        .state()
        .session
        .doc()
        .get(h.state().session.doc().root())
        .children[0];
    // Whether the caret is at the start of its segment, after its `>`.
    let state = |h: &Harness<'_, App>| {
        let s = &h.state().session;
        (s.tip(), s.caret().seg, s.caret().off == 0)
    };
    for x in [25.0, 50.0, 100.0] {
        click(&mut h, pos2(x, 50.0), PointerButton::Primary);
        // Onto the bubble's line, after the active branch's mark.
        key(&mut h, Modifiers::NONE, Key::ArrowDown);
        assert_eq!(state(&h), (tip, b, true), "{x}");
        key(&mut h, Modifiers::NONE, Key::ArrowDown);
        assert_eq!(state(&h), (tip, b, false), "{x}");
        key(&mut h, Modifiers::NONE, Key::ArrowUp);
        assert_eq!(state(&h), (tip, b, true), "{x}");
        key(&mut h, Modifiers::NONE, Key::ArrowUp);
        assert_eq!(h.state().session.tip(), tip);
        assert_eq!(h.state().session.inside(), Inside::No);
    }
}

#[test]
fn moving_and_duplicating_lines() {
    let mut h = harness("A\nB\n>\n  C\n>\n  D\n");
    // After the `A`, since at a branch's start, the branch would move.
    click(&mut h, pos2(60.0, 50.0), PointerButton::Primary);
    key(&mut h, Modifiers::ALT, Key::ArrowDown);
    assert_eq!(file(&h), "B\nA\n>\n  C\n>\n  D\n");
    key(&mut h, Modifiers::ALT, Key::ArrowDown);
    assert_eq!(file(&h), "B\n>\n  A\n  C\n>\n  D\n");
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::ArrowDown);
    assert_eq!(file(&h), "B\n>\n  A\n  A\n  C\n>\n  D\n");
    // At a branch's start, after its `>`, the branch moves.
    key(&mut h, Modifiers::NONE, Key::ArrowUp);
    key(&mut h, Modifiers::NONE, Key::ArrowUp);
    assert_eq!(h.state().session.caret().off, 0);
    key(&mut h, Modifiers::ALT, Key::ArrowDown);
    assert_eq!(file(&h), "B\n>\n  D\n>\n  A\n  A\n  C\n");
    // In twist mode too.
    key(&mut h, Modifiers::NONE, Key::Escape);
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::ArrowUp);
    assert_eq!(
        h.state()
            .session
            .doc()
            .get(h.state().session.doc().root())
            .children
            .len(),
        3
    );
}

#[test]
fn forks_wrap() {
    let long = "R U F D ".repeat(30);
    let mut h = harness(&format!(
        "{long}\n>\n  F\n  >\n    {long}\n  >\n    D\n>\n  D\n"
    ));
    shot(&mut h, "wrap");
    // Adjacent forks.
    let mut h = harness("R\n>\n\n  >\n    U\n  >\n    D\n>\n  F\n>\n  B\n");
    shot(&mut h, "adjacent");
}

/// Renders the README's screenshot: `cargo test --test ui -- --ignored`.
#[test]
#[ignore]
fn readme_screenshot() {
    let app = App::new(Doc::from_file(FILE).unwrap(), "/dev/null".into());
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 600.0))
        .with_pixels_per_point(2.0)
        .with_accessibility_check(false)
        // Fast enough for double clicks.
        .with_step_dt(1.0 / 60.0)
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run_steps(3);
    // Select the second F2L pair, with the caret partway into the branch.
    for _ in 0..3 {
        key(&mut h, Modifiers::NONE, Key::ArrowLeft);
    }
    key(&mut h, Modifiers::SHIFT, Key::ArrowLeft);
    key(&mut h, Modifiers::SHIFT, Key::ArrowLeft);
    h.event(Event::PointerMoved(pos2(-10.0, -10.0)));
    h.run_steps(3);
    let path = format!("{}/ss.png", env!("CARGO_MANIFEST_DIR"));
    // Without the harness's margin.
    let image = h.render().unwrap();
    let m = 16;
    let (w, h) = (image.width() - 2 * m, image.height() - 2 * m);
    image::imageops::crop_imm(&image, m, m, w, h)
        .to_image()
        .save(path)
        .unwrap();
}

/// Presses `n` times in quick succession, then drags through `path`.
fn clicks(h: &mut Harness<'_, App>, n: usize, pos: Pos2, path: &[Pos2]) {
    // Far enough apart in time from any earlier clicks.
    h.run_steps(30);
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    h.event(Event::PointerMoved(pos));
    h.run_steps(1);
    for i in 0..n {
        h.event(button(pos, true));
        h.run_steps(1);
        if i + 1 < n {
            h.event(button(pos, false));
            h.run_steps(1);
        }
    }
    for &p in path {
        h.event(Event::PointerMoved(p));
        h.run_steps(2);
    }
    h.event(button(path.last().copied().unwrap_or(pos), false));
    h.run_steps(3);
}

#[test]
fn selecting_text_by_units() {
    // Characters are 9 wide, from 23.
    let mut h = harness("R U' [F, D B] L\nB' D2 // cd\n");
    let x = |i: f32| 27.5 + 9.0 * i;
    let cases: &[(usize, Pos2, &[Pos2], &str)] = &[
        (2, pos2(x(3.0), 50.0), &[], "U'"),
        (2, pos2(x(5.0), 50.0), &[], "[F, D B]"),
        (2, pos2(x(7.0), 50.0), &[], "[F, D B]"),
        (3, pos2(x(11.0), 50.0), &[], "D B"),
        (3, pos2(x(5.0), 50.0), &[], "F, D B"),
        (3, pos2(x(1.0), 67.0), &[], "B' D2 // cd"),
        (3, pos2(x(9.0), 67.0), &[], "B' D2 // cd"),
        // In a comment, a word.
        (2, pos2(x(10.0), 67.0), &[], "cd"),
        // A drag extends by units, and takes in whole groups.
        (
            2,
            pos2(x(0.0), 50.0),
            &[pos2(x(9.0), 50.0)],
            "R U' [F, D B]",
        ),
        (2, pos2(x(14.0), 50.0), &[pos2(x(11.0), 50.0)], "[F, D B] L"),
        (
            3,
            pos2(x(9.0), 50.0),
            &[pos2(x(1.0), 67.0)],
            "[F, D B] L\nB' D2 // cd",
        ),
    ];
    for &(n, pos, path, want) in cases {
        clicks(&mut h, n, pos, path);
        assert_eq!(copied(&mut h), want, "{n} at {pos:?} through {path:?}");
    }
    shot(&mut h, "units");
    assert_eq!(file(&h), "R U' [F, D B] L\nB' D2 // cd\n");
}

#[test]
fn selecting_timeline_by_units() {
    // Cells are 12 wide, centered from 42.
    let mut h = harness("R U [F, D] B\n>\n  L\n");
    let (x, y) = (|i: f32| 42.0 + 12.0 * i, 625.0);
    let cases: &[(usize, Pos2, &[Pos2], &str)] = &[
        (2, pos2(x(1.0), y), &[], "U"),
        (2, pos2(x(2.0), y), &[], "[F, D]"),
        (3, pos2(x(3.0), y), &[], "F"),
        (3, pos2(x(0.0), y), &[], "R U"),
        (2, pos2(x(0.0), y), &[pos2(x(3.0), y)], "R U [F, D]"),
        (2, pos2(x(7.0), y), &[pos2(x(9.0), y)], "B\n>\nL"),
    ];
    for &(n, pos, path, want) in cases {
        clicks(&mut h, n, pos, path);
        assert_eq!(copied(&mut h), want, "{n} at {pos:?} through {path:?}");
    }
    assert_eq!(file(&h), "R U [F, D] B\n>\n  L\n");
}

#[test]
fn twist_mode_runs_and_segments() {
    let mut h = harness("R U [F, D] B\n>\n  L x\n");
    let caret = |h: &Harness<'_, App>| {
        let s = &h.state().session;
        (s.doc().get(s.caret().seg).text.clone(), s.caret().off)
    };
    // Twist mode starts at the end of the path.
    key(&mut h, Modifiers::COMMAND, Key::ArrowLeft);
    assert_eq!(caret(&h), ("L x".into(), 0));
    key(&mut h, Modifiers::COMMAND, Key::ArrowLeft);
    assert_eq!(caret(&h).1, 0);
    key(&mut h, Modifiers::ALT, Key::ArrowRight);
    assert_eq!(caret(&h).1, 3);
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::ArrowRight);
    key(&mut h, Modifiers::ALT | Modifiers::SHIFT, Key::ArrowRight);
    assert_eq!(copied(&mut h), " [F");
    key(&mut h, Modifiers::COMMAND, Key::ArrowRight);
    assert_eq!(caret(&h).1, 12);
    assert_eq!(file(&h), "R U [F, D] B\n>\n  L x\n");
}

#[test]
fn selection_drawn_while_dragging_by_units() {
    // A triple click and hold: egui would select the whole line, but the
    // run ends at the bracket, and that's what's drawn, before release.
    let mut h = harness("R U [F, D] B\n");
    let x = |i: f32| 27.5 + 9.0 * i;
    let pos = pos2(x(0.0), 50.0);
    h.run_steps(30);
    h.event(Event::PointerMoved(pos));
    h.run_steps(1);
    for pressed in [true, false, true, false, true] {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        h.run_steps(1);
    }
    let image = h.render().unwrap();
    let selection = egui::Visuals::dark().selection.bg_fill;
    // Above the glyphs, within the row.
    let at = |i: f32| image.get_pixel(x(i) as u32, 43).0;
    let is_selected = |i| {
        let [r, g, b, _] = at(i);
        egui::Color32::from_rgb(r, g, b) == selection
    };
    assert!(is_selected(0.0) && is_selected(2.0), "{:?}", at(0.0));
    assert!(!is_selected(11.0), "{:?}", at(11.0));
    shot(&mut h, "held");
}

#[test]
fn panels_agree_within_a_frame() {
    // The views are drawn before the text box, which finds what's hovered,
    // and they show it in the same frame.
    let mut h = harness("R U F\n");
    h.event(Event::PointerMoved(pos2(40.0, 50.0)));
    h.run_steps(1);
    assert_eq!(h.query_all_by_label("preview").count(), 2);
}

#[test]
fn text_mode_deletes_by_tokens() {
    let mut h = harness("R U2' [F, D]\n");
    key(&mut h, Modifiers::NONE, Key::Escape);
    key(&mut h, Modifiers::NONE, Key::Escape);
    // Text mode, at the end.
    key(&mut h, Modifiers::ALT, Key::Backspace);
    assert_eq!(file(&h), "R U2'\n");
    key(&mut h, Modifiers::ALT, Key::ArrowLeft);
    key(&mut h, Modifiers::ALT, Key::Delete);
    assert_eq!(file(&h), "R\n");
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(file(&h), "R U2'\n");
}
