//! egui front end. Holds only view state; everything else is in `Session`.

use crate::doc::{Doc, Pos, SegId};
use crate::notation::{self, GroupKind, Inversion, TokenKind, Unit};
use crate::puzzle::{self, Perm, STICKERS};
use crate::session::{END, InversionMode, MARK, OPEN, Session};
use eframe::egui::{
    self, Align2, Color32, Event, FontId, Id, Key, Modifiers, Pos2, Rect, Sense, Shape, Stroke,
    TextFormat, Ui, pos2, vec2,
};
use glam::{Quat, Vec3};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Colors for a theme.
struct Palette {
    moves: Color32,
    comment: Color32,
    /// A plain group's brackets.
    group: Color32,
    commutator: Color32,
    conjugate: Color32,
    niss: Color32,
    error: Color32,
    dim: Color32,
}
const DARK: Palette = Palette {
    moves: Color32::from_rgb(110, 170, 255),
    comment: Color32::from_rgb(110, 190, 110),
    group: Color32::from_rgb(190, 140, 255),
    commutator: Color32::from_rgb(235, 200, 80),
    conjugate: Color32::from_rgb(70, 205, 200),
    niss: Color32::from_rgb(255, 120, 200),
    error: Color32::from_rgb(255, 80, 80),
    dim: Color32::from_gray(120),
};
/// Darker, for a light background.
const LIGHT: Palette = Palette {
    moves: Color32::from_rgb(25, 95, 205),
    comment: Color32::from_rgb(35, 125, 45),
    group: Color32::from_rgb(125, 65, 200),
    commutator: Color32::from_rgb(165, 115, 0),
    conjugate: Color32::from_rgb(0, 130, 125),
    niss: Color32::from_rgb(195, 35, 125),
    error: Color32::from_rgb(210, 30, 30),
    dim: Color32::from_gray(140),
};
fn palette(ui: &Ui) -> &'static Palette {
    if ui.visuals().dark_mode {
        &DARK
    } else {
        &LIGHT
    }
}
/// Caret in both the text box and the timeline.
fn caret_stroke(ui: &Ui) -> Stroke {
    Stroke::new(2.0, ui.visuals().strong_text_color())
}
/// Where hovering would put the caret.
const GHOST_CARET: Stroke = Stroke {
    width: 2.0,
    color: Color32::from_gray(130),
};
const EDITOR: &str = "path editor";
const FONT: FontId = FontId::monospace(15.0);
/// Width of a `MARK` in the text box.
const MARK_WIDTH: f32 = 18.0;

struct View {
    mode: InversionMode,
    orientation: Quat,
}

/// There's one caret and selection, and mostly the same commands in each mode.
/// The mode is what a key inserts, and the unit that keys move and delete by.
/// The text box always has egui's focus, but in twist mode, it only gets the
/// pointer and the clipboard.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Characters, in the text box.
    Text,
    /// Tokens, with twists done in this view.
    Twist(usize),
}

/// What a gesture on the timeline does.
enum TimelineAction {
    Go(Pos),
    /// Selects from `from`, or else from where the selection already starts.
    Select {
        from: Option<Pos>,
        to: Pos,
    },
    /// A double or triple click selected a unit.
    StartUnits(Unit, (Pos, Pos)),
    /// The drag after one reached a unit.
    ExtendUnits((Pos, Pos)),
    /// A right click, which opens the context menu there.
    Menu(Pos),
}

/// An item in the context menu.
#[derive(Clone, Copy)]
enum MenuCommand {
    Cut,
    Copy,
    Paste,
    /// A commutator or conjugate, by its separator.
    Binary(char),
    /// Brackets around the selection.
    Wrap(&'static str, &'static str),
    Expand,
    Invert,
}

/// A drag after a double or triple click, which extends by whole units.
#[derive(Clone, Copy)]
struct UnitDrag {
    unit: Unit,
    /// The unit first clicked.
    anchor: (Pos, Pos),
    timeline: bool,
}

pub struct App {
    pub session: Session,
    path: PathBuf,
    saved: Doc,
    status: String,
    views: Vec<View>,
    mode: Mode,
    /// Position hovered last frame, in the timeline or the text box. Both show a
    /// ghost caret there, and the views show it instead of the caret.
    preview: Option<Pos>,
    /// A key was pressed since the pointer last moved. Otherwise a pointer
    /// resting on the text box would hide what's being typed.
    ignore_hover: bool,
    /// Click count, time and place of the last primary press. egui only counts
    /// clicks on release, too late to start a drag by units.
    press: (u32, f64, Pos2),
    unit_drag: Option<UnitDrag>,
    /// Where the last right click was, for the separator that the context
    /// menu puts in.
    menu_at: Option<Pos>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        self.show(ui);
    }
}

/// The colors of `text`'s tokens. A group's brackets, separator and
/// multiplier are its kind's color: plain, commutator, conjugate or NISS.
fn token_colors(p: &Palette, text: &str, tokens: &[notation::Token]) -> Vec<Color32> {
    let mut out: Vec<Color32> = tokens
        .iter()
        .map(|t| match t.kind {
            TokenKind::Move => p.moves,
            TokenKind::Pause => p.dim,
            TokenKind::Comment => p.comment,
            TokenKind::Open | TokenKind::Sep | TokenKind::Close { .. } | TokenKind::Multiplier => {
                p.group
            }
            TokenKind::BadClose | TokenKind::Stray | TokenKind::Error => p.error,
        })
        .collect();
    // The tokens of each open group, and its color once known.
    let mut open: Vec<(Vec<usize>, Color32)> = vec![];
    for (i, t) in tokens.iter().enumerate() {
        match t.kind {
            // A nested p.niss is an error, but still a group.
            TokenKind::Open | TokenKind::Error => {
                let Some((kind, _)) = notation::opener(&text[t.span.clone()]) else {
                    continue;
                };
                let color = if kind == GroupKind::Niss {
                    p.niss
                } else {
                    p.group
                };
                let members = if t.kind == TokenKind::Open {
                    vec![i]
                } else {
                    vec![]
                };
                open.push((members, color));
            }
            TokenKind::Sep => {
                if let Some(g) = open.last_mut() {
                    g.0.push(i);
                    g.1 = if text[t.span.clone()].starts_with(',') {
                        p.commutator
                    } else {
                        p.conjugate
                    };
                }
            }
            TokenKind::Close { .. } => {
                if let Some((members, color)) = open.pop() {
                    for j in members.into_iter().chain([i]) {
                        out[j] = color;
                    }
                }
            }
            TokenKind::BadClose => {
                open.pop();
            }
            TokenKind::Multiplier if i > 0 => out[i] = out[i - 1],
            _ => {}
        }
    }
    // Still open: the end of the text is inside them.
    for (members, color) in open {
        for j in members {
            if out[j] != p.error {
                out[j] = color;
            }
        }
    }
    out
}

// Hyperspeedcube's Rubiks3D Default preset, including ^MU.
fn cube_move(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::Num2 | Key::V => "U2'",
        Key::Num3 => "M",
        Key::Num4 | Key::F => "U'",
        Key::Num8 | Key::J => "U",
        Key::Num9 => "M'",
        Key::Num0 => "M2'",
        Key::Q => "z'",
        Key::W => "B",
        Key::E => "L'",
        Key::R => "y2'",
        Key::I => "R",
        Key::O => "B'",
        Key::P => "z",
        Key::A => "y'",
        Key::D => "L",
        Key::G => "F'",
        Key::H => "F",
        Key::K => "R'",
        Key::Semicolon => "y",
        Key::Z => "x'",
        Key::X => "D",
        Key::C => "L2'",
        Key::B => "D2",
        Key::N => "U2",
        Key::M => "R2'",
        Key::Comma => "D'",
        Key::Period | Key::Slash => "x",
        _ => return None,
    })
}

impl App {
    pub fn new(doc: Doc, path: PathBuf) -> Self {
        let view = |mode| View {
            mode,
            orientation: Quat::from_rotation_x(0.5) * Quat::from_rotation_y(-0.6),
        };
        Self {
            saved: doc.clone(),
            session: Session::new(doc),
            path,
            status: String::new(),
            views: vec![
                view(InversionMode::Attached),
                view(InversionMode::Detached(Inversion::Inverse)),
            ],
            mode: Mode::Twist(0),
            preview: None,
            ignore_hover: false,
            press: (0, f64::NEG_INFINITY, Pos2::ZERO),
            unit_drag: None,
            menu_at: None,
        }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        // Keep the text box focused. See `Mode`.
        ui.ctx().options_mut(|o| {
            o.input_options.surrender_focus_on = egui::SurrenderFocusOn::Never;
            // One more pass than egui's default, so that a pass discarded for
            // an edit can still be redrawn for the state after it. See `drawn`.
            o.max_passes = std::num::NonZeroUsize::new(3).expect("nonzero");
        });
        ui.memory_mut(|m| m.request_focus(Id::unique(EDITOR)));
        let (delay, dist) = ui.options(|o| {
            (
                o.input_options.max_double_click_delay,
                o.input_options.max_click_dist,
            )
        });
        ui.input(|i| {
            for event in &i.events {
                if let Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    ..
                } = *event
                {
                    let (n, time, at) = self.press;
                    let again = i.time - time < delay && at.distance(pos) < dist;
                    self.press = (if again { (n + 1).min(3) } else { 1 }, i.time, pos);
                    self.unit_drag = None;
                }
                match event {
                    Event::PointerMoved(_) | Event::PointerButton { .. } => {
                        self.ignore_hover = false
                    }
                    Event::Key { pressed: true, .. }
                    | Event::Text(_)
                    | Event::Cut
                    | Event::Paste(_) => self.ignore_hover = true,
                    _ => {}
                }
            }
        });
        self.shortcuts(ui);
        // Edits since last frame can invalidate the preview.
        self.preview = self
            .preview
            .filter(|p| !self.ignore_hover && self.is_valid(*p));
        let mut preview = None;
        let mut command = None;
        // Panels show the state as it was when each was drawn, and a later one
        // can change it, so redraw everything until nothing does.
        let drawn = self.drawn();
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("timeline")
            .resizable(true)
            .default_size(190.0)
            .show(ui, |ui| {
                self.timeline(ui, &mut preview, &mut command);
            });
        egui::Panel::left("views")
            .resizable(true)
            .default_size(420.0)
            .show(ui, |ui| {
                let height = ui.available_height() / self.views.len() as f32;
                for i in 0..self.views.len() {
                    self.view(ui, i, height);
                }
            });
        egui::CentralPanel::default().show(ui, |ui| {
            // The text box is at least as tall as the panel, and the panel scrolls.
            let height = ui.available_height();
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    self.editor(ui, height, &mut preview, &mut command);
                });
        });
        preview = preview.filter(|_| !self.ignore_hover);
        self.preview = preview;
        if let Some(c) = command {
            self.run(ui, c);
        }
        if self.drawn() != drawn {
            ui.ctx().request_discard("changed after drawing");
            // In case that's past `max_passes`.
            ui.ctx().request_repaint();
        }
    }

    /// Everything that the panels draw from, besides the views' orientations.
    fn drawn(&self) -> impl PartialEq + use<> {
        let s = &self.session;
        let pos = (s.tip(), s.caret(), s.anchor_offset(), s.inside());
        (s.doc().clone(), pos, self.mode, self.preview)
    }

    fn is_valid(&self, p: Pos) -> bool {
        let doc = self.session.doc();
        doc.ids().any(|s| s == p.seg) && doc.get(p.seg).text.is_char_boundary(p.off)
    }

    fn shortcuts(&mut self, ui: &mut Ui) {
        // The session does the clipboard, in both modes, since the text box's
        // text doesn't have the tree.
        let mut copied = None;
        ui.input_mut(|i| {
            i.events.retain(|e| {
                match e {
                    Event::Copy => copied = self.session.copy().or(copied.take()),
                    Event::Cut => copied = self.session.cut().or(copied.take()),
                    Event::Paste(t) => self.session.paste(t),
                    _ => return true,
                }
                false
            })
        });
        if let Some(t) = copied {
            ui.ctx().copy_text(t);
        }
        let key = |m, k| ui.input_mut(|i| i.consume_key(m, k));
        let (cmd, shift, alt) = (Modifiers::COMMAND, Modifiers::SHIFT, Modifiers::ALT);
        let s = &mut self.session;
        // Shifted first: `consume_key` ignores extra shifts.
        for (k, down) in [(Key::ArrowUp, false), (Key::ArrowDown, true)] {
            if key(alt | shift, k) {
                s.duplicate_lines(down);
            } else if key(alt, k) {
                s.move_lines(down);
            }
        }
        // Shifted first: `consume_key` ignores extra shifts.
        if key(cmd | shift, Key::Z) || key(cmd, Key::Y) {
            s.redo();
        }
        if key(cmd, Key::Z) {
            s.undo();
        }
        if key(cmd, Key::Enter) {
            s.fork();
        }
        if key(cmd, Key::J) {
            s.join();
        }
        if key(cmd, Key::Slash) {
            s.toggle_comment();
        }
        if key(cmd, Key::OpenBracket) {
            s.cycle_sibling(-1);
        }
        if key(cmd, Key::CloseBracket) {
            s.cycle_sibling(1);
        }
        if key(cmd, Key::S) {
            self.save();
        }
        // Would insert a tab or move egui's focus. In text mode, it's four
        // spaces, typed like any other text.
        if key(Modifiers::NONE, Key::Tab) && self.mode == Mode::Text {
            ui.input_mut(|i| i.events.push(Event::Text("    ".to_owned())));
        }
        // Escape cycles: text mode, then twist mode in each view.
        if key(Modifiers::NONE, Key::Escape) {
            self.mode = match self.mode {
                Mode::Text => Mode::Twist(0),
                Mode::Twist(i) if i + 1 < self.views.len() => Mode::Twist(i + 1),
                Mode::Twist(_) => Mode::Text,
            };
        }
        let Mode::Twist(view) = self.mode else {
            // An opening bracket, or a `^` for NISS, wraps the selection
            // instead of replacing it.
            ui.input_mut(|i| {
                i.events.retain(|e| {
                    let (open, close) = match e {
                        Event::Text(t) if t == "(" => ("(", ")"),
                        Event::Text(t) if t == "[" => ("[", "]"),
                        Event::Text(t) if t == "^" => ("^(", ")"),
                        _ => return true,
                    };
                    !self.session.wrap(open, close)
                });
            });
            ui.input_mut(|i| {
                i.events.retain(|event| {
                    let Event::Key {
                        key,
                        pressed: true,
                        modifiers: m,
                        ..
                    } = *event
                    else {
                        return true;
                    };
                    match key {
                        Key::ArrowLeft | Key::ArrowRight if m.command && !m.ctrl && !m.alt => {
                            self.session.step_line(key == Key::ArrowRight, m.shift);
                        }
                        Key::ArrowLeft | Key::ArrowRight
                            if m.shift
                                && !m.alt
                                && !m.command
                                && !m.ctrl
                                && self.session.select_bubble_at_edge(key == Key::ArrowRight) => {}
                        Key::ArrowLeft | Key::ArrowRight if m.alt && !m.command && !m.ctrl => {
                            let forward = key == Key::ArrowRight;
                            if m.shift {
                                self.session.select_by_meaning(forward, false);
                            } else {
                                self.session.step_word(forward, false);
                            }
                        }
                        Key::Backspace | Key::Delete if m.alt && !m.command && !m.ctrl => {
                            self.session.delete_word(key == Key::Delete);
                        }
                        _ => return true,
                    }
                    false
                });
            });
            return;
        };
        ui.input_mut(|input| {
            input.events.retain(|event| match *event {
                Event::Text(_) | Event::Ime(_) => false,
                Event::Key {
                    key,
                    physical_key,
                    pressed: true,
                    repeat,
                    modifiers,
                } => {
                    let arrow = matches!(key, Key::ArrowLeft | Key::ArrowRight);
                    let delete = matches!(key, Key::Backspace | Key::Delete);
                    if (arrow || delete) && !modifiers.ctrl && (modifiers.command || modifiers.alt)
                    {
                        // By segment or by run.
                        let forward = matches!(key, Key::ArrowRight | Key::Delete);
                        let s = &mut self.session;
                        match (delete, modifiers.command) {
                            (false, true) => s.step_segment(forward, modifiers.shift),
                            (false, false) if modifiers.shift => s.select_by_meaning(forward, true),
                            (false, false) => s.step_run(forward, false),
                            (true, by_segment) => s.delete_to(forward, by_segment),
                        }
                        return false;
                    }
                    if modifiers.command {
                        // Select all, and egui's zoom.
                        return matches!(
                            key,
                            Key::A | Key::Plus | Key::Equals | Key::Minus | Key::Num0
                        );
                    }
                    if !modifiers.ctrl && !modifiers.alt {
                        let key = match key {
                            Key::ArrowLeft
                            | Key::ArrowRight
                            | Key::ArrowUp
                            | Key::ArrowDown
                            | Key::Home
                            | Key::End
                            | Key::Backspace
                            | Key::Delete => key,
                            _ => physical_key.unwrap_or(key),
                        };
                        self.twist_mode_key(view, key, modifiers.shift, repeat);
                    }
                    false
                }
                _ => true,
            });
        });
    }

    /// The same commands that the text box has, but by tokens.
    fn twist_mode_key(&mut self, view: usize, key: Key, shift: bool, repeat: bool) {
        let s = &mut self.session;
        match key {
            Key::ArrowLeft => s.step(false, shift),
            Key::ArrowRight => s.step(true, shift),
            Key::ArrowUp if !shift => s.cycle_sibling(-1),
            Key::ArrowDown if !shift => s.cycle_sibling(1),
            Key::Home => s.step_all(false, shift),
            Key::End => s.step_all(true, shift),
            Key::Backspace => s.delete(false),
            Key::Delete => s.delete(true),
            _ => {
                let Some(name) = cube_move(key).filter(|_| !repeat) else {
                    return;
                };
                let (face, amount) = name.split_at(1);
                if shift && "RLUDFB".contains(face) {
                    self.twist(view, &format!("{face}w{amount}"));
                } else {
                    self.twist(view, name);
                }
            }
        }
    }

    fn twist(&mut self, view: usize, mv: &str) {
        let (_, caret) = self.session.eval(self.session.caret());
        self.session.twist(mv, self.views[view].mode.resolve(caret));
    }

    fn run(&mut self, ui: &Ui, command: MenuCommand) {
        let s = &mut self.session;
        match command {
            MenuCommand::Cut => {
                if let Some(t) = s.cut() {
                    ui.ctx().copy_text(t);
                }
            }
            MenuCommand::Copy => {
                if let Some(t) = s.copy() {
                    ui.ctx().copy_text(t);
                }
            }
            // It comes back as a paste event. See `shortcuts`.
            MenuCommand::Paste => ui
                .ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste),
            MenuCommand::Binary(sep) => {
                s.wrap_binary(sep, self.menu_at.unwrap_or(s.caret()));
            }
            MenuCommand::Wrap(open, close) => {
                s.wrap(open, close);
            }
            MenuCommand::Expand => s.expand(),
            MenuCommand::Invert => s.invert(),
        }
    }

    /// Where a right click on the text box or the timeline leaves the caret:
    /// in place if it's in the selection, or else at the click.
    fn right_click(&mut self, pos: Pos, off: Option<usize>, timeline: bool) {
        if !off.is_some_and(|o| self.session.selection_contains(o)) {
            match off {
                Some(o) if !timeline => self.session.set_caret_offset(o),
                _ => self.session.go_to_on_timeline(pos),
            }
        }
        self.menu_at = Some(pos);
    }

    fn save(&mut self) {
        self.status = match std::fs::write(&self.path, self.session.doc().to_file()) {
            Ok(()) => {
                self.saved = self.session.doc().clone();
                format!("saved {}", self.path.display())
            }
            Err(e) => format!("{}: {e}", self.path.display()),
        };
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            let s = &mut self.session;
            let seg = s.caret().seg;
            let has_parent = s.doc().get(seg).parent.is_some();
            let button = |ui: &mut Ui, enabled, text: &str, hint: &str| {
                ui.add_enabled(enabled, egui::Button::new(text))
                    .on_hover_text(hint)
                    .clicked()
            };
            if button(
                ui,
                true,
                "Fork",
                "Type a `>`, even in twist mode (Cmd+Enter)",
            ) {
                s.fork();
            }
            if button(
                ui,
                has_parent,
                "Join",
                "Delete the bubble before this segment (Cmd+J)",
            ) {
                s.join();
            }
            if button(
                ui,
                has_parent,
                "Delete branch",
                "Delete this segment and all after it",
            ) {
                s.delete_branch();
            }
            ui.separator();
            if button(ui, true, "Undo", "Cmd+Z") {
                s.undo();
            }
            if button(ui, true, "Redo", "Cmd+Shift+Z") {
                s.redo();
            }
            ui.separator();
            if button(ui, true, "New scramble", "Replace the document (undoable)") {
                s.replace(Doc::new(&random_scramble()));
            }
            if button(ui, true, "Save", "Cmd+S") {
                self.save();
            }
            if self.saved != *self.session.doc() {
                ui.label("unsaved");
            } else {
                ui.label(&self.status);
            }
        });
    }

    // ---- text editor ----

    fn editor(
        &mut self,
        ui: &mut Ui,
        min_height: f32,
        preview: &mut Option<Pos>,
        command: &mut Option<MenuCommand>,
    ) {
        let id = Id::unique(EDITOR);
        let mut text = self.session.text();
        let caret = self.session.caret_offset();

        // The session owns the caret and selection.
        let caret_chars = text[..caret].chars().count();
        let anchor = self.session.anchor_offset();
        let range = egui::text::CCursorRange {
            secondary: egui::text::CCursor::new(text[..anchor].chars().count()),
            ..egui::text::CCursorRange::one(egui::text::CCursor::new(caret_chars))
        };
        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        let old = state.cursor.char_range();
        let moved = old.is_none_or(|r| r.primary.index != range.primary.index);
        if moved || old.is_none_or(|r| r.secondary.index != range.secondary.index) {
            state.cursor.set_char_range(Some(range));
            state.store(ui.ctx(), id);
        }

        // egui draws the text box from its own selection, before we correct
        // it, so it draws nothing visible, and `paint_text` draws the rest.
        let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, width: f32| {
            let mut job = highlight(ui, buf.as_str());
            job.wrap.max_width = width;
            for s in &mut job.sections {
                s.format.color = Color32::TRANSPARENT;
            }
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let visuals = ui.visuals().clone();
        let w = &visuals.widgets.active;
        // The focus border, which is otherwise the selection's text color.
        let border = if self.mode == Mode::Text {
            visuals.selection.stroke
        } else {
            Stroke::NONE
        };
        let frame = egui::Frame {
            fill: visuals.text_edit_bg_color(),
            corner_radius: w.corner_radius,
            inner_margin: egui::Margin::symmetric(4, 2) - egui::Margin::from(border.width),
            outer_margin: egui::Margin::from(-w.expansion),
            stroke: border,
            ..Default::default()
        };
        let v = ui.visuals_mut();
        v.text_cursor.stroke = Stroke::NONE;
        v.selection.bg_fill = Color32::TRANSPARENT;
        v.selection.stroke.color = Color32::TRANSPARENT;
        let out = egui::TextEdit::multiline(&mut text)
            .id(id)
            .frame(frame)
            .min_size(vec2(0.0, min_height))
            .desired_width(f32::INFINITY)
            .desired_rows(1)
            // Escape is ours; see `shortcuts`.
            .event_filter(egui::EventFilter {
                horizontal_arrows: true,
                vertical_arrows: true,
                tab: false,
                escape: true,
            })
            .layouter(&mut layouter)
            .show(ui);
        *ui.visuals_mut() = visuals;
        out.response
            .context_menu(|ui| *command = context_menu(ui, &self.session));
        let wrap = out.galley.job.wrap.max_width;
        let paint_text = |ui: &Ui, s: &Session| {
            let text = s.text();
            let mut job = highlight(ui, &text);
            job.wrap.max_width = wrap;
            let mut galley = ui.fonts_mut(|f| f.layout_job(job));
            let chars = |off: usize| egui::text::CCursor::new(text[..off].chars().count());
            let (anchor, caret) = (s.anchor_offset(), s.caret_offset());
            let range = egui::text::CCursorRange::two(chars(anchor), chars(caret));
            if anchor != caret {
                egui::text_selection::visuals::paint_text_selection(
                    &mut galley,
                    ui.visuals(),
                    &range,
                    None,
                );
            }
            let pos = out.galley_pos - vec2(galley.rect.left(), 0.0);
            ui.painter().galley(pos, galley, Color32::PLACEHOLDER);
        };

        // `out.cursor_range` doesn't include this frame's clicks, but this does.
        let range = out.state.cursor.char_range();
        if out.response.is_pointer_button_down_on() {
            self.mode = Mode::Text;
        }
        let byte = |c: egui::text::CCursor| {
            let n: usize = c.index.into();
            text.char_indices().nth(n).map_or(text.len(), |(i, _)| i)
        };
        if out.response.changed() {
            let caret = range.map_or(text.len(), |r| byte(r.primary));
            self.session.edit(&text, caret);
            ui.ctx().request_discard("edited");
            paint_text(ui, &self.session);
            return; // the rest of the layout is stale
        }
        // Double and triple clicks, and drags after them, select by units.
        // egui would select by its own words and lines.
        let (pressed, down, released, shift, pointer) = ui.input(|i| {
            let p = &i.pointer;
            (
                p.primary_pressed(),
                p.primary_down(),
                p.button_released(egui::PointerButton::Primary),
                i.modifiers.shift,
                p.interact_pos(),
            )
        });
        // egui moves its caret on a right click, but the session's stays in
        // a selection that the click is in.
        let secondary = egui::PointerButton::Secondary;
        let (right_pressed, right_click) = ui.input(|i| {
            let p = &i.pointer;
            let pressed = p.button_pressed(secondary);
            (
                pressed,
                pressed || p.button_down(secondary) || p.button_released(secondary),
            )
        });
        // Offset of the character under a point.
        let char_at = |p: Pos2| {
            let local = (p - out.galley_pos).to_pos2();
            let mut c = out.galley.cursor_from_pos(local.to_vec2());
            let n: usize = c.index.into();
            let before = n.checked_sub(1).and_then(|i| text.chars().nth(i));
            if local.x < out.galley.pos_from_cursor(c).min.x && before.is_some_and(|ch| ch != '\n')
            {
                c = egui::text::CCursor::new(n - 1);
            }
            byte(c)
        };
        // After the `>` under a point. egui would go to its nearer side, so
        // that clicking a `>` could make the one before it active.
        let mark_after = |p: Pos2| {
            let off = char_at(p);
            text[off..]
                .starts_with(MARK)
                .then_some(off + MARK.len_utf8())
        };
        let unit = if self.press.0 == 2 {
            Unit::Token
        } else {
            Unit::Run
        };
        let bubbles: Vec<_> = self
            .session
            .bubbles()
            .into_iter()
            .map(|b| {
                let index = egui::text::CCursor::new(text[..b.line].chars().count());
                let rect = out
                    .galley
                    .pos_from_cursor(index)
                    .translate(out.galley_pos.to_vec2());
                (b.seg, rect)
            })
            .collect();
        // A mark has its own click response. Use the press on the bubble's
        // row so the same gesture works on the marks and in the gaps.
        let bubble_click = (!shift
            && self.press.0 >= 2
            && (down || released)
            && out.response.interact_rect.contains(self.press.2))
        .then(|| {
            bubbles
                .iter()
                .find(|(_, rect)| rect.y_range().contains(self.press.2.y))
                .map(|&(seg, _)| seg)
        })
        .flatten();
        if bubble_click.is_none()
            && pressed
            && !shift
            && self.press.0 >= 2
            && out.response.is_pointer_button_down_on()
            && let Some(anchor) =
                pointer.and_then(|p| self.session.unit_at_offset(char_at(p), unit))
        {
            self.unit_drag = Some(UnitDrag {
                unit,
                anchor,
                timeline: false,
            });
        }
        let unit_drag = self.unit_drag.filter(|d| !d.timeline && (down || released));
        if let Some(seg) = bubble_click {
            self.mode = Mode::Text;
            self.session.select_bubble(seg);
        } else if let Some(d) = unit_drag {
            if let Some(target) =
                pointer.and_then(|p| self.session.unit_at_offset(char_at(p), d.unit))
            {
                self.session.select_units(d.anchor, target);
                ui.ctx().request_repaint();
                if self.session.text() != text {
                    ui.ctx().request_discard("switched branches");
                    paint_text(ui, &self.session);
                    return; // the layout is stale
                }
            }
        } else if right_click {
            if right_pressed
                && out.response.hovered()
                && let Some(p) = pointer
            {
                let off = byte(out.galley.cursor_from_pos(p - out.galley_pos));
                let pos = self.session.at_offset(off).0;
                self.right_click(pos, Some(off), false);
            }
        } else if pressed
            && !shift
            && self.press.0 == 1
            && out.response.is_pointer_button_down_on()
            && let Some(off) = pointer.and_then(mark_after)
        {
            // Even if egui's caret is already where it would put it.
            self.session.set_caret_offset(off);
            ui.ctx().request_repaint();
            if self.session.text() != text {
                ui.ctx().request_discard("switched branches");
                paint_text(ui, &self.session);
                return; // the layout is stale
            }
        } else if let Some(r) = range
            && (byte(r.primary), byte(r.secondary)) != (caret, anchor)
        {
            let pointer = ui.input(|i| i.pointer.primary_down() || i.pointer.any_released());
            let mut new = byte(r.primary);
            if !pointer {
                new = self.session.key_offset(new, r.h_pos.is_some());
            } else if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                new = mark_after(p).unwrap_or(new);
            }
            let from = byte(r.secondary);
            if r.is_empty() || new == from {
                self.session.set_caret_offset(new);
            } else {
                // A selection from outside of a bubble goes over all of it or
                // none of it: to the nearer side for the pointer, and for
                // keys, past it.
                let after = (!pointer).then_some(new > caret);
                self.session.select_offsets(from, new, after);
            }
            ui.ctx().request_repaint();
            if self.session.text() != text {
                ui.ctx().request_discard("switched branches");
                paint_text(ui, &self.session);
                return; // the layout is stale
            }
        }

        paint_text(ui, &self.session);

        // egui only follows carets that it moved itself.
        if moved {
            let c = egui::text::CCursor::new(caret_chars);
            let rect = out
                .galley
                .pos_from_cursor(c)
                .translate(out.galley_pos.to_vec2());
            ui.scroll_to_rect(rect.expand(8.0), None);
        }

        // egui hides its caret when the window is unfocused.
        let paint_caret = |off: usize, stroke| {
            let c = egui::text::CCursor::new(text[..off].chars().count());
            let rect = out
                .galley
                .pos_from_cursor(c)
                .translate(out.galley_pos.to_vec2());
            ui.painter()
                .vline(rect.center().x, rect.y_range().expand(1.5), stroke);
        };
        paint_caret(self.session.caret_offset(), caret_stroke(ui));
        if let Some(pos) = self.preview.filter(|p| self.is_valid(*p))
            && pos != self.session.caret()
            && let Some(off) = self.session.offset_of(pos)
        {
            paint_caret(off, GHOST_CARET);
        }

        if out.response.hovered()
            && let Some(p) = ui.input(|i| i.pointer.hover_pos().filter(|_| !i.pointer.any_down()))
        {
            let off = mark_after(p)
                .unwrap_or_else(|| byte(out.galley.cursor_from_pos(p - out.galley_pos)));
            *preview = Some(self.session.at_offset(off).0);
        }

        // A `>` for each branch, on its `MARK`. The active branch's is the
        // text color, as on the timeline.
        let p = palette(ui);
        for b in self.session.bubbles() {
            for i in 0..b.n {
                let x = |off: usize| {
                    let c = egui::text::CCursor::new(text[..off].chars().count());
                    out.galley
                        .pos_from_cursor(c)
                        .translate(out.galley_pos.to_vec2())
                };
                let center = x(b.line + i).center().lerp(x(b.line + i + 1).center(), 0.5);
                let color = if i == b.active {
                    ui.visuals().strong_text_color()
                } else {
                    p.dim
                };
                ui.painter()
                    .text(center, Align2::CENTER_CENTER, ">", FONT, color);
            }
        }
    }

    // ---- puzzle views ----

    fn view(&mut self, ui: &mut Ui, i: usize, height: f32) {
        // Edits since last frame can invalidate the preview.
        self.preview = self.preview.filter(|p| self.is_valid(*p));
        let pos = self.preview.unwrap_or(self.session.caret());
        let (state, caret_inversion) = self.session.eval(pos);
        let inversion = self.views[i].mode.resolve(caret_inversion);
        let state = if inversion == Inversion::Inverse {
            state.inv()
        } else {
            state
        };

        ui.push_id(("view", i), |ui| {
            let top = ui.cursor().top();
            ui.horizontal(|ui| {
                let mode = &mut self.views[i].mode;
                let mut attached = *mode == InversionMode::Attached;
                if ui.checkbox(&mut attached, "attached").changed() {
                    *mode = if attached {
                        InversionMode::Attached
                    } else {
                        InversionMode::Detached(inversion)
                    };
                }
                let mut inverse = inversion == Inversion::Inverse;
                if ui.checkbox(&mut inverse, "inverse").changed() {
                    *mode = InversionMode::Detached(inversion.flip());
                }
                if state.is_solved() {
                    ui.colored_label(palette(ui).comment, "solved");
                }
                if self.preview.is_some() {
                    ui.colored_label(palette(ui).dim, "preview");
                }
            });
            let size = vec2(
                ui.available_width(),
                (height - (ui.cursor().top() - top) - 8.0).max(60.0),
            );
            let (rect, r) = ui.allocate_exact_size(size, Sense::click_and_drag());
            let drag = r.drag_delta();
            if drag != egui::Vec2::ZERO {
                let angle = drag * (0.5 * std::f32::consts::TAU / rect.size().min_elem());
                let rotation = Quat::from_rotation_x(angle.y) * Quat::from_rotation_y(angle.x);
                // Apply each drag in screen space, before the existing orientation.
                let v = &mut self.views[i];
                v.orientation = (rotation * v.orientation).normalize();
            }
            let border = match inversion {
                Inversion::Normal => palette(ui).moves,
                Inversion::Inverse => palette(ui).niss,
            };
            let width = if self.mode == Mode::Twist(i) {
                2.0
            } else {
                0.5
            };
            let stroke = Stroke::new(width, border);
            ui.painter()
                .rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);

            let face = draw_cube(ui, rect, &state, &self.views[i], r.interact_pointer_pos());
            if r.clicked() || r.secondary_clicked() {
                self.mode = Mode::Twist(i);
                if let Some(face) = face {
                    let prime = if r.clicked() { "'" } else { "" };
                    self.twist(i, &format!("{face}{prime}"));
                }
            }
        });
    }

    // ---- timeline ----

    /// Draws each path like a line of text with a cell per token and per fork.
    /// The caret goes between cells.
    fn timeline(
        &mut self,
        ui: &mut Ui,
        preview: &mut Option<Pos>,
        command: &mut Option<MenuCommand>,
    ) {
        const ROW: f32 = 26.0;
        struct Node {
            x: f32,
            row: usize,
            len: f32,
        }
        fn layout(
            doc: &Doc,
            seg: SegId,
            x: f32,
            row: usize,
            next_row: &mut usize,
            out: &mut BTreeMap<SegId, Node>,
        ) {
            let n = notation::parse(&doc.get(seg).text).tokens.len();
            let len = n as f32 * TIMELINE_STEP;
            out.insert(seg, Node { x, row, len });
            for (i, &c) in doc.get(seg).children.iter().enumerate() {
                let row = if i == 0 {
                    row
                } else {
                    *next_row += 1;
                    *next_row - 1
                };
                layout(doc, c, x + len + TIMELINE_STEP, row, next_row, out);
            }
        }
        let doc = self.session.doc();
        let mut nodes = BTreeMap::new();
        let mut rows = 1;
        layout(doc, doc.root(), 0.0, 0, &mut rows, &mut nodes);
        let width = nodes.values().map(|n| n.x + n.len).fold(0.0, f32::max);
        let size = vec2(width + 40.0, rows as f32 * ROW + 20.0);

        let action = egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            let size = size.max(ui.available_size());
            let (rect, r) = ui.allocate_exact_size(size, Sense::click_and_drag());
            r.context_menu(|ui| *command = context_menu(ui, &self.session));
            let painter = ui.painter().clone();
            let pal = palette(ui);
            let origin = rect.min + vec2(20.0, 20.0);
            let at = |n: &Node, x: f32| origin + vec2(n.x + x, n.row as f32 * ROW);
            let path = doc.path(self.session.tip());
            let caret = self.session.caret();
            let mut stops: Vec<(Pos2, Pos)> = vec![];
            // Each token's dot, at its start.
            let mut cells: Vec<(Pos2, Pos)> = vec![];

            // Selection, behind everything else.
            let fill = ui.visuals().selection.bg_fill;
            let paint_band = |from: Pos2, to: Pos2| {
                let half = vec2(0.0, 7.0);
                painter.rect_filled(Rect::from_min_max(from - half, to + half), 0.0, fill);
            };
            let paint_connector = |from, to| {
                let vertical = Rect::from_two_pos(from, to);
                painter.rect_filled(vertical.expand2(vec2(2.0, 0.0)), 0.0, fill);
            };
            let index = |p: Pos| path.iter().position(|s| *s == p.seg).map(|i| (i, p.off));
            if let Some((parent, range)) = self.session.selected_branches() {
                let p = &nodes[&parent];
                let x = at(p, p.len).x;
                let branches = &doc.get(parent).children[range];
                let mut previous = None;
                let mut pending = Vec::new();
                for &seg in branches {
                    let corner = pos2(x, at(&nodes[&seg], 0.0).y);
                    if let Some(from) = previous {
                        paint_connector(from, corner);
                    }
                    previous = Some(corner);
                    pending.push((seg, corner));
                }
                while let Some((seg, corner)) = pending.pop() {
                    let node = &nodes[&seg];
                    let end = at(node, node.len);
                    paint_band(corner, end);
                    let mut from = end;
                    for &child in &doc.get(seg).children {
                        let corner = pos2(end.x, at(&nodes[&child], 0.0).y);
                        if corner.y != from.y {
                            paint_connector(from, corner);
                        }
                        from = corner;
                        pending.push((child, corner));
                    }
                }
            } else if let Some(a) = index(self.session.anchor())
                && let Some(c) = index(caret)
                && a != c
            {
                let (a, b) = (a.min(c), a.max(c));
                for i in a.0..=b.0 {
                    let node = &nodes[&path[i]];
                    let text = &doc.get(path[i]).text;
                    let x0 = if i == a.0 { timeline_x(text, a.1) } else { 0.0 };
                    let x1 = if i == b.0 {
                        timeline_x(text, b.1)
                    } else {
                        node.len
                    };
                    paint_band(at(node, x0), at(node, x1));
                    if i > a.0 {
                        // The fork's cell.
                        let p = &nodes[&path[i - 1]];
                        let (from, to) = (at(p, p.len), at(node, 0.0));
                        let corner = pos2(from.x, to.y);
                        paint_connector(from, corner);
                        paint_band(corner, to);
                    }
                }
            }

            for (&seg, node) in &nodes {
                let on_path = path.contains(&seg);
                // Neutral, so as not to clash with the tokens' colors.
                let (color, line) = if on_path {
                    let v = ui.visuals();
                    (v.strong_text_color(), Stroke::new(2.0, v.text_color()))
                } else {
                    (pal.dim, Stroke::new(1.0, pal.dim.gamma_multiply(0.6)))
                };
                let (start, end) = (at(node, 0.0), at(node, node.len));
                painter.line_segment([start, end], line);
                let s = doc.get(seg);
                stops.push((start, Pos { seg, off: 0 }));
                let tokens = notation::parse(&s.text).tokens;
                let colors = token_colors(pal, &s.text, &tokens);
                if let Some(parent) = s.parent {
                    let p = &nodes[&parent];
                    let from = at(p, p.len);
                    let corner = pos2(from.x, start.y);
                    painter.line_segment([from, corner], line);
                    let mark = [vec2(3.0, -4.0), vec2(10.0, 0.0), vec2(3.0, 4.0)];
                    let points = mark.iter().map(|v| corner + *v).collect();
                    painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
                    // The near edge of the fork's cell is the parent's end.
                    let text = &doc.get(parent).text;
                    let last = notation::parse(text).tokens.len();
                    let off = notation::gap_offset(text, last);
                    stops.push((corner, Pos { seg: parent, off }));
                }
                // Open groups: where each starts, and how many levels of
                // groups have closed inside it so far.
                let mut open: Vec<(usize, f32, u32)> = vec![];
                // Ends the groups from `idx` on, and passes `level` to the one outside them.
                let close = |open: &mut Vec<(usize, f32, u32)>, idx: usize, arc: bool| {
                    let inner = open[idx..].iter().map(|o| o.2).max().unwrap_or(0);
                    let level = inner + arc as u32;
                    open.truncate(idx);
                    if let Some(o) = open.last_mut() {
                        o.2 = o.2.max(level);
                    }
                    level
                };
                for (i, t) in tokens.iter().enumerate() {
                    // A line break is a tick in the gap before the token, but a
                    // comment always ends one, so it stands for it.
                    if i > 0
                        && tokens[i - 1].kind != TokenKind::Comment
                        && s.text[tokens[i - 1].span.end..t.span.start].contains('\n')
                    {
                        let g = at(node, i as f32 * TIMELINE_STEP);
                        painter.line_segment([g - vec2(0.0, 6.0), g + vec2(0.0, 6.0)], line);
                    }
                    let c = at(node, (i as f32 + 0.5) * TIMELINE_STEP);
                    let col = colors[i].gamma_multiply(if on_path { 1.0 } else { 0.55 });
                    match t.kind {
                        TokenKind::Open => {
                            open.push((t.span.start, c.x, 0));
                            painter.circle_stroke(c, 2.5, Stroke::new(1.0, col));
                        }
                        TokenKind::Close { open: start, .. } => {
                            if let Some(idx) = open.iter().position(|o| o.0 == start) {
                                let x = open[idx].1;
                                let level = close(&mut open, idx, true);
                                // Outer groups arc higher, so nested ones don't overlap.
                                let rise = (8.0 + 3.0 * (level - 1) as f32).min(ROW - 8.0);
                                let y = c.y - rise;
                                let pts = vec![
                                    pos2(x, c.y - 4.0),
                                    pos2(x, y),
                                    pos2(c.x, y),
                                    pos2(c.x, c.y - 4.0),
                                ];
                                painter.add(Shape::line(pts, Stroke::new(1.0, col)));
                            }
                            painter.circle_stroke(c, 2.5, Stroke::new(1.0, col));
                        }
                        TokenKind::BadClose => {
                            if let Some(idx) = open.len().checked_sub(1) {
                                close(&mut open, idx, false);
                            }
                            painter.circle_filled(c, 4.0, col);
                        }
                        TokenKind::Sep => {
                            painter.circle_stroke(c, 2.0, Stroke::new(1.0, col));
                        }
                        _ => {
                            painter.circle_filled(c, 4.0, col);
                        }
                    }
                    cells.push((
                        c,
                        Pos {
                            seg,
                            off: t.span.start,
                        },
                    ));
                    let right = c + vec2(TIMELINE_STEP / 2.0, 0.0);
                    stops.push((
                        right,
                        Pos {
                            seg,
                            off: t.span.end,
                        },
                    ));
                }
            }
            let paint_caret = |pos: Pos, stroke| {
                let c = at(
                    &nodes[&pos.seg],
                    timeline_x(&doc.get(pos.seg).text, pos.off),
                );
                painter.line_segment([c - vec2(0.0, 10.0), c + vec2(0.0, 10.0)], stroke);
            };
            if let Some(pos) = self.preview.filter(|p| *p != caret) {
                paint_caret(pos, GHOST_CARET);
            }
            paint_caret(caret, caret_stroke(ui));

            let pointer = r.interact_pointer_pos().or(r.hover_pos())?;
            // Whether a selection can go between them.
            let related = |a: SegId, b: SegId| doc.path(a).contains(&b) || doc.path(b).contains(&a);
            let nearest_to = |p: Pos2, anchor: Option<Pos>| {
                stops
                    .iter()
                    .filter(|(_, pos)| anchor.is_none_or(|a| related(a.seg, pos.seg)))
                    .min_by(|a, b| a.0.distance(p).total_cmp(&b.0.distance(p)))
                    .copied()
            };
            // Drags pick the branch by row, even past its end, out of those that
            // have started by then.
            let drag_seg = |p: Pos2, anchor: SegId| {
                nodes
                    .iter()
                    .filter(|(s, _)| related(anchor, **s))
                    .filter(|(s, n)| **s == doc.root() || at(n, 0.0).x - TIMELINE_STEP / 2.0 <= p.x)
                    .min_by(|(_, a), (_, b)| {
                        let dy = |n: &Node| (at(n, 0.0).y - p.y).abs();
                        let dx = |n: &Node| (p.x - at(n, n.len).x).max(at(n, 0.0).x - p.x).max(0.0);
                        dy(a).total_cmp(&dy(b)).then(dx(a).total_cmp(&dx(b)))
                    })
                    .map(|(s, _)| *s)
            };
            // The nearest of `points` on that branch.
            let drag_target = |points: &[(Pos2, Pos)], p: Pos2, anchor: SegId| {
                let seg = drag_seg(p, anchor)?;
                points
                    .iter()
                    .filter(|(_, pos)| pos.seg == seg)
                    .min_by(|a, b| (a.0.x - p.x).abs().total_cmp(&(b.0.x - p.x).abs()))
                    .map(|s| s.1)
            };
            let (shift, pressed, down, released) = ui.input(|i| {
                let p = &i.pointer;
                (
                    i.modifiers.shift,
                    p.primary_pressed(),
                    p.primary_down(),
                    p.any_released(),
                )
            });
            // Double and triple clicks, and drags after them, select by units.
            let unit = if self.press.0 == 2 {
                Unit::Token
            } else {
                Unit::Run
            };
            if pressed && !shift && self.press.0 >= 2 && r.is_pointer_button_down_on() {
                let cell = cells
                    .iter()
                    .min_by(|a, b| a.0.distance(pointer).total_cmp(&b.0.distance(pointer)))
                    .filter(|c| c.0.distance(pointer) <= 12.0)?;
                let anchor = self.session.unit_on_timeline(cell.1, unit)?;
                return Some(TimelineAction::StartUnits(unit, anchor));
            }
            if let Some(d) = self.unit_drag.filter(|d| d.timeline && (down || released)) {
                let cell = drag_target(&cells, pointer, d.anchor.1.seg)?;
                return Some(TimelineAction::ExtendUnits(
                    self.session.unit_on_timeline(cell, d.unit)?,
                ));
            }
            if r.dragged_by(egui::PointerButton::Primary) {
                // Like in text: a drag selects, and Shift extends instead.
                let origin = ui.input(|i| i.pointer.press_origin());
                let from = match origin.filter(|_| r.drag_started() && !shift) {
                    Some(p) => Some(nearest_to(p, None)?.1),
                    None => None,
                };
                let to = drag_target(&stops, pointer, from.unwrap_or(self.session.anchor()).seg)?;
                return Some(TimelineAction::Select { from, to });
            }
            if r.secondary_clicked() {
                return Some(TimelineAction::Menu(nearest_to(pointer, None)?.1));
            }
            let nearest = nearest_to(pointer, shift.then(|| self.session.anchor()))?;
            if nearest.0.distance(pointer) > 12.0 {
                return None;
            }
            if r.clicked() && shift {
                return Some(TimelineAction::Select {
                    from: None,
                    to: nearest.1,
                });
            }
            if r.clicked() {
                return Some(TimelineAction::Go(nearest.1));
            }
            let pos = nearest.1;
            *preview = Some(pos);
            let text = &doc.get(pos.seg).text;
            r.on_hover_text_at_pointer(egui::RichText::new(hover_text(text, pos.off)).monospace());
            None
        });
        if let Some(n) = self.session.stm() {
            let painter = ui.painter();
            let color = ui.visuals().text_color();
            let galley = painter.layout_no_wrap(format!("{n} STM"), FONT, color);
            let corner = action.inner_rect.right_bottom() - vec2(10.0, 6.0);
            let rect = Align2::RIGHT_BOTTOM.anchor_size(corner, galley.size());
            painter.rect_filled(rect.expand(3.0), 3.0, ui.visuals().panel_fill);
            painter.galley(rect.min, galley, color);
        }
        if let Some(action) = action.inner {
            match action {
                TimelineAction::Go(pos) => self.session.go_to_on_timeline(pos),
                TimelineAction::Select {
                    from: Some(from),
                    to,
                } => self.session.set_selection(from, to),
                TimelineAction::Select { from: None, to } => self.session.extend_to(to),
                TimelineAction::StartUnits(unit, anchor) => {
                    self.unit_drag = Some(UnitDrag {
                        unit,
                        anchor,
                        timeline: true,
                    });
                    self.session.select_units(anchor, anchor);
                }
                TimelineAction::ExtendUnits(target) => {
                    let anchor = self.unit_drag.expect("dragging by units").anchor;
                    self.session.select_units(anchor, target);
                }
                TimelineAction::Menu(pos) => {
                    let off = self.session.offset_of(pos);
                    self.right_click(pos, off, true);
                }
            }
            ui.ctx().request_repaint();
        }
    }
}

/// Draws the context menu, and returns the item clicked. Without a
/// selection, it only pastes. FMC tools need a selection of text, and
/// inverting needs one in a single segment that cuts no group.
fn context_menu(ui: &mut Ui, s: &Session) -> Option<MenuCommand> {
    let mut out = None;
    let mut item = |ui: &mut Ui, enabled: bool, text: &str, key: Option<Key>, command| {
        let mut button = egui::Button::new(text);
        if let Some(k) = key {
            let shortcut = egui::KeyboardShortcut::new(Modifiers::COMMAND, k);
            button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut));
        }
        if ui.add_enabled(enabled, button).clicked() {
            out = Some(command);
        }
    };
    if s.anchor_offset() == s.caret_offset() {
        item(ui, true, "Paste", Some(Key::V), MenuCommand::Paste);
        return out;
    }
    item(ui, true, "Cut", Some(Key::X), MenuCommand::Cut);
    item(ui, true, "Copy", Some(Key::C), MenuCommand::Copy);
    item(ui, true, "Paste", Some(Key::V), MenuCommand::Paste);
    ui.separator();
    let text = s.selects_text();
    item(ui, text, "Group", None, MenuCommand::Wrap("(", ")"));
    item(ui, text, "Commutator", None, MenuCommand::Binary(','));
    item(ui, text, "Conjugate", None, MenuCommand::Binary(':'));
    item(ui, text, "NISS", None, MenuCommand::Wrap("^(", ")"));
    ui.separator();
    item(ui, s.can_expand(), "Expand", None, MenuCommand::Expand);
    item(ui, s.can_invert(), "Invert", None, MenuCommand::Invert);
    out
}

/// Width of a cell on the timeline.
const TIMELINE_STEP: f32 = 12.0;

/// How far along its segment an offset is on the timeline.
/// Inverse of `notation::gap_offset`, except within tokens and whitespace.
fn timeline_x(text: &str, off: usize) -> f32 {
    let (gap, partway) = notation::gap(text, off);
    (gap as f32 + if partway { 0.5 } else { 0.0 }) * TIMELINE_STEP
}

fn hover_text(text: &str, off: usize) -> String {
    if text.is_empty() {
        return "(empty)".to_owned();
    }
    format!("{}|{}", &text[..off], &text[off..])
}

/// Colors the text of a path. Bubbles are just left room for, each on a line
/// of its own; see `App::bubble`.
fn highlight(ui: &Ui, text: &str) -> egui::text::LayoutJob {
    const SPACE: char = '\u{a0}'; // doesn't wrap
    let (space, row) = ui.fonts_mut(|f| (f.glyph_width(&FONT, SPACE), f.row_height(&FONT)));
    let p = palette(ui);
    let mut job = egui::text::LayoutJob::default();
    for seg in text.split_inclusive([OPEN, MARK, END]) {
        let (seg, special) = match seg.chars().next_back() {
            Some(c @ (OPEN | MARK | END)) => (&seg[..seg.len() - 1], Some(c)),
            _ => (seg, None),
        };
        let mut add = |s: &str, color| job.append(s, 0.0, TextFormat::simple(FONT, color));
        let parsed = notation::parse(seg);
        let colors = token_colors(p, seg, &parsed.tokens);
        let mut at = 0;
        for (i, t) in parsed.tokens.iter().enumerate() {
            add(&seg[at..t.span.start], p.dim);
            let unclosed = parsed.open.iter().any(|g| g.span == t.span);
            let color = if unclosed { p.error } else { colors[i] };
            add(&seg[t.span.clone()], color);
            at = t.span.end;
        }
        add(&seg[at..], p.dim);
        match special {
            // A bubble's line breaks.
            Some(OPEN | END) => add("\n", p.dim),
            Some(_) => {
                let size = FONT.size * MARK_WIDTH / space;
                let mut format = TextFormat::simple(FontId::monospace(size), p.dim);
                format.line_height = Some(row);
                job.append(&SPACE.to_string(), 0.0, format);
            }
            None => {}
        }
    }
    job
}

/// Draws the cube and returns the face under the pointer.
fn draw_cube(
    ui: &Ui,
    rect: Rect,
    state: &Perm,
    view: &View,
    pointer: Option<Pos2>,
) -> Option<char> {
    let scale = rect.size().min_elem() / 5.6;
    let project = |v: [f32; 3]| {
        let Vec3 { x, y, z } = view.orientation * Vec3::from_array(v);
        let k = scale * 14.0 / (14.0 - z);
        rect.center() + vec2(x * k, -y * k)
    };
    let mut hit = None;
    for (i, &(pos, normal)) in STICKERS.iter().enumerate() {
        let axis = normal.iter().position(|c| *c != 0).expect("unit normal");
        let corner = |a: f32, b: f32| {
            let mut v = pos.map(|c| c as f32);
            v[axis] *= 1.5;
            v[(axis + 1) % 3] += a;
            v[(axis + 2) % 3] += b;
            project(v)
        };
        let quad = |h: f32| vec![corner(-h, -h), corner(h, -h), corner(h, h), corner(-h, h)];
        let outer = quad(0.5);
        // Cull by winding on screen, so perspective is accounted for.
        let area: f32 = (0..4)
            .map(|j| {
                let (p, q) = (outer[j], outer[(j + 1) % 4]);
                p.x * q.y - q.x * p.y
            })
            .sum();
        if area * normal[axis] as f32 >= 0.0 {
            continue;
        }
        let home = STICKERS[state.0[i] as usize].1;
        let color = match puzzle::face_of_normal(home) {
            'R' => Color32::from_rgb(220, 40, 40),
            'L' => Color32::from_rgb(255, 140, 0),
            'U' => Color32::from_rgb(245, 245, 245),
            'D' => Color32::from_rgb(250, 220, 40),
            'F' => Color32::from_rgb(40, 180, 70),
            _ => Color32::from_rgb(50, 90, 230),
        };
        ui.painter().add(Shape::convex_polygon(
            outer.clone(),
            Color32::BLACK,
            Stroke::NONE,
        ));
        ui.painter()
            .add(Shape::convex_polygon(quad(0.44), color, Stroke::NONE));
        if let Some(p) = pointer {
            let inside = (0..4).all(|j| {
                let (a, b) = (outer[j], outer[(j + 1) % 4]);
                let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                cross * area >= 0.0
            });
            if inside {
                hit = Some(puzzle::face_of_normal(normal));
            }
        }
    }
    hit
}

fn random_scramble() -> String {
    use std::hash::{BuildHasher, RandomState};
    let mut seed = RandomState::new().hash_one(0);
    let mut below = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n) as usize
    };
    let mut last = [9, 9];
    let mut out = vec![];
    while out.len() < 25 {
        let f = below(6);
        // Not the same face twice, nor three times on one axis.
        if f == last[0] || (f / 2 == last[0] / 2 && f / 2 == last[1] / 2) {
            continue;
        }
        last = [f, last[0]];
        let suffix = ["", "2", "'"][below(3)];
        out.push(format!("{}{suffix}", ["R", "L", "U", "D", "F", "B"][f]));
    }
    out.join(" ")
}
