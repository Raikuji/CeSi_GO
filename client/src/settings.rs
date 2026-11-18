//! Réglages du joueur : touches, souris, viseur. Enregistrés en JSON.

use crate::keys::{key_from_id, key_id, key_label};
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Touches de sélection directe, dans l'ordre des types de grenade.
pub const NADE_ACTIONS: [Action; 4] = [Action::NadeFrag, Action::NadeSmoke, Action::NadeFlash, Action::NadeFire];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Crouch,
    Walk,
    Fire,
    Aim,
    Reload,
    Use,
    Drop,
    DropBomb,
    Buy,
    Scoreboard,
    Primary,
    Secondary,
    Knife,
    Grenade,
    NadeFrag,
    NadeSmoke,
    NadeFlash,
    NadeFire,
    Chat,
    Ping,
}

/// (action, identifiant dans le fichier, libellé du menu)
pub const ACTIONS: [(Action, &str, &str); 25] = [
    (Action::Forward, "forward", "Avancer"),
    (Action::Back, "back", "Reculer"),
    (Action::Left, "left", "Gauche"),
    (Action::Right, "right", "Droite"),
    (Action::Jump, "jump", "Sauter"),
    (Action::Crouch, "crouch", "S'accroupir"),
    (Action::Walk, "walk", "Marcher (silencieux)"),
    (Action::Fire, "fire", "Tirer"),
    (Action::Aim, "aim", "Lunette / coup puissant"),
    (Action::Reload, "reload", "Recharger"),
    (Action::Use, "use", "Poser / désamorcer / échanger"),
    (Action::Drop, "drop", "Jeter l'arme"),
    (Action::DropBomb, "drop_bomb", "Lâcher la bombe"),
    (Action::Buy, "buy", "Menu d'achat"),
    (Action::Scoreboard, "scoreboard", "Tableau des scores"),
    (Action::Primary, "primary", "Arme principale"),
    (Action::Secondary, "secondary", "Pistolet"),
    (Action::Knife, "knife", "Couteau"),
    (Action::Grenade, "grenade", "Grenades (type suivant)"),
    (Action::NadeFrag, "nade_frag", "Grenade à fragmentation"),
    (Action::NadeSmoke, "nade_smoke", "Fumigène"),
    (Action::NadeFlash, "nade_flash", "Grenade aveuglante"),
    (Action::NadeFire, "nade_fire", "Grenade incendiaire"),
    (Action::Chat, "chat", "Discussion"),
    (Action::Ping, "ping", "Signaler un point"),
];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Bind {
    Key(KeyCode),
    Mouse(MouseButton),
    None,
}

impl Bind {
    pub fn down(self) -> bool {
        match self {
            Bind::Key(k) => is_key_down(k),
            Bind::Mouse(b) => is_mouse_button_down(b),
            Bind::None => false,
        }
    }

    pub fn pressed(self) -> bool {
        match self {
            Bind::Key(k) => is_key_pressed(k),
            Bind::Mouse(b) => is_mouse_button_pressed(b),
            Bind::None => false,
        }
    }

    fn id(self) -> String {
        match self {
            Bind::Key(k) => key_id(k),
            Bind::Mouse(MouseButton::Left) => "Mouse1".into(),
            Bind::Mouse(MouseButton::Right) => "Mouse2".into(),
            Bind::Mouse(_) => "Mouse3".into(),
            Bind::None => "None".into(),
        }
    }

    fn parse(s: &str) -> Bind {
        match s {
            "Mouse1" => Bind::Mouse(MouseButton::Left),
            "Mouse2" => Bind::Mouse(MouseButton::Right),
            "Mouse3" => Bind::Mouse(MouseButton::Middle),
            _ => key_from_id(s).map_or(Bind::None, Bind::Key),
        }
    }

    pub fn label(self) -> String {
        match self {
            Bind::Key(k) => key_label(k),
            Bind::Mouse(MouseButton::Left) => "Clic gauche".into(),
            Bind::Mouse(MouseButton::Right) => "Clic droit".into(),
            Bind::Mouse(_) => "Clic molette".into(),
            Bind::None => "—".into(),
        }
    }
}

fn default_bind(a: Action) -> Bind {
    // Positions physiques : W/A/S/D correspond à Z/Q/S/D sur un clavier AZERTY.
    match a {
        Action::Forward => Bind::Key(KeyCode::W),
        Action::Back => Bind::Key(KeyCode::S),
        Action::Left => Bind::Key(KeyCode::A),
        Action::Right => Bind::Key(KeyCode::D),
        Action::Jump => Bind::Key(KeyCode::Space),
        Action::Crouch => Bind::Key(KeyCode::LeftControl),
        Action::Walk => Bind::Key(KeyCode::LeftShift),
        Action::Fire => Bind::Mouse(MouseButton::Left),
        Action::Aim => Bind::Mouse(MouseButton::Right),
        Action::Reload => Bind::Key(KeyCode::R),
        Action::Use => Bind::Key(KeyCode::E),
        Action::Drop => Bind::Key(KeyCode::G),
        Action::DropBomb => Bind::Key(KeyCode::H),
        Action::Buy => Bind::Key(KeyCode::B),
        Action::Scoreboard => Bind::Key(KeyCode::Tab),
        Action::Primary => Bind::Key(KeyCode::Key1),
        Action::Secondary => Bind::Key(KeyCode::Key2),
        Action::Knife => Bind::Key(KeyCode::Key3),
        Action::Grenade => Bind::Key(KeyCode::Key4),
        Action::NadeFrag => Bind::Key(KeyCode::Key5),
        Action::NadeSmoke => Bind::Key(KeyCode::Key6),
        Action::NadeFlash => Bind::Key(KeyCode::Key7),
        Action::NadeFire => Bind::Key(KeyCode::Key8),
        Action::Chat => Bind::Key(KeyCode::Y),
        Action::Ping => Bind::Mouse(MouseButton::Middle),
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Crosshair {
    pub size: f32,
    pub gap: f32,
    pub thickness: f32,
    pub alpha: f32,
    pub color: [u8; 3],
    pub dot: bool,
    pub outline: bool,
    pub t_style: bool,
    /// Le viseur s'écarte avec l'imprécision du tir.
    pub dynamic: bool,
}

impl Default for Crosshair {
    fn default() -> Self {
        Crosshair {
            size: 8.0,
            gap: 4.0,
            thickness: 2.0,
            alpha: 1.0,
            color: [60, 255, 90],
            dot: false,
            outline: true,
            t_style: false,
            dynamic: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub name: String,
    /// Sensibilité souris (même échelle que les FPS classiques : 0,022° par point).
    pub sensitivity: f32,
    /// Multiplicateur appliqué quand la lunette est utilisée.
    pub zoom_sensitivity: f32,
    pub fov: f32,
    pub volume: f32,
    pub fullscreen: bool,
    /// Taille de la fenêtre en mode fenêtré.
    pub window_size: [u32; 2],
    /// Hauteur de rendu de la scène 3D en plein écran (0 = celle de l'écran).
    pub render_height: u32,
    pub last_server: String,
    pub bot_skill: f32,
    pub bot_team_size: f32,
    /// Carte des parties locales (indice dans `map::MAPS`).
    pub map: u8,
    binds: BTreeMap<String, String>,
    pub crosshair: Crosshair,
    #[serde(skip)]
    cache: Vec<Bind>,
}

impl Default for Settings {
    fn default() -> Self {
        let mut s = Settings {
            name: "Joueur".into(),
            sensitivity: 2.0,
            zoom_sensitivity: 1.0,
            fov: 90.0,
            volume: 0.7,
            fullscreen: false,
            window_size: [1280, 720],
            render_height: 0,
            last_server: "127.0.0.1:27500".into(),
            bot_skill: 50.0,
            bot_team_size: 5.0,
            map: 0,
            binds: BTreeMap::new(),
            crosshair: Crosshair::default(),
            cache: Vec::new(),
        };
        s.rebuild();
        s
    }
}

fn path() -> PathBuf {
    let dir = std::env::var_os("APPDATA")
        .map(|d| PathBuf::from(d).join("CeSi_GO"))
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir.join("settings.json")
}

impl Settings {
    pub fn load() -> Settings {
        let mut s: Settings = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        s.sensitivity = s.sensitivity.clamp(0.05, 20.0);
        s.fov = s.fov.clamp(60.0, 120.0);
        s.map = s.map.min(cesi_common::map::MAPS.len() as u8 - 1);
        s.rebuild();
        s
    }

    pub fn save(&self) {
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path(), t);
        }
    }

    fn rebuild(&mut self) {
        self.cache = ACTIONS
            .iter()
            .map(|(a, id, _)| self.binds.get(*id).map_or_else(|| default_bind(*a), |s| Bind::parse(s)))
            .collect();
    }

    pub fn bind(&self, a: Action) -> Bind {
        self.cache[a as usize]
    }

    pub fn down(&self, a: Action) -> bool {
        self.bind(a).down()
    }

    pub fn pressed(&self, a: Action) -> bool {
        self.bind(a).pressed()
    }

    /// Associe une touche à une action ; l'action qui l'utilisait récupère l'ancienne.
    pub fn set_bind(&mut self, a: Action, b: Bind) {
        let old = self.bind(a);
        if let Some((other, id, _)) = ACTIONS.iter().find(|(o, _, _)| *o != a && self.cache[*o as usize] == b) {
            self.binds.insert(id.to_string(), old.id());
            self.cache[*other as usize] = old;
        }
        self.binds.insert(ACTIONS[a as usize].1.to_string(), b.id());
        self.cache[a as usize] = b;
    }

    pub fn reset_binds(&mut self) {
        self.binds.clear();
        self.rebuild();
    }
}
