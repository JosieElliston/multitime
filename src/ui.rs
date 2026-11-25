//! egui front end. Holds only view state; everything else is in `Session`.

use crate::doc::{self, Doc, FORK, Pos, SegId};
use crate::notation::{self, GroupKind, Inversion, TokenKind};
use crate::puzzle::{self, Perm, STICKERS};
use crate::session::{InversionMode, Session};
use eframe::egui::{
    self, Align2, Color32, Event, FontId, Id, Key, Modifiers, Pos2, Rect, Sense, Shape, Stroke,
    TextFormat, Ui, Vec2, pos2, vec2,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

const ACCENT: Color32 = Color32::from_rgb(255, 170, 60);
const MOVE: Color32 = Color32::from_rgb(110, 170, 255);
const COMMENT: Color32 = Color32::from_rgb(110, 190, 110);
const GROUP: Color32 = Color32::from_rgb(190, 140, 255);
const NISS: Color32 = Color32::from_rgb(255, 120, 200);
const ERROR: Color32 = Color32::from_rgb(255, 80, 80);
const DIM: Color32 = Color32::from_gray(120);
/// Caret in both the text box and the map.
const CARET: Stroke = Stroke { width: 2.0, color: Color32::WHITE };
const EDITOR: &str = "path editor";
const FONT: FontId = FontId::monospace(15.0);

struct View {
    mode: InversionMode,
    yaw: f32,
    pitch: f32,
}

/// Where keys go. The text box always has egui's focus, so that the caret,
/// selection and clipboard work the same either way. When twisting, the keys
/// that twist (and typing in general) are taken before it sees them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Keys {
    Type,
    /// Twist in this view.
    Twist(usize),
}

/// What a gesture on the map does.
enum MapAction {
    Go(Pos),
    /// Goes there and makes it the tip.
    Shorten(Pos),
    /// Selects from `from`, or else from where the selection already starts.
    Select { from: Option<Pos>, to: Pos },
}

pub struct App {
    pub session: Session,
    path: PathBuf,
    saved: Doc,
    status: String,
    views: Vec<View>,
    keys: Keys,
    /// Position hovered last frame; shown in the views instead of the caret.
    preview: Option<Pos>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        self.show(ui);
    }
}

fn token_color(kind: TokenKind) -> Color32 {
    match kind {
        TokenKind::Move => MOVE,
        TokenKind::Pause => DIM,
        TokenKind::Comment => COMMENT,
        TokenKind::Close { kind: GroupKind::Niss, .. } => NISS,
        TokenKind::Open | TokenKind::Sep | TokenKind::Close { .. } => GROUP,
        TokenKind::BadClose | TokenKind::Stray | TokenKind::Error => ERROR,
    }
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
        let view = |mode| View { mode, yaw: 0.6, pitch: 0.5 };
        Self {
            saved: doc.clone(),
            session: Session::new(doc),
            path,
            status: String::new(),
            views: vec![
                view(InversionMode::Attached),
                view(InversionMode::Detached(Inversion::Inverse)),
            ],
            keys: Keys::Twist(0),
            preview: None,
        }
    }

    pub fn show(&mut self, ui: &mut Ui) {
        // Keep the text box focused. See `Keys`.
        ui.ctx().options_mut(|o| {
            o.input_options.surrender_focus_on = egui::SurrenderFocusOn::Never;
        });
        ui.memory_mut(|m| m.request_focus(Id::unique(EDITOR)));
        self.shortcuts(ui);
        let mut preview = None;
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("map").resizable(true).default_size(190.0).show(ui, |ui| {
            self.map(ui, &mut preview);
        });
        egui::Panel::right("views").resizable(true).default_size(420.0).show(ui, |ui| {
            let height = ui.available_height() / self.views.len() as f32;
            for i in 0..self.views.len() {
                self.view(ui, i, height);
            }
        });
        egui::CentralPanel::default().show(ui, |ui| {
            // The text box is at least as tall as the panel, and the panel scrolls.
            let height = ui.available_height();
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                self.editor(ui, height, &mut preview);
            });
        });
        if preview != self.preview {
            self.preview = preview;
            ui.ctx().request_repaint();
        }
    }

    fn is_valid(&self, p: Pos) -> bool {
        let doc = self.session.doc();
        doc.ids().any(|s| s == p.seg) && doc.get(p.seg).text.is_char_boundary(p.off)
    }

    fn shortcuts(&mut self, ui: &mut Ui) {
        let key = |m, k| ui.input_mut(|i| i.consume_key(m, k));
        let (cmd, shift) = (Modifiers::COMMAND, Modifiers::SHIFT);
        let s = &mut self.session;
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
        if key(cmd, Key::D) {
            s.split();
        }
        if key(cmd, Key::J) {
            s.join();
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
        // Would insert a tab or move egui's focus.
        key(Modifiers::NONE, Key::Tab);
        // Escape cycles: typing, then twisting in each view.
        if key(Modifiers::NONE, Key::Escape) {
            self.keys = match self.keys {
                Keys::Type => Keys::Twist(0),
                Keys::Twist(i) if i + 1 < self.views.len() => Keys::Twist(i + 1),
                Keys::Twist(_) => Keys::Type,
            };
        }
        let Keys::Twist(view) = self.keys else {
            return;
        };
        let none = Modifiers::NONE;
        ui.input_mut(|input| {
            input.events.retain(|event| {
                if matches!(event, egui::Event::Text(_) | egui::Event::Ime(_)) {
                    return false;
                }
                let Event::Key { key, physical_key, pressed: true, repeat, modifiers } = *event else {
                    return true;
                };
                // Deleting is for typing, like inserting. The last few are
                // egui's Ctrl+letter deletions.
                if matches!(key, Key::Backspace | Key::Delete)
                    || (modifiers.ctrl && matches!(key, Key::H | Key::K | Key::U | Key::W))
                {
                    return false;
                }
                if modifiers != none && modifiers != shift {
                    return true;
                }
                if modifiers == none {
                    match key {
                        Key::ArrowLeft => self.session.step(false),
                        Key::ArrowRight => self.session.step(true),
                        Key::ArrowUp => self.session.cycle_sibling(-1),
                        Key::ArrowDown => self.session.cycle_sibling(1),
                        _ => {}
                    }
                    if matches!(
                        key,
                        Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown
                    ) {
                        return false;
                    }
                }
                if key == Key::Enter {
                    return false; // would type a newline
                }
                let Some(name) = cube_move(physical_key.unwrap_or(key)) else {
                    return true;
                };
                if !repeat {
                    let (face, amount) = name.split_at(1);
                    if modifiers.shift && "RLUDFB".contains(face) {
                        self.twist(view, &format!("{face}w{amount}"));
                    } else {
                        self.twist(view, name);
                    }
                }
                false
            });
        });
    }

    fn twist(&mut self, view: usize, mv: &str) {
        let (_, caret) = self.session.eval(self.session.caret());
        self.session.twist(mv, self.views[view].mode.resolve(caret));
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
            let button = |ui: &mut Ui, enabled, text: &str, hint: &str| {
                ui.add_enabled(enabled, egui::Button::new(text)).on_hover_text(hint).clicked()
            };
            if button(ui, true, "Fork", "New branch from the caret (Cmd+Enter)") {
                s.fork();
            }
            if button(ui, true, "Split", "Break the segment at the caret (Cmd+D)") {
                s.split();
            }
            if button(ui, s.doc().can_join(seg), "Join", "Merge into the parent (Cmd+J)") {
                s.join();
            }
            let has_parent = s.doc().get(seg).parent.is_some();
            if button(ui, has_parent, "Delete branch", "Delete this segment and all after it") {
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

    fn editor(&mut self, ui: &mut Ui, min_height: f32, preview: &mut Option<Pos>) {
        let id = Id::unique(EDITOR);
        let tip = self.session.tip();
        let mut text = self.session.doc().path_text(tip);
        let doc_offset =
            |s: &Session, pos| s.doc().path_offset(s.tip(), pos).expect("position on path");
        let caret = doc_offset(&self.session, self.session.caret());

        // The session owns the caret and selection.
        let caret_chars = text[..caret].chars().count();
        let anchor = doc_offset(&self.session, self.session.anchor());
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

        let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, width: f32| {
            let mut job = highlight(buf.as_str());
            job.wrap.max_width = width;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        // The tip's children go within the box, under the text.
        let children = self.session.doc().get(tip).children.clone();
        let children_height = if children.is_empty() { 0 } else { 24 };
        let margin = egui::Margin { left: 4, right: 4, top: 2, bottom: 2 + children_height };
        ui.visuals_mut().text_cursor.stroke = CARET;
        ui.visuals_mut().text_cursor.blink = false;
        if self.keys != Keys::Type {
            ui.visuals_mut().selection.stroke.width = 0.0; // focus border
        }
        let out = egui::TextEdit::multiline(&mut text)
            .id(id)
            .margin(margin)
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

        // `out.cursor_range` doesn't include this frame's clicks, but this does.
        let range = out.state.cursor.char_range();
        if out.response.is_pointer_button_down_on() {
            self.keys = Keys::Type;
        }
        // Copying and cutting leave out the forks.
        ui.output_mut(|o| {
            for command in &mut o.commands {
                if let egui::OutputCommand::CopyText(s) = command {
                    *s = doc::without_forks(s);
                }
            }
        });

        let byte = |c: egui::text::CCursor| {
            let n: usize = c.index.into();
            text.char_indices().nth(n).map_or(text.len(), |(i, _)| i)
        };
        if out.response.changed() {
            self.session.edit(&text, range.map_or(text.len(), |r| byte(r.primary)));
            return; // the rest of the layout is stale
        }
        if let Some(r) = range
            && (byte(r.primary), byte(r.secondary)) != (caret, anchor)
        {
            let doc = self.session.doc();
            let mut new = byte(r.primary);
            // The caret can't rest within a fork. Keys cross it in the direction
            // of motion. The pointer goes to the nearest end, since a held drag
            // would otherwise flip between the two.
            let pointer = ui.input(|i| i.pointer.primary_down() || i.pointer.any_released());
            for (start, seg) in doc.path_starts(tip).into_iter().zip(doc.path(tip)) {
                let end = start + doc.get(seg).text.len();
                let next = end + FORK.len();
                if end < new && new < next && next <= text.len() {
                    let forward = if pointer { new - end >= 2 } else { new > caret };
                    new = if forward { next } else { end };
                    break;
                }
            }
            let pos = doc.path_pos(tip, new);
            if r.is_empty() {
                self.session.go_to(pos);
            } else {
                let anchor = doc.path_pos(tip, byte(r.secondary));
                self.session.set_selection(anchor, pos);
            }
            ui.ctx().request_repaint();
        }

        // egui only follows carets that it moved itself.
        if moved {
            let c = egui::text::CCursor::new(caret_chars);
            let rect = out.galley.pos_from_cursor(c).translate(out.galley_pos.to_vec2());
            ui.scroll_to_rect(rect.expand(8.0), None);
        }

        // egui hides its caret when the window is unfocused.
        let c = egui::text::CCursor::new(text[..doc_offset(&self.session, self.session.caret())].chars().count());
        let rect = out.galley.pos_from_cursor(c).translate(out.galley_pos.to_vec2());
        ui.painter().vline(
            rect.center().x,
            rect.y_range().expand(1.5),
            CARET,
        );

        // Paint the siblings at each fork.
        let mut clicked = None;
        let path = self.session.doc().path(tip);
        let starts = self.session.doc().path_starts(tip);
        for i in 1..path.len() {
            let mark = text[..starts[i] - 2].chars().count();
            let rect = out.galley.pos_from_cursor(egui::text::CCursor::new(mark));
            let rect = rect.translate(out.galley_pos.to_vec2());
            let sibs = self.session.doc().siblings(path[i]).to_vec();
            self.fork_marks(ui, rect.left_center(), &sibs, Some(path[i]), &mut clicked, preview);
        }
        if !children.is_empty() {
            let at = out.galley_pos + vec2(0.0, out.galley.size().y + 13.0);
            self.fork_marks(ui, at, &children, None, &mut clicked, preview);
        }
        if let Some(seg) = clicked {
            self.session.select(seg);
        }
    }

    fn fork_marks(
        &self,
        ui: &mut Ui,
        at: Pos2,
        segs: &[SegId],
        selected: Option<SegId>,
        clicked: &mut Option<SegId>,
        preview: &mut Option<Pos>,
    ) {
        for (i, &seg) in segs.iter().enumerate() {
            let center = at + vec2(9.0 + 24.0 * i as f32, 0.0);
            let rect = Rect::from_center_size(center, vec2(20.0, 16.0));
            let r = ui.interact(rect, Id::unique(("fork mark", seg)), Sense::click());
            let is_selected = selected == Some(seg);
            let color = if is_selected { ACCENT } else { DIM };
            // Covers the real `>`, which isn't transparent when it's selected.
            let background = ui.visuals().text_edit_bg_color();
            ui.painter().rect_filled(rect.shrink2(vec2(-1.0, 2.0)), 0.0, background);
            if is_selected || r.hovered() {
                ui.painter().rect_filled(rect, 4.0, color.gamma_multiply(0.25));
            }
            ui.painter().text(center, Align2::CENTER_CENTER, ">", FONT, color);
            if r.clicked() && !is_selected {
                *clicked = Some(seg);
            }
            if r.hovered() && !is_selected {
                *preview = Some(self.session.doc().end(seg));
            }
            r.on_hover_text(match self.session.doc().get(seg).text.as_str() {
                "" => "(empty)",
                s => s,
            });
        }
    }

    // ---- puzzle views ----

    fn view(&mut self, ui: &mut Ui, i: usize, height: f32) {
        // Edits since last frame can invalidate the preview.
        self.preview = self.preview.filter(|p| self.is_valid(*p));
        let pos = self.preview.unwrap_or(self.session.caret());
        let (state, caret_inversion) = self.session.eval(pos);
        let inversion = self.views[i].mode.resolve(caret_inversion);
        let state = if inversion == Inversion::Inverse { state.inv() } else { state };

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
                    ui.colored_label(COMMENT, "solved");
                }
                if self.preview.is_some() {
                    ui.colored_label(DIM, "preview");
                }
            });
            let size = vec2(ui.available_width(), (height - (ui.cursor().top() - top) - 8.0).max(60.0));
            let (rect, r) = ui.allocate_exact_size(size, Sense::click_and_drag());
            if r.dragged() {
                let d = r.drag_delta() * 0.01;
                let v = &mut self.views[i];
                v.yaw -= d.x;
                v.pitch = (v.pitch + d.y).clamp(-1.5, 1.5);
            }
            let border = match inversion {
                Inversion::Normal => MOVE,
                Inversion::Inverse => NISS,
            };
            let width = if self.keys == Keys::Twist(i) { 2.0 } else { 0.5 };
            let stroke = Stroke::new(width, border);
            ui.painter().rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);

            let face = draw_cube(ui, rect, &state, &self.views[i], r.interact_pointer_pos());
            if r.clicked() || r.secondary_clicked() {
                self.keys = Keys::Twist(i);
                if let Some(face) = face {
                    let prime = if r.clicked() { "'" } else { "" };
                    self.twist(i, &format!("{face}{prime}"));
                }
            }
        });
    }

    // ---- map ----

    fn map(&mut self, ui: &mut Ui, preview: &mut Option<Pos>) {
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
            let len = (n + 1) as f32 * MAP_STEP;
            out.insert(seg, Node { x, row, len });
            for (i, &c) in doc.get(seg).children.iter().enumerate() {
                let row = if i == 0 {
                    row
                } else {
                    *next_row += 1;
                    *next_row - 1
                };
                layout(doc, c, x + len + MAP_STEP, row, next_row, out);
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
            let painter = ui.painter().clone();
            let origin = rect.min + vec2(20.0, 20.0);
            let at = |n: &Node, x: f32| origin + vec2(n.x + x, n.row as f32 * ROW);
            let path = doc.path(self.session.tip());
            let caret = self.session.caret();
            let mut stops: Vec<(Pos2, Pos)> = vec![];

            // Selection, behind everything else.
            let index = |p: Pos| path.iter().position(|s| *s == p.seg).map(|i| (i, p.off));
            if let Some(a) = index(self.session.anchor())
                && let Some(c) = index(caret)
                && a != c
            {
                let (a, b) = (a.min(c), a.max(c));
                let fill = ui.visuals().selection.bg_fill;
                for i in a.0..=b.0 {
                    let node = &nodes[&path[i]];
                    let text = &doc.get(path[i]).text;
                    let x0 = if i == a.0 { map_x(text, a.1) } else { 0.0 };
                    let x1 = if i == b.0 { map_x(text, b.1) } else { node.len };
                    let half = vec2(0.0, 7.0);
                    let band = Rect::from_min_max(at(node, x0) - half, at(node, x1) + half);
                    painter.rect_filled(band, 0.0, fill);
                    if i > a.0 {
                        let p = &nodes[&path[i - 1]];
                        let (from, to) = (at(p, p.len), at(node, 0.0));
                        let v = Rect::from_two_pos(from, pos2(from.x, to.y));
                        let h = Rect::from_min_max(pos2(from.x, to.y - 7.0), to + half);
                        painter.rect_filled(v.expand2(vec2(2.0, 0.0)), 0.0, fill);
                        painter.rect_filled(h, 0.0, fill);
                    }
                }
            }

            for (&seg, node) in &nodes {
                let on_path = path.contains(&seg);
                let color = if on_path { ACCENT } else { DIM };
                let line = Stroke::new(if on_path { 2.0 } else { 1.0 }, color.gamma_multiply(0.6));
                let (start, end) = (at(node, 0.0), at(node, node.len));
                painter.line_segment([start, end], line);
                if let Some(p) = doc.get(seg).parent {
                    let p = &nodes[&p];
                    let from = at(p, p.len);
                    let corner = pos2(from.x, start.y);
                    painter.add(Shape::line(vec![from, corner, start], line));
                }
                let stop = Rect::from_center_size(end, Vec2::splat(7.0));
                painter.rect_filled(stop, 1.0, color);
                let s = doc.get(seg);
                stops.push((start, Pos { seg, off: 0 }));
                let tokens = notation::parse(&s.text).tokens;
                let mut open_xs = vec![];
                for (i, t) in tokens.iter().enumerate() {
                    let c = at(node, (i + 1) as f32 * MAP_STEP);
                    let col = token_color(t.kind).gamma_multiply(if on_path { 1.0 } else { 0.55 });
                    match t.kind {
                        TokenKind::Open => {
                            open_xs.push((t.span.start, c.x));
                            painter.circle_stroke(c, 2.5, Stroke::new(1.0, col));
                        }
                        TokenKind::Close { open, .. } => {
                            if let Some((_, x)) = open_xs.iter().find(|o| o.0 == open) {
                                let y = c.y - 8.0;
                                let pts = vec![
                                    pos2(*x, c.y - 4.0),
                                    pos2(*x, y),
                                    pos2(c.x, y),
                                    pos2(c.x, c.y - 4.0),
                                ];
                                painter.add(Shape::line(pts, Stroke::new(1.0, col)));
                            }
                            painter.circle_stroke(c, 2.5, Stroke::new(1.0, col));
                        }
                        TokenKind::Sep => {
                            painter.circle_stroke(c, 2.0, Stroke::new(1.0, col));
                        }
                        _ => {
                            painter.circle_filled(c, 4.0, col);
                        }
                    }
                    stops.push((c, Pos { seg, off: t.span.end }));
                }
                stops.push((end, Pos { seg, off: s.text.len() }));
                if caret.seg == seg {
                    let c = at(node, map_x(&s.text, caret.off));
                    painter.line_segment([c - vec2(0.0, 10.0), c + vec2(0.0, 10.0)], CARET);
                }
            }

            let pointer = r.interact_pointer_pos().or(r.hover_pos())?;
            let nearest_to = |p: Pos2, path_only: bool| {
                stops
                    .iter()
                    .filter(|(_, pos)| !path_only || path.contains(&pos.seg))
                    .min_by(|a, b| a.0.distance(p).total_cmp(&b.0.distance(p)))
                    .copied()
            };
            let shift = ui.input(|i| i.modifiers.shift);
            if r.dragged_by(egui::PointerButton::Primary) {
                let to = nearest_to(pointer, true)?.1;
                // Like in text: a drag selects, and Shift extends instead.
                let origin = ui.input(|i| i.pointer.press_origin());
                let from = match origin.filter(|_| r.drag_started() && !shift) {
                    Some(p) => Some(nearest_to(p, true)?.1),
                    None => None,
                };
                return Some(MapAction::Select { from, to });
            }
            let nearest = nearest_to(pointer, shift)?;
            if nearest.0.distance(pointer) > 12.0 {
                return None;
            }
            painter.circle_stroke(nearest.0, 7.0, Stroke::new(1.0, Color32::WHITE));
            if r.clicked() && shift {
                return Some(MapAction::Select { from: None, to: nearest.1 });
            }
            if r.double_clicked() {
                return Some(MapAction::Shorten(nearest.1));
            }
            if r.clicked() {
                return Some(MapAction::Go(nearest.1));
            }
            let pos = nearest.1;
            *preview = Some(pos);
            let text = &doc.get(pos.seg).text;
            r.on_hover_text_at_pointer(egui::RichText::new(hover_text(text, pos.off)).monospace());
            None
        });
        if let Some(action) = action.inner {
            match action {
                MapAction::Go(pos) => self.session.go_to(pos),
                MapAction::Shorten(pos) => {
                    self.session.set_tip(pos.seg);
                    self.session.go_to(pos);
                }
                MapAction::Select { from: Some(from), to } => self.session.set_selection(from, to),
                MapAction::Select { from: None, to } => self.session.extend_to(to),
            }
            ui.ctx().request_repaint();
        }
    }
}

/// Distance between dots on the map.
const MAP_STEP: f32 = 12.0;

/// How far along its segment an offset is on the map.
fn map_x(text: &str, off: usize) -> f32 {
    let tokens = notation::parse(text).tokens;
    if off == text.len() {
        return (tokens.len() + 1) as f32 * MAP_STEP;
    }
    let done = tokens.iter().filter(|t| t.span.end <= off).count();
    let partway = tokens.iter().any(|t| t.span.start < off && off < t.span.end);
    (done as f32 + if partway { 0.5 } else { 0.0 }) * MAP_STEP
}

fn hover_text(text: &str, off: usize) -> String {
    if text.is_empty() {
        return "(empty)".to_owned();
    }
    format!("{}|{}", &text[..off], &text[off..])
}

/// Colors the text of a path. Fork marks are transparent, since they're painted
/// along with their siblings.
fn highlight(text: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let mut add = |s: &str, color| job.append(s, 0.0, TextFormat::simple(FONT, color));
    for (i, seg) in text.split(FORK).enumerate() {
        if i > 0 {
            add(FORK, Color32::TRANSPARENT);
        }
        let parsed = notation::parse(seg);
        let mut niss = vec![];
        for t in &parsed.tokens {
            if let TokenKind::Close { kind: GroupKind::Niss, open, .. } = t.kind {
                niss.push(open);
            }
        }
        let mut at = 0;
        for t in &parsed.tokens {
            add(&seg[at..t.span.start], DIM);
            let unclosed = parsed.open.iter().any(|g| g.span == t.span);
            let color = if unclosed {
                ERROR
            } else if niss.contains(&t.span.start) && !t.kind.is_error() {
                NISS
            } else {
                token_color(t.kind)
            };
            add(&seg[t.span.clone()], color);
            at = t.span.end;
        }
        add(&seg[at..], DIM);
    }
    job
}

/// Draws the cube and returns the face under the pointer.
fn draw_cube(ui: &Ui, rect: Rect, state: &Perm, view: &View, pointer: Option<Pos2>) -> Option<char> {
    let scale = rect.size().min_elem() / 5.6;
    let (sy, cy) = view.yaw.sin_cos();
    let (sp, cp) = view.pitch.sin_cos();
    let rotate = |v: [f32; 3]| {
        let (x, z) = (v[0] * cy - v[2] * sy, v[0] * sy + v[2] * cy);
        [x, v[1] * cp - z * sp, v[1] * sp + z * cp]
    };
    let project = |v: [f32; 3]| {
        let [x, y, z] = rotate(v);
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
        ui.painter().add(Shape::convex_polygon(outer.clone(), Color32::BLACK, Stroke::NONE));
        ui.painter().add(Shape::convex_polygon(quad(0.44), color, Stroke::NONE));
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
