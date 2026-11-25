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
    key(&mut h, Modifiers::SHIFT, Key::U);
    assert!(file(&h).contains("U')"), "{}", file(&h));
    key(&mut h, Modifiers::COMMAND, Key::Z);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(file(&h), after);
    shot(&mut h, "twist");
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
