//! Mode d'affichage et résolution.
//!
//! - Fenêtré : la résolution est la taille de la fenêtre.
//! - Plein écran (sans bordure, à la taille de l'écran) : la scène 3D est
//!   calculée à la résolution choisie puis agrandie à l'écran ; les menus et
//!   le HUD restent nets, à la résolution de l'écran.

use crate::settings::Settings;
use macroquad::miniquad::window::{set_window_position, set_window_size};
use macroquad::prelude::*;

/// Hauteurs proposées (format 16:9 en fenêtré, format de l'écran en plein écran).
const HEIGHTS: [u32; 8] = [720, 768, 900, 1080, 1200, 1440, 1800, 2160];

/// Taille de l'écran principal, en pixels.
#[cfg(windows)]
pub fn monitor() -> (u32, u32) {
    use winapi::um::winuser::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    if w > 0 && h > 0 {
        (w as u32, h as u32)
    } else {
        (1920, 1080)
    }
}

#[cfg(not(windows))]
pub fn monitor() -> (u32, u32) {
    (1920, 1080)
}

fn width_16_9(h: u32) -> u32 {
    match h {
        768 => 1366,
        _ => (h * 16 + 8) / 9,
    }
}

/// Tailles de fenêtre qui tiennent à l'écran (barre de titre et barre des tâches comprises).
pub fn window_sizes() -> Vec<(u32, u32)> {
    let (mw, mh) = monitor();
    let mut v: Vec<(u32, u32)> = HEIGHTS.iter().map(|h| (width_16_9(*h), *h)).filter(|(w, h)| *w <= mw && *h + 80 <= mh).collect();
    if v.is_empty() {
        v.push((mw.min(1280), (mh.saturating_sub(80)).clamp(360, 720)));
    }
    v
}

/// Résolutions de rendu en plein écran, au format de l'écran ; la dernière est la native.
pub fn render_sizes() -> Vec<(u32, u32)> {
    let (mw, mh) = monitor();
    let mut v: Vec<(u32, u32)> = HEIGHTS.iter().filter(|h| **h < mh).map(|h| ((h * mw + mh / 2) / mh, *h)).collect();
    v.push((mw, mh));
    v
}

/// Résolution effective pour les réglages donnés.
pub fn resolution(s: &Settings) -> (u32, u32) {
    if s.fullscreen {
        let sizes = render_sizes();
        let native = *sizes.last().unwrap();
        if s.render_height == 0 {
            native
        } else {
            sizes.into_iter().find(|(_, h)| *h == s.render_height).unwrap_or(native)
        }
    } else {
        let sizes = window_sizes();
        let want = (s.window_size[0], s.window_size[1]);
        if sizes.contains(&want) {
            want
        } else {
            // Taille enregistrée trop grande pour cet écran : la plus grande qui tient.
            sizes.into_iter().filter(|(w, h)| *w <= want.0 && *h <= want.1).last().unwrap_or((1280, 720))
        }
    }
}

/// Passe à la résolution suivante (`step` = 1) ou précédente (-1) du mode actuel.
pub fn cycle(s: &mut Settings, step: i32) {
    let cur = resolution(s);
    let sizes = if s.fullscreen { render_sizes() } else { window_sizes() };
    let i = sizes.iter().position(|r| *r == cur).unwrap_or(0) as i32;
    let n = sizes.len() as i32;
    let (w, h) = sizes[((i + step).rem_euclid(n)) as usize];
    if s.fullscreen {
        s.render_height = if (w, h) == monitor() { 0 } else { h };
    } else {
        s.window_size = [w, h];
    }
}

pub fn label(s: &Settings) -> String {
    let (w, h) = resolution(s);
    if s.fullscreen && (w, h) == monitor() {
        format!("{} × {} (native)", w, h)
    } else {
        format!("{} × {}", w, h)
    }
}

/// État appliqué à la fenêtre et cible de rendu de la scène 3D.
pub struct Display {
    applied: Option<(bool, [u32; 2])>,
    target: Option<((u32, u32), RenderTarget)>,
}

impl Display {
    pub fn new() -> Display {
        Display { applied: None, target: None }
    }

    /// Applique les réglages à la fenêtre s'ils ont changé.
    pub fn apply(&mut self, s: &Settings) {
        let (w, h) = resolution(s);
        let want = (s.fullscreen, if s.fullscreen { [0, 0] } else { [w, h] });
        if self.applied == Some(want) {
            return;
        }
        let first = self.applied.is_none();
        self.applied = Some(want);
        if s.fullscreen {
            set_fullscreen(true);
        } else {
            if !first {
                set_fullscreen(false);
            }
            // En pixels réels (request_new_screen_size multiplierait par la mise à l'échelle de Windows).
            set_window_size(w, h);
            let (mw, mh) = monitor();
            set_window_position(mw.saturating_sub(w) / 2, (mh.saturating_sub(h) / 2).saturating_sub(20));
        }
    }

    /// Cible où dessiner la scène 3D : `None` pour dessiner directement à l'écran.
    pub fn scene_target(&mut self, s: &Settings) -> Option<RenderTarget> {
        if !s.fullscreen {
            self.target = None;
            return None;
        }
        let size = resolution(s);
        // screen_height() est en unités logiques : on compare en pixels réels.
        let screen_h = (screen_height() * macroquad::miniquad::window::dpi_scale()).round() as u32;
        if size.1 >= screen_h {
            self.target = None;
            return None;
        }
        if self.target.as_ref().map_or(true, |(sz, _)| *sz != size) {
            let rt = render_target_ex(size.0, size.1, RenderTargetParams { sample_count: 4, depth: true });
            rt.texture.set_filter(FilterMode::Linear);
            self.target = Some((size, rt));
        }
        self.target.as_ref().map(|(_, rt)| rt.clone())
    }
}

/// Affiche à l'écran la scène calculée dans `rt`.
pub fn present(rt: &RenderTarget) {
    draw_texture_ex(
        &rt.texture,
        0.0,
        0.0,
        WHITE,
        DrawTextureParams { dest_size: Some(vec2(screen_width(), screen_height())), flip_y: true, ..Default::default() },
    );
}
