//! Petite interface en mode immédiat : texte, boutons, curseurs, cases, champs.
//! Les menus sont dessinés dans un espace virtuel de 1280x720 mis à l'échelle.

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
    pub backspace: bool,
    active: Option<u32>,
    pub focus: Option<u32>,
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
            backspace: false,
            active: None,
            focus: None,
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
        }
        self.chars.clear();
        while let Some(c) = get_char_pressed() {
            if !c.is_control() {
                self.chars.push(c);
            }
        }
        self.backspace = is_key_pressed(KeyCode::Backspace);
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

    pub fn textfield(&mut self, id: u32, r: Rect, text: &mut String, max: usize) -> bool {
        if self.click {
            if self.hover(r) {
                self.focus = Some(id);
            } else if self.focus == Some(id) {
                self.focus = None;
            }
        }
        let focused = self.focus == Some(id);
        let mut changed = false;
        if focused {
            for c in &self.chars {
                if text.chars().count() < max {
                    text.push(*c);
                    changed = true;
                }
            }
            if self.backspace && text.pop().is_some() {
                changed = true;
            }
        }
        self.rect(r, WIDGET);
        if focused {
            self.frame(r, ACCENT);
        }
        let caret = if focused && (get_time() * 2.0) as i64 % 2 == 0 { "|" } else { "" };
        self.text(&format!("{}{}", text, caret), r.x + 10.0, r.y + r.h / 2.0 + 7.0, 20.0, TEXT);
        changed
    }
}
