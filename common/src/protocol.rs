//! Messages échangés entre le client et le serveur.

use glam::Vec3;
use serde::{Deserialize, Serialize};

pub const PROTOCOL: u16 = 12;
pub const DEFAULT_PORT: u16 = 27500;
pub const TICK_RATE: u32 = 60;
/// Dégâts à infliger à un adversaire, sans l'achever, pour compter une assistance.
pub const ASSIST_DAMAGE: i32 = 40;

pub type PlayerId = u8;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Team {
    /// Attaquants : posent la bombe.
    T,
    /// Défenseurs : protègent les sites.
    CT,
}

impl Team {
    pub fn other(self) -> Team {
        match self {
            Team::T => Team::CT,
            Team::CT => Team::T,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Team::T => "Attaquants",
            Team::CT => "Défenseurs",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    /// Aucun joueur : le serveur attend.
    Warmup,
    /// Début de round : déplacements bloqués, achats ouverts.
    Freeze,
    Live,
    RoundEnd,
    MatchEnd,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum BombState {
    None,
    Carried(PlayerId),
    Dropped(Vec3),
    Planted { pos: Vec3, time_left: f32, site: u8 },
    Defused(Vec3),
    Exploded(Vec3),
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum EndReason {
    Elimination,
    Time,
    Exploded,
    Defused,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuyItem {
    Weapon(u8),
    Kevlar,
    KevlarHelmet,
    Kit,
    /// Type de grenade (voir `grenades`).
    Grenade(u8),
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct MatchCfg {
    pub server_name: String,
    /// Carte jouée (indice dans `map::MAPS`).
    pub map: u8,
    pub freeze_time: f32,
    pub buy_time: f32,
    pub round_time: f32,
    pub bomb_time: f32,
    pub max_rounds: u16,
}

/// Messages fiables client -> serveur.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientMsg {
    Join { version: u16, name: String },
    Leave,
    /// Tir : le serveur calcule l'impact, en remontant le temps à `render_time`.
    /// `heavy` : coup de couteau puissant (clic droit).
    Fire { weapon: u8, origin: Vec3, dir: Vec3, render_time: f64, heavy: bool },
    Buy(BuyItem),
    /// Revend un objet acheté pendant ce round (achats encore ouverts).
    Sell(BuyItem),
    /// Lancer de grenade ; `strong` : lancer long (sinon lancer court).
    Throw { kind: u8, origin: Vec3, dir: Vec3, strong: bool },
    /// Prend l'arme au sol `id` ; `mag` et `reserve` sont les munitions de l'arme
    /// laissée en échange.
    Pickup { id: u16, mag: u16, reserve: u16 },
    /// Jette l'arme de l'emplacement `slot` (0 principale, 1 pistolet) avec ses munitions.
    Drop { slot: u8, mag: u16, reserve: u16 },
    /// Le porteur lâche la bombe devant lui (pour la donner à un coéquipier).
    DropBomb,
    /// Signale un point à son équipe (là où l'on vise).
    Ping { pos: Vec3 },
    SwitchTeam,
    Chat(String),
}

/// État du joueur, envoyé en continu (non fiable).
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ClientState {
    pub n: u32,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub crouch: bool,
    pub weapon: u8,
    /// Touche « utiliser » maintenue (poser / désamorcer).
    pub using: bool,
    /// Pour l'animation vue par les autres joueurs.
    pub reloading: bool,
    pub air: bool,
    /// Heure de la dernière photo reçue du serveur, et temps écoulé depuis sa
    /// réception : le serveur en déduit la latence aller-retour.
    pub echo: f64,
    pub echo_delay: f32,
    /// Joueur mort ou hors jeu : seul l'écho compte, le reste est ignoré.
    pub idle: bool,
}

/// Messages fiables serveur -> client.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerMsg {
    Welcome { id: PlayerId, cfg: MatchCfg },
    Reject(String),
    /// Téléportation au point d'apparition et réapprovisionnement.
    Spawn { pos: Vec3, yaw: f32 },
    /// Tir d'un autre joueur (traçante et son).
    Shot { shooter: PlayerId, weapon: u8, origin: Vec3, end: Vec3, hit_player: bool },
    /// Confirmation de touche pour le tireur ; `damage` : vie réellement retirée.
    Hit { victim: PlayerId, damage: u16, headshot: bool, killed: bool, pos: Vec3 },
    /// Dégâts reçus.
    Hurt { from: Vec3, damage: u16 },
    /// `assist` : celui qui, après le tueur, a infligé le plus de dégâts (au moins `ASSIST_DAMAGE`).
    Kill { killer: Option<PlayerId>, assist: Option<PlayerId>, victim: PlayerId, weapon: u8, headshot: bool },
    RoundStart { round: u16 },
    RoundEnd { winner: Team, reason: EndReason },
    BombPlanted { site: u8 },
    BombDefused,
    BombExploded { pos: Vec3 },
    /// Une grenade vient d'exploser ou de se déclencher.
    Detonate { kind: u8, pos: Vec3 },
    /// Le joueur est aveuglé pendant `duration` secondes.
    Flashed { duration: f32 },
    MatchEnd { winner: Option<Team> },
    /// Le serveur change de carte : le client la reconstruit.
    MapChange { map: u8 },
    /// Arme ramassée au sol, avec les munitions qu'elle contenait.
    /// `auto` : ramassée en passant dessus.
    Picked { weapon: u8, mag: u16, reserve: u16, auto: bool },
    /// Point signalé par un coéquipier (ou par soi-même).
    Ping { from: PlayerId, pos: Vec3 },
    /// Objets revendables maintenant et montant rendu, envoyé quand la liste change.
    Refunds(Vec<(BuyItem, u32)>),
    Chat { from: String, team: Option<Team>, text: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlayerSnap {
    pub id: PlayerId,
    pub name: String,
    pub bot: bool,
    pub team: Team,
    pub alive: bool,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub crouch: bool,
    /// Arme en main (0 = couteau).
    pub weapon: u8,
    pub primary: u8,
    pub secondary: u8,
    pub health: u8,
    pub armor: u8,
    pub helmet: bool,
    pub kit: bool,
    pub money: u32,
    pub kills: u16,
    pub assists: u16,
    /// Latence aller-retour vers le serveur, en millisecondes (0 pour un bot).
    pub ping: u16,
    pub deaths: u16,
    /// 0 rien, 1 pose la bombe, 2 désamorce.
    pub action: u8,
    pub progress: f32,
    pub reloading: bool,
    /// En l'air (saut ou chute).
    pub air: bool,
    /// Grenades possédées, par type.
    pub nades: [u8; 4],
}

/// Grenade en vol.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct NadeSnap {
    pub id: u16,
    pub kind: u8,
    pub pos: Vec3,
}

/// Zone d'effet au sol : fumée ou flammes.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct AreaSnap {
    pub id: u16,
    pub kind: u8,
    pub pos: Vec3,
    pub time_left: f32,
}

/// Photo de l'état du jeu, envoyée 30 fois par seconde (non fiable).
/// Arme posée au sol.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct DropSnap {
    pub id: u16,
    pub weapon: u8,
    pub pos: Vec3,
    pub yaw: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    pub time: f64,
    pub phase: Phase,
    pub time_left: f32,
    pub round: u16,
    pub score_t: u16,
    pub score_ct: u16,
    pub bomb: BombState,
    pub players: Vec<PlayerSnap>,
    pub nades: Vec<NadeSnap>,
    pub areas: Vec<AreaSnap>,
    pub drops: Vec<DropSnap>,
}
