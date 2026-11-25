//! Drives the real UI headlessly. Screenshots go to target/ui-*.png.

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, pos2};
use egui_kittest::Harness;
use multitime::{doc::Doc, ui::App};

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
        .wgpu()
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    h.run_steps(3);
    h
}
fn click(h: &mut Harness<'_, App>, pos: Pos2, button: PointerButton) {
    h.event(Event::PointerMoved(pos));
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button, pressed, modifiers: Modifiers::NONE });
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
    typ(&mut h, "R U\n>F"); // the `>` starts the new segment by itself
    assert_eq!(file(&h), "R U\n>\n  F\n");
    // Arrow keys cross the fork both ways.
    key(&mut h, Modifiers::NONE, Key::ArrowLeft);
    key(&mut h, Modifiers::NONE, Key::ArrowLeft);
    typ(&mut h, "2");
    assert_eq!(file(&h), "R U2\n>\n  F\n");
    key(&mut h, Modifiers::NONE, Key::ArrowRight);
    typ(&mut h, "x ");
    assert_eq!(file(&h), "R U2\n>\n  x F\n");
    key(&mut h, Modifiers::COMMAND, Key::Enter);
    typ(&mut h, "B");
    assert!(file(&h).contains(">\n  x\n") && file(&h).contains("B"), "{}", file(&h));
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
    // Editing keys need the text box.
    for m in [Modifiers::NONE, Modifiers::SHIFT, Modifiers::ALT, Modifiers::COMMAND] {
        key(&mut h, m, Key::Backspace);
        key(&mut h, m, Key::Delete);
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
    key(&mut h, Modifiers::COMMAND, Key::Enter); // an empty child
    // Its end: the root is 4 stops long, then a gap and the child.
    let pos = pos2(36.0 + 12.0 * 6.0, 625.0);
    h.event(Event::PointerMoved(pos));
    h.run_steps(3);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(file(&h), "R U F\n");
}

#[test]
fn clicking_text_moves_caret() {
    let mut h = harness("R U F D B L\nR2 U2");
    for (pos, want) in [(pos2(50.0, 50.0), 3), (pos2(24.0, 67.0), 12), (pos2(86.0, 50.0), 7)] {
        click(&mut h, pos, PointerButton::Primary);
        assert!(h.state().session.caret().off.abs_diff(want) <= 1, "{:?}", h.state().session.caret());
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
    // Deleting a selection keeps the fork, which has siblings.
    let mut h = harness("R U F\n>\n  D B\n>\n  L\n");
    // Drag from after `R U` up to after `D`, pausing on the fork line.
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    h.event(Event::PointerMoved(pos2(50.0, 50.0)));
    h.event(button(pos2(50.0, 50.0), true));
    h.run_steps(1);
    for pos in [pos2(150.0, 68.0), pos2(32.0, 85.0)] {
        h.event(Event::PointerMoved(pos));
        h.run_steps(4);
    }
    h.event(button(pos2(32.0, 85.0), false));
    h.run_steps(2);
    key(&mut h, Modifiers::NONE, Key::Backspace);
    let f = file(&h);
    assert!(f.starts_with("R U\n>\n   B\n>\n"), "{f:?}");
}

#[test]
fn text_box_fills_panel() {
    let mut h = harness("R U\n>\n  F\n>\n  D\n");
    let root = h.state().session.doc().root();
    h.state_mut().session.select(root);
    h.run_steps(3);
    shot(&mut h, "fill");
    click(&mut h, pos2(500.0, 560.0), PointerButton::Primary);
    typ(&mut h, "x");
    assert_eq!(file(&h), "R Ux\n>\n  F\n>\n  D\n");
    // The children are inside of the box, but are still clickable.
    click(&mut h, pos2(57.0, 75.0), PointerButton::Primary);
    assert_eq!(h.state().session.doc().get(h.state().session.tip()).text, "D");

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
fn selecting_on_the_map() {
    // Dots are 12 apart. R U are at 48 and 60, then the end at 72. F and B are
    // at 96 and 108.
    let mut h = harness("R U\n>\n  F B\n>\n  D // é\n");
    let y = 625.0;
    // Shift+click extends from the caret, across the fork.
    click(&mut h, pos2(48.0, y), PointerButton::Primary);
    assert_eq!(copied(&mut h), "");
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    click(&mut h, pos2(96.0, y), PointerButton::Primary);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    shot(&mut h, "select");
    assert_eq!(copied(&mut h), " U\nF");

    // Drag, without Shift.
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    h.event(Event::PointerMoved(pos2(108.0, y)));
    h.event(button(pos2(108.0, y), true));
    h.run_steps(1);
    for x in [100.0, 90.0, 60.0] {
        h.event(Event::PointerMoved(pos2(x, y)));
        h.run_steps(2);
    }
    h.event(button(pos2(60.0, y), false));
    h.run_steps(2);
    shot(&mut h, "drag");
    assert_eq!(copied(&mut h), "\nF B");
    assert_eq!(file(&h), "R U\n>\n  F B\n>\n  D // é\n");
}

#[test]
fn selection_drawn_across_rows() {
    let mut h = harness("R U\n>\n  F B\n>\n  D [R: U2] L\n");
    click(&mut h, pos2(132.0, 651.0), PointerButton::Primary);
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    click(&mut h, pos2(48.0, 625.0), PointerButton::Primary);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.event(Event::PointerMoved(pos2(400.0, 750.0)));
    shot(&mut h, "rows");
    assert_eq!(copied(&mut h), " U\nD [R:");
}

#[test]
fn map_leaves_focus_alone() {
    let select = |h: &mut Harness<'_, App>| {
        click(h, pos2(48.0, 625.0), PointerButton::Primary);
        h.event(Event::ModifiersChanged(Modifiers::SHIFT));
        click(h, pos2(96.0, 625.0), PointerButton::Primary);
        h.event(Event::ModifiersChanged(Modifiers::NONE));
        h.run_steps(2);
        assert_eq!(copied(h), " U\nF");
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
    assert_eq!(copied(&mut h), " U\nF");
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
    click(&mut h, pos2(96.0, 625.0), PointerButton::Primary);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
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
    assert_eq!(copied(&mut h), "F");
    key(&mut h, Modifiers::NONE, Key::ArrowRight);
    key(&mut h, Modifiers::NONE, Key::J);
    assert_eq!(file(&h), "R U\nF U B\n");
    // Toolbar and Tab don't disturb it.
    key(&mut h, Modifiers::NONE, Key::Tab);
    key(&mut h, Modifiers::NONE, Key::J);
    assert_eq!(file(&h), "R U\nF U U B\n");
}
