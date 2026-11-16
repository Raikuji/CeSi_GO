//! Menu principal, écran de lancement, options (jeu, touches, viseur) et pause.

use crate::hud::draw_crosshair;
use crate::keys::ALL_KEYS;
use crate::settings::{Bind, Settings, ACTIONS};
use crate::ui::{Ui, ACCENT, DIM, PANEL, TEXT};
use macroquad::prelude::*;

#[derive(PartialEq, Clone, Copy)]
pub enum Screen {
    Main,
    Play,
    Settings,
}

pub struct MenuState {
    pub screen: Screen,
    pub tab: usize,
    /// Action en attente d'une nouvelle touche.
    pub rebinding: Option<usize>,
    rebind_skip: bool,
    pub error: Option<String>,
}

pub enum MenuAction {
    None,
    PlayLocal,
    Join,
    Quit,
}

pub enum PauseAction {
    None,
    Resume,
    Settings,
    SwitchTeam,
    Leave,
}

impl MenuState {
    pub fn new() -> MenuState {
        MenuState { screen: Screen::Main, tab: 0, rebinding: None, rebind_skip: false, error: None }
    }
}

pub fn main_menu(ui: &mut Ui, st: &mut MenuState, s: &mut Settings) -> MenuAction {
    let mut action = MenuAction::None;
    match st.screen {
        Screen::Main => {
            ui.text("CeSi GO", 90.0, 190.0, 96.0, TEXT);
            ui.rect(Rect::new(94.0, 210.0, 120.0, 6.0), ACCENT);
            ui.text("Tir tactique en voxels — posez la bombe ou défendez les sites", 94.0, 252.0, 20.0, DIM);
            if ui.button(Rect::new(90.0, 320.0, 320.0, 56.0), "JOUER", true) {
                st.screen = Screen::Play;
                st.error = None;
            }
            if ui.button(Rect::new(90.0, 388.0, 320.0, 56.0), "OPTIONS", true) {
                st.screen = Screen::Settings;
            }
            if ui.button(Rect::new(90.0, 456.0, 320.0, 56.0), "QUITTER", true) {
                action = MenuAction::Quit;
            }
            if let Some(e) = &st.error {
                ui.text(e, 94.0, 560.0, 20.0, Color::new(1.0, 0.45, 0.35, 1.0));
            }
            ui.text("Serveur dédié : lancez cesi_go_server.exe", 94.0, 690.0, 15.0, DIM);
        }
        Screen::Play => {
            ui.text("JOUER", 90.0, 110.0, 44.0, TEXT);
            ui.text("Pseudo", 90.0, 168.0, 18.0, DIM);
            ui.textfield(1, Rect::new(180.0, 144.0, 280.0, 36.0), &mut s.name, 16);

            let left = Rect::new(90.0, 210.0, 520.0, 330.0);
            ui.rect(left, PANEL);
            ui.text("Partie locale contre des bots", 114.0, 252.0, 24.0, TEXT);
            ui.text("Joueurs par équipe", 114.0, 310.0, 18.0, DIM);
            ui.slider(2, Rect::new(114.0, 322.0, 380.0, 24.0), &mut s.bot_team_size, 1.0, 5.0);
            s.bot_team_size = s.bot_team_size.round();
            ui.text(&format!("{} contre {}", s.bot_team_size, s.bot_team_size), 510.0, 340.0, 18.0, TEXT);
            ui.text("Niveau des bots", 114.0, 392.0, 18.0, DIM);
            ui.slider(3, Rect::new(114.0, 404.0, 380.0, 24.0), &mut s.bot_skill, 0.0, 100.0);
            let level = match s.bot_skill as i32 {
                0..=24 => "Facile",
                25..=54 => "Normal",
                55..=79 => "Difficile",
                _ => "Expert",
            };
            ui.text(level, 510.0, 422.0, 18.0, TEXT);
            if ui.button(Rect::new(114.0, 464.0, 300.0, 52.0), "LANCER LA PARTIE", true) {
                action = MenuAction::PlayLocal;
            }

            let right = Rect::new(670.0, 210.0, 520.0, 330.0);
            ui.rect(right, PANEL);
            ui.text("Rejoindre un serveur", 694.0, 252.0, 24.0, TEXT);
            ui.text("Adresse du serveur (ip ou ip:port)", 694.0, 310.0, 18.0, DIM);
            ui.textfield(4, Rect::new(694.0, 324.0, 400.0, 38.0), &mut s.last_server, 60);
            ui.text("Le serveur se lance avec cesi_go_server.exe,", 694.0, 400.0, 16.0, DIM);
            ui.text("qui affiche l'adresse à saisir ici.", 694.0, 422.0, 16.0, DIM);
            if ui.button(Rect::new(694.0, 464.0, 300.0, 52.0), "CONNEXION", !s.last_server.trim().is_empty()) {
                action = MenuAction::Join;
            }

            if let Some(e) = &st.error {
                ui.text(e, 94.0, 586.0, 20.0, Color::new(1.0, 0.45, 0.35, 1.0));
            }
            if ui.button(Rect::new(90.0, 620.0, 200.0, 48.0), "RETOUR", true) {
                st.screen = Screen::Main;
                s.save();
            }
        }
        Screen::Settings => {
            if settings_panel(ui, st, s) {
                st.screen = Screen::Main;
            }
        }
    }
    action
}

/// Panneau d'options ; renvoie vrai quand le joueur clique sur « Retour ».
pub fn settings_panel(ui: &mut Ui, st: &mut MenuState, s: &mut Settings) -> bool {
    let panel = Rect::new(140.0, 40.0, 1000.0, 640.0);
    ui.rect(panel, PANEL);
    ui.text("OPTIONS", 170.0, 92.0, 34.0, TEXT);
    for (i, name) in ["JEU", "TOUCHES", "VISEUR"].iter().enumerate() {
        if ui.tab(Rect::new(170.0 + i as f32 * 190.0, 112.0, 180.0, 42.0), name, st.tab == i) {
            st.tab = i;
            st.rebinding = None;
        }
    }
    match st.tab {
        0 => tab_game(ui, s),
        1 => tab_keys(ui, st, s),
        _ => tab_crosshair(ui, s),
    }
    if ui.button(Rect::new(170.0, 616.0, 200.0, 46.0), "RETOUR", true) {
        st.rebinding = None;
        s.save();
        return true;
    }
    false
}

fn tab_game(ui: &mut Ui, s: &mut Settings) {
    let (lx, wx, vx) = (190.0, 520.0, 915.0);
    let mut y = 220.0;
    ui.text("Pseudo", lx, y, 20.0, TEXT);
    ui.textfield(10, Rect::new(wx, y - 26.0, 300.0, 38.0), &mut s.name, 16);
    y += 62.0;

    ui.text("Sensibilité de la souris", lx, y, 20.0, TEXT);
    ui.slider(11, Rect::new(wx, y - 18.0, 370.0, 24.0), &mut s.sensitivity, 0.1, 8.0);
    s.sensitivity = (s.sensitivity * 100.0).round() / 100.0;
    ui.text(&format!("{:.2}", s.sensitivity), vx, y, 20.0, ACCENT);
    if ui.button(Rect::new(990.0, y - 24.0, 36.0, 34.0), "–", true) {
        s.sensitivity = (s.sensitivity - 0.05).max(0.05);
    }
    if ui.button(Rect::new(1032.0, y - 24.0, 36.0, 34.0), "+", true) {
        s.sensitivity = (s.sensitivity + 0.05).min(20.0);
    }
    y += 62.0;

    ui.text("Sensibilité avec la lunette", lx, y, 20.0, TEXT);
    ui.slider(12, Rect::new(wx, y - 18.0, 370.0, 24.0), &mut s.zoom_sensitivity, 0.3, 2.0);
    ui.text(&format!("x {:.2}", s.zoom_sensitivity), vx, y, 20.0, ACCENT);
    y += 62.0;

    ui.text("Champ de vision", lx, y, 20.0, TEXT);
    ui.slider(13, Rect::new(wx, y - 18.0, 370.0, 24.0), &mut s.fov, 70.0, 110.0);
    s.fov = s.fov.round();
    ui.text(&format!("{:.0}°", s.fov), vx, y, 20.0, ACCENT);
    y += 62.0;

    ui.text("Volume", lx, y, 20.0, TEXT);
    ui.slider(14, Rect::new(wx, y - 18.0, 370.0, 24.0), &mut s.volume, 0.0, 1.0);
    ui.text(&format!("{:.0} %", s.volume * 100.0), vx, y, 20.0, ACCENT);
    y += 62.0;

    if ui.checkbox(Rect::new(lx, y - 26.0, 300.0, 36.0), "Plein écran", &mut s.fullscreen) {
        set_fullscreen(s.fullscreen);
    }
    ui.text("La sensibilité utilise la même échelle que les FPS classiques (0,022° par point).", lx, 580.0, 15.0, DIM);
}

fn tab_keys(ui: &mut Ui, st: &mut MenuState, s: &mut Settings) {
    // Capture de la nouvelle touche.
    if let Some(i) = st.rebinding {
        if st.rebind_skip {
            st.rebind_skip = false;
        } else {
            let keys = get_keys_pressed();
            let mut new: Option<Bind> = ALL_KEYS.iter().copied().find(|k| keys.contains(k)).map(Bind::Key);
            for b in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
                if is_mouse_button_pressed(b) {
                    new = Some(Bind::Mouse(b));
                }
            }
            if let Some(b) = new {
                s.set_bind(ACTIONS[i].0, b);
                st.rebinding = None;
                // Le clic qui valide ne doit pas activer un autre bouton.
                ui.click = false;
            }
        }
    }
    for (i, (action, _, label)) in ACTIONS.iter().enumerate() {
        let (col, row) = (i / 9, i % 9);
        let x = 190.0 + col as f32 * 470.0;
        let y = 180.0 + row as f32 * 46.0;
        ui.text(label, x, y + 25.0, 19.0, TEXT);
        let r = Rect::new(x + 235.0, y, 195.0, 38.0);
        if st.rebinding == Some(i) {
            ui.rect(r, ACCENT);
            ui.text_c("Appuyez…", r.x + r.w / 2.0, r.y + 26.0, 18.0, BLACK);
        } else if ui.button(r, &s.bind(*action).label(), true) && st.rebinding.is_none() {
            st.rebinding = Some(i);
            st.rebind_skip = true;
        }
    }
    ui.text("Molette : changer d'arme · Échap : menu / annuler", 660.0, 570.0, 15.0, DIM);
    if ui.button(Rect::new(820.0, 616.0, 290.0, 46.0), "TOUCHES PAR DÉFAUT", st.rebinding.is_none()) && st.rebinding.is_none() {
        s.reset_binds();
    }
}

fn tab_crosshair(ui: &mut Ui, s: &mut Settings) {
    let c = &mut s.crosshair;
    let (lx, wx, vx) = (190.0, 360.0, 680.0);
    let mut y = 196.0;
    let row = |ui: &mut Ui, id: u32, label: &str, v: &mut f32, min: f32, max: f32, step: f32, y: &mut f32| {
        ui.text(label, lx, *y + 6.0, 19.0, TEXT);
        ui.slider(id, Rect::new(wx, *y - 12.0, 300.0, 24.0), v, min, max);
        *v = (*v / step).round() * step;
        let text = if step < 0.1 { format!("{:.0} %", *v * 100.0) } else { format!("{}", *v) };
        ui.text(&text, vx, *y + 6.0, 19.0, ACCENT);
        *y += 44.0;
    };
    row(ui, 20, "Longueur", &mut c.size, 0.0, 30.0, 0.5, &mut y);
    row(ui, 21, "Écart", &mut c.gap, -5.0, 20.0, 0.5, &mut y);
    row(ui, 22, "Épaisseur", &mut c.thickness, 1.0, 8.0, 0.5, &mut y);
    row(ui, 23, "Opacité", &mut c.alpha, 0.1, 1.0, 0.05, &mut y);
    let mut rgb = [c.color[0] as f32, c.color[1] as f32, c.color[2] as f32];
    row(ui, 24, "Rouge", &mut rgb[0], 0.0, 255.0, 1.0, &mut y);
    row(ui, 25, "Vert", &mut rgb[1], 0.0, 255.0, 1.0, &mut y);
    row(ui, 26, "Bleu", &mut rgb[2], 0.0, 255.0, 1.0, &mut y);
    c.color = [rgb[0] as u8, rgb[1] as u8, rgb[2] as u8];

    ui.checkbox(Rect::new(lx, y - 8.0, 250.0, 34.0), "Point central", &mut c.dot);
    ui.checkbox(Rect::new(lx + 270.0, y - 8.0, 250.0, 34.0), "Contour noir", &mut c.outline);
    y += 40.0;
    ui.checkbox(Rect::new(lx, y - 8.0, 250.0, 34.0), "Style en T", &mut c.t_style);
    ui.checkbox(Rect::new(lx + 270.0, y - 8.0, 250.0, 34.0), "Dynamique (dispersion)", &mut c.dynamic);

    // Aperçu sur trois fonds pour juger du contraste.
    let p = Rect::new(790.0, 180.0, 310.0, 300.0);
    ui.rect(Rect::new(p.x, p.y, p.w, 100.0), Color::new(0.56, 0.74, 0.92, 1.0));
    ui.rect(Rect::new(p.x, p.y + 100.0, p.w, 100.0), Color::new(0.84, 0.75, 0.56, 1.0));
    ui.rect(Rect::new(p.x, p.y + 200.0, p.w, 100.0), Color::new(0.24, 0.23, 0.25, 1.0));
    ui.frame(p, DIM);
    ui.text("Aperçu", p.x, p.y - 10.0, 16.0, DIM);
    for i in 0..3 {
        let cx = (p.x + p.w / 2.0) * ui.s + ui.ox;
        let cy = (p.y + 50.0 + i as f32 * 100.0) * ui.s;
        draw_crosshair(cx, cy, c, 0.0);
    }

    // Couleurs prédéfinies.
    ui.text("Couleurs", p.x, 512.0, 16.0, DIM);
    let presets: [[u8; 3]; 6] = [[60, 255, 90], [0, 255, 255], [255, 235, 60], [255, 255, 255], [255, 70, 220], [255, 60, 50]];
    for (i, col) in presets.iter().enumerate() {
        let r = Rect::new(p.x + i as f32 * 52.0, 524.0, 44.0, 44.0);
        ui.rect(r, Color::from_rgba(col[0], col[1], col[2], 255));
        if c.color == *col {
            ui.frame(r, TEXT);
        }
        if ui.hover(r) && ui.click {
            c.color = *col;
        }
    }
    if ui.button(Rect::new(820.0, 616.0, 290.0, 46.0), "VISEUR PAR DÉFAUT", true) {
        *c = Default::default();
    }
}

pub fn pause_menu(ui: &mut Ui, switching_allowed: bool) -> PauseAction {
    ui.dim(0.55);
    let panel = Rect::new(460.0, 150.0, 360.0, 400.0);
    ui.rect(panel, PANEL);
    ui.text_c("PAUSE", 640.0, 210.0, 34.0, TEXT);
    let mut action = PauseAction::None;
    let items = [
        ("REPRENDRE", PauseAction::Resume, true),
        ("OPTIONS", PauseAction::Settings, true),
        ("CHANGER D'ÉQUIPE", PauseAction::SwitchTeam, switching_allowed),
        ("QUITTER LA PARTIE", PauseAction::Leave, true),
    ];
    for (i, (label, a, enabled)) in items.into_iter().enumerate() {
        if ui.button(Rect::new(490.0, 250.0 + i as f32 * 66.0, 300.0, 54.0), label, enabled) {
            action = a;
        }
    }
    action
}
