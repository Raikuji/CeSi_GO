//! Petite interface en mode immédiat : texte, boutons, curseurs, cases, champs.
//! Les menus sont dessinés dans un espace virtuel de 1280x720 mis à l'échelle.

use crate::textedit::{Cmd, Edit};
use macroquad::miniquad::window::{clipboard_get, clipboard_set, set_mouse_cursor};
use macroquad::miniquad::CursorIcon;
use macroquad::prelude::*;
use std::borrow::Cow;

pub const ACCENT: Color = Color::new(0.96, 0.60, 0.14, 1.0);
pub const TEXT: Color = Color::new(0.92, 0.92, 0.94, 1.0);
pub const DIM: Color = Color::new(0.62, 0.64, 0.70, 1.0);
pub const PANEL: Color = Color::new(0.09, 0.10, 0.13, 0.94);
pub const WIDGET: Color = Color::new(0.17, 0.19, 0.24, 1.0);
pub const WIDGET_HOT: Color = Color::new(0.26, 0.29, 0.36, 1.0);
pub const T_COLOR: Color = Color::new(0.93, 0.68, 0.26, 1.0);
pub const CT_COLOR: Color = Color::new(0.42, 0.66, 0.96, 1.0);

/// Taille du texte des champs de saisie.
const FIELD_TEXT: f32 = 20.0;
/// Touches d'édition répétées tant qu'elles sont maintenues.
const EDIT_KEYS: [KeyCode; 6] = [KeyCode::Backspace, KeyCode::Delete, KeyCode::Left, KeyCode::Right, KeyCode::Home, KeyCode::End];

pub struct Ui {
    pub font: Option<Font>,
    /// Échelle et décalage de l'espace virtuel.
    pub s: f32,
    pub ox: f32,
    /// Souris en coordonnées virtuelles.
    pub mouse: Vec2,
    pub click: bool,
    pub down: bool,
    pub chars: Vec<char>,
    /// Touches d'édition déclenchées cette image, répétitions comprises.
    keys: Vec<KeyCode>,
    /// Touche d'édition maintenue et heure de sa prochaine répétition.
    held: Option<(KeyCode, f64)>,
    /// Raccourci d'édition reçu cette image.
    cmd: Option<Cmd>,
    active: Option<u32>,
    pub focus: Option<u32>,
    /// Curseur et sélection du champ qui a la saisie (`edit_id`).
    edit: Edit,
    edit_id: Option<u32>,
    /// Premier caractère affiché quand le texte dépasse du champ.
    first: usize,
    /// Sélection à la souris en cours.
    drag: bool,
    last_click: f64,
    /// Heure de la dernière action, pour le clignotement du curseur.
    blink: f64,
    /// La souris survole un champ de texte : curseur en forme de barre.
    ibeam: bool,
    ibeam_on: bool,
}

fn strip_accents(t: &str) -> String {
    t.chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'É' | 'È' | 'Ê' => 'E',
            'à' | 'â' | 'ä' => 'a',
            'À' | 'Â' => 'A',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'Ç' => 'C',
            '«' | '»' => '"',
            '—' | '–' => '-',
            '…' => '.',
            '’' => '\'',
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect()
}

impl Ui {
    pub fn new() -> Ui {
        let font = [
            "C:/Windows/Fonts/seguisb.ttf",
            "C:/Windows/Fonts/segoeui.ttf",
            "C:/Windows/Fonts/arialbd.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        ]
        .iter()
        .find_map(|p| std::fs::read(p).ok().and_then(|b| load_ttf_font_from_bytes(&b).ok()));
        Ui {
            font,
            s: 1.0,
            ox: 0.0,
            mouse: Vec2::ZERO,
            click: false,
            down: false,
            chars: Vec::new(),
            keys: Vec::new(),
            held: None,
            cmd: None,
            active: None,
            focus: None,
            edit: Edit::default(),
            edit_id: None,
            first: 0,
            drag: false,
            last_click: 0.0,
            blink: 0.0,
            ibeam: false,
            ibeam_on: false,
        }
    }

    pub fn begin(&mut self) {
        self.s = screen_height() / 720.0;
        self.ox = (screen_width() - 1280.0 * self.s) / 2.0;
        let (mx, my) = mouse_position();
        self.mouse = vec2((mx - self.ox) / self.s, my / self.s);
        self.click = is_mouse_button_pressed(MouseButton::Left);
        self.down = is_mouse_button_down(MouseButton::Left);
        if !self.down {
            self.active = None;
            self.drag = false;
        }
        if self.ibeam != self.ibeam_on {
            set_mouse_cursor(if self.ibeam { CursorIcon::Text } else { CursorIcon::Default });
            self.ibeam_on = self.ibeam;
        }
        self.ibeam = false;

        self.chars.clear();
        self.cmd = None;
        while let Some(c) = get_char_pressed() {
            match c {
                // Sous Windows, Ctrl+lettre arrive comme caractère de contrôle : la lettre
                // est celle de la disposition du clavier (AZERTY compris), pas une position.
                '\x01' => self.cmd = Some(Cmd::SelectAll),
                '\x03' => self.cmd = Some(Cmd::Copy),
                '\x16' => self.cmd = Some(Cmd::Paste),
                '\x18' => self.cmd = Some(Cmd::Cut),
                c if !c.is_control() => self.chars.push(c),
                _ => {}
            }
        }
        let ctrl = is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl);
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        if cfg!(not(windows)) && (ctrl || is_key_down(KeyCode::LeftSuper)) {
            for (k, c) in [(KeyCode::A, Cmd::SelectAll), (KeyCode::C, Cmd::Copy), (KeyCode::V, Cmd::Paste), (KeyCode::X, Cmd::Cut)] {
                if is_key_pressed(k) {
                    self.cmd = Some(c);
                }
            }
        }
        // Anciens raccourcis : Ctrl+Inser, Maj+Inser, Maj+Suppr.
        if is_key_pressed(KeyCode::Insert) && (ctrl || shift) {
            self.cmd = Some(if ctrl { Cmd::Copy } else { Cmd::Paste });
        }
        if is_key_pressed(KeyCode::Delete) && shift {
            self.cmd = Some(Cmd::Cut);
        }

        // macroquad ne signale pas la répétition des touches maintenues : on la refait ici.
        self.keys.clear();
        let now = get_time();
        for k in EDIT_KEYS {
            if is_key_pressed(k) {
                self.keys.push(k);
                self.held = Some((k, now + 0.4));
            }
        }
        match self.held {
            Some((k, _)) if !is_key_down(k) => self.held = None,
            Some((k, t)) if now >= t => {
                self.keys.push(k);
                self.held = Some((k, now + 0.035));
            }
            _ => {}
        }
    }

    fn fix<'a>(&self, t: &'a str) -> Cow<'a, str> {
        if self.font.is_some() || t.is_ascii() {
            Cow::Borrowed(t)
        } else {
            Cow::Owned(strip_accents(t))
        }
    }

    // ---- Coordonnées en pixels (HUD) ----

    pub fn width_px(&self, t: &str, size: f32) -> f32 {
        measure_text(&self.fix(t), self.font.as_ref(), size.max(1.0) as u16, 1.0).width
    }

    pub fn text_px(&self, t: &str, x: f32, y: f32, size: f32, color: Color) {
        draw_text_ex(
            &self.fix(t),
            x.round(),
            y.round(),
            TextParams { font: self.font.as_ref(), font_size: size.max(1.0) as u16, color, ..Default::default() },
        );
    }

    /// Texte avec une ombre, lisible sur le décor.
    pub fn shadow_px(&self, t: &str, x: f32, y: f32, size: f32, color: Color) {
        let o = (size / 16.0).max(1.0);
        self.text_px(t, x + o, y + o, size, Color::new(0.0, 0.0, 0.0, 0.75 * color.a));
        self.text_px(t, x, y, size, color);
    }

    pub fn shadow_px_c(&self, t: &str, cx: f32, y: f32, size: f32, color: Color) {
        self.shadow_px(t, cx - self.width_px(t, size) / 2.0, y, size, color);
    }

    pub fn shadow_px_r(&self, t: &str, right: f32, y: f32, size: f32, color: Color) {
        self.shadow_px(t, right - self.width_px(t, size), y, size, color);
    }

    // ---- Coordonnées virtuelles (menus) ----

    pub fn text(&self, t: &str, x: f32, y: f32, size: f32, color: Color) {
        self.text_px(t, x * self.s + self.ox, y * self.s, size * self.s, color);
    }

    pub fn width(&self, t: &str, size: f32) -> f32 {
        self.width_px(t, size * self.s) / self.s
    }

    pub fn text_c(&self, t: &str, cx: f32, y: f32, size: f32, color: Color) {
        self.text(t, cx - self.width(t, size) / 2.0, y, size, color);
    }

    pub fn text_r(&self, t: &str, right: f32, y: f32, size: f32, color: Color) {
        self.text(t, right - self.width(t, size), y, size, color);
    }

    pub fn rect(&self, r: Rect, color: Color) {
        draw_rectangle(r.x * self.s + self.ox, r.y * self.s, r.w * self.s, r.h * self.s, color);
    }

    pub fn frame(&self, r: Rect, color: Color) {
        draw_rectangle_lines(r.x * self.s + self.ox, r.y * self.s, r.w * self.s, r.h * self.s, 2.0 * self.s.max(1.0), color);
    }

    /// Assombrit tout l'écran derrière un menu.
    pub fn dim(&self, a: f32) {
        draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.02, 0.03, 0.05, a));
    }

    pub fn hover(&self, r: Rect) -> bool {
        r.contains(self.mouse)
    }

    pub fn button(&mut self, r: Rect, label: &str, enabled: bool) -> bool {
        let hot = enabled && self.hover(r);
        self.rect(r, if hot { WIDGET_HOT } else { WIDGET });
        if hot {
            self.rect(Rect::new(r.x, r.y, 4.0, r.h), ACCENT);
        }
        let size = (r.h * 0.44).clamp(14.0, 26.0);
        self.text_c(label, r.x + r.w / 2.0, r.y + r.h / 2.0 + size * 0.35, size, if enabled { TEXT } else { DIM });
        hot && self.click
    }

    /// Onglet : bouton avec un état sélectionné.
    pub fn tab(&mut self, r: Rect, label: &str, selected: bool) -> bool {
        let hot = self.hover(r);
        self.rect(r, if selected { WIDGET_HOT } else { WIDGET });
        if selected {
            self.rect(Rect::new(r.x, r.y + r.h - 4.0, r.w, 4.0), ACCENT);
        }
        self.text_c(label, r.x + r.w / 2.0, r.y + r.h / 2.0 + 7.0, 20.0, if selected || hot { TEXT } else { DIM });
        hot && self.click
    }

    pub fn slider(&mut self, id: u32, r: Rect, v: &mut f32, min: f32, max: f32) -> bool {
        if self.click && self.hover(Rect::new(r.x - 8.0, r.y - 6.0, r.w + 16.0, r.h + 12.0)) {
            self.active = Some(id);
        }
        let mut changed = false;
        if self.active == Some(id) {
            let k = ((self.mouse.x - r.x) / r.w).clamp(0.0, 1.0);
            let nv = min + (max - min) * k;
            changed = nv != *v;
            *v = nv;
        }
        let k = ((*v - min) / (max - min)).clamp(0.0, 1.0);
        let mid = r.y + r.h / 2.0;
        self.rect(Rect::new(r.x, mid - 3.0, r.w, 6.0), WIDGET);
        self.rect(Rect::new(r.x, mid - 3.0, r.w * k, 6.0), ACCENT);
        self.rect(Rect::new(r.x + r.w * k - 6.0, mid - 11.0, 12.0, 22.0), TEXT);
        changed
    }

    pub fn checkbox(&mut self, r: Rect, label: &str, v: &mut bool) -> bool {
        let b = Rect::new(r.x, r.y + (r.h - 22.0) / 2.0, 22.0, 22.0);
        let hot = self.hover(r);
        self.rect(b, if hot { WIDGET_HOT } else { WIDGET });
        self.frame(b, DIM);
        if *v {
            self.rect(Rect::new(b.x + 5.0, b.y + 5.0, 12.0, 12.0), ACCENT);
        }
        self.text(label, r.x + 34.0, r.y + r.h / 2.0 + 7.0, 20.0, TEXT);
        if hot && self.click {
            *v = !*v;
            return true;
        }
        false
    }

    /// Largeur des caractères `a..b` d'un champ de texte.
    fn span(&self, chars: &[char], a: usize, b: usize) -> f32 {
        if a >= b {
            return 0.0;
        }
        self.width(&chars[a..b].iter().collect::<String>(), FIELD_TEXT)
    }

    /// Dernier caractère (exclu) qui tient dans `room` en affichant à partir de `first`.
    fn fit(&self, chars: &[char], first: usize, room: f32) -> usize {
        let mut last = first;
        while last < chars.len() && self.span(chars, first, last + 1) <= room {
            last += 1;
        }
        last
    }

    /// Position du curseur la plus proche de la souris, parmi les caractères affichés.
    fn index_at(&self, chars: &[char], x0: f32, room: f32) -> usize {
        let last = self.fit(chars, self.first, room);
        let x = self.mouse.x - x0;
        (self.first..=last)
            .min_by(|a, b| {
                let (da, db) = ((self.span(chars, self.first, *a) - x).abs(), (self.span(chars, self.first, *b) - x).abs());
                da.total_cmp(&db)
            })
            .unwrap_or(self.first)
    }

    /// Diagnostic : impose la sélection du champ `id` et lui applique un raccourci.
    pub fn inject(&mut self, id: u32, sel: (usize, usize), cmd: Option<Cmd>) {
        self.focus = Some(id);
        self.edit_id = Some(id);
        self.edit = Edit { anchor: sel.0, caret: sel.1 };
        self.cmd = cmd;
        self.blink = get_time();
    }

    /// Champ de saisie d'une ligne : curseur, sélection à la souris ou avec Maj,
    /// copier / couper / coller. Renvoie vrai si le texte a changé.
    pub fn textfield(&mut self, id: u32, r: Rect, text: &mut String, max: usize) -> bool {
        let (x0, room) = (r.x + 10.0, r.w - 20.0);
        let base = r.y + r.h / 2.0 + 7.0;
        let inside = self.hover(r);
        if inside {
            self.ibeam = true;
        }
        if self.click {
            if inside {
                self.focus = Some(id);
            } else if self.focus == Some(id) {
                self.focus = None;
            }
        }
        if self.focus != Some(id) {
            if self.edit_id == Some(id) {
                self.edit_id = None;
            }
            let chars: Vec<char> = text.chars().collect();
            let shown: String = chars[..self.fit(&chars, 0, room)].iter().collect();
            self.rect(r, WIDGET);
            self.text(&shown, x0, base, FIELD_TEXT, TEXT);
            return false;
        }

        let now = get_time();
        if self.edit_id != Some(id) {
            // Le champ vient de recevoir la saisie : curseur à la fin.
            self.edit_id = Some(id);
            self.edit = Edit::at_end(text);
            self.first = 0;
            self.last_click = 0.0;
            self.blink = now;
        }
        self.edit.clamp(text);
        let before = self.edit;
        let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

        // Souris : clic pour placer le curseur, glisser pour sélectionner, double clic pour tout prendre.
        let chars: Vec<char> = text.chars().collect();
        self.first = self.first.min(chars.len());
        if self.click && inside {
            if now - self.last_click < 0.35 && !shift {
                self.edit.select_all(text);
                self.drag = false;
                self.last_click = 0.0;
            } else {
                self.edit.move_to(self.index_at(&chars, x0, room), shift);
                self.drag = true;
                self.last_click = now;
            }
        } else if self.drag && self.down {
            // Au-delà des bords, la sélection fait défiler le texte.
            let i = if self.mouse.x < x0 {
                self.first.saturating_sub(1)
            } else if self.mouse.x > x0 + room {
                (self.fit(&chars, self.first, room) + 1).min(chars.len())
            } else {
                self.index_at(&chars, x0, room)
            };
            self.edit.move_to(i, true);
        }

        // Clavier.
        let mut changed = false;
        if !self.chars.is_empty() {
            let typed: String = self.chars.iter().collect();
            changed |= self.edit.insert(text, &typed, max);
        }
        for k in 0..self.keys.len() {
            match self.keys[k] {
                KeyCode::Backspace => changed |= self.edit.erase(text, false),
                KeyCode::Delete if !shift => changed |= self.edit.erase(text, true),
                KeyCode::Left => self.edit.step(text, -1, shift),
                KeyCode::Right => self.edit.step(text, 1, shift),
                KeyCode::Home => self.edit.move_to(0, shift),
                KeyCode::End => self.edit.move_to(text.chars().count(), shift),
                _ => {}
            }
        }
        match self.cmd.take() {
            Some(Cmd::SelectAll) => self.edit.select_all(text),
            Some(Cmd::Copy) if self.edit.has_selection() => clipboard_set(self.edit.selected(text)),
            Some(Cmd::Cut) if self.edit.has_selection() => {
                clipboard_set(self.edit.selected(text));
                changed |= self.edit.delete_selection(text);
            }
            Some(Cmd::Paste) => {
                if let Some(clip) = clipboard_get() {
                    changed |= self.edit.insert(text, clip.trim(), max);
                }
            }
            _ => {}
        }
        if changed || self.edit != before {
            self.blink = now;
        }

        // Le texte défile pour garder le curseur visible.
        let chars: Vec<char> = text.chars().collect();
        let caret = self.edit.caret;
        self.first = self.first.min(caret);
        while self.span(&chars, self.first, caret) > room {
            self.first += 1;
        }
        while self.first > 0 && self.span(&chars, self.first - 1, chars.len()) <= room {
            self.first -= 1;
        }
        let (first, last) = (self.first, self.fit(&chars, self.first, room));

        self.rect(r, WIDGET);
        self.frame(r, ACCENT);
        let (a, b) = self.edit.range();
        let (a, b) = (a.clamp(first, last), b.clamp(first, last));
        if a < b {
            let sel = Rect::new(x0 + self.span(&chars, first, a), r.y + 6.0, self.span(&chars, a, b), r.h - 12.0);
            self.rect(sel, Color::new(ACCENT.r, ACCENT.g, ACCENT.b, 0.5));
        }
        let shown: String = chars[first..last].iter().collect();
        self.text(&shown, x0, base, FIELD_TEXT, TEXT);
        if ((now - self.blink) * 2.0) as i64 % 2 == 0 {
            self.rect(Rect::new(x0 + self.span(&chars, first, caret) - 1.0, r.y + 7.0, 2.0, r.h - 14.0), TEXT);
        }
        changed
    }
}
