//! Grenades : fragmentation, fumigène, aveuglante et incendiaire.

use crate::weapons::weapon;
use glam::Vec3;

pub const FRAG: u8 = 0;
pub const SMOKE: u8 = 1;
pub const FLASH: u8 = 2;
pub const FIRE: u8 = 3;

/// Identifiant d'« arme » d'une grenade : `GRENADE_BASE + type` (arme en main, éliminations).
pub const GRENADE_BASE: u8 = 100;
/// Nombre total de grenades transportables.
pub const MAX_GRENADES: u8 = 4;

pub struct GrenadeDef {
    pub name: &'static str,
    pub short: &'static str,
    pub price: u32,
    /// Nombre maximal de grenades de ce type.
    pub max: u8,
    pub info: &'static str,
}

pub const GRENADES: [GrenadeDef; 4] = [
    GrenadeDef {
        name: "Grenade à fragmentation",
        short: "Frag",
        price: 300,
        max: 1,
        info: "Grenade à fragmentation — explose après 1,8 s, jusqu'à 105 dégâts dans un rayon de 7,5 m",
    },
    GrenadeDef {
        name: "Fumigène",
        short: "Fumigène",
        price: 300,
        max: 1,
        info: "Fumigène — écran de fumée opaque pendant 15 s, éteint les flammes",
    },
    GrenadeDef {
        name: "Grenade aveuglante",
        short: "Flash",
        price: 200,
        max: 2,
        info: "Grenade aveuglante — aveugle quiconque la regarde exploser, alliés compris",
    },
    GrenadeDef {
        name: "Grenade incendiaire",
        short: "Incendiaire",
        price: 500,
        max: 1,
        info: "Grenade incendiaire — s'enflamme au sol pendant 7 s et brûle ceux qui traversent",
    },
];

/// Vitesse d'un lancer normal (clic gauche) et d'un lancer court (clic droit), en m/s.
pub const THROW_SPEED: f32 = 17.0;
pub const LOB_SPEED: f32 = 8.5;
pub const NADE_GRAVITY: f32 = 14.0;
/// Durée de l'armement avant que la grenade quitte la main (s).
pub const THROW_WINDUP: f32 = 0.28;

pub const FRAG_FUSE: f32 = 1.8;
pub const FRAG_RADIUS: f32 = 7.5;
pub const FRAG_DAMAGE: f32 = 105.0;
pub const FLASH_FUSE: f32 = 1.6;
/// Durée maximale d'aveuglement (s) et portée (m).
pub const FLASH_TIME: f32 = 4.5;
pub const FLASH_RANGE: f32 = 45.0;
pub const SMOKE_RADIUS: f32 = 3.3;
pub const SMOKE_TIME: f32 = 15.0;
pub const FIRE_RADIUS: f32 = 2.8;
pub const FIRE_TIME: f32 = 7.0;
/// Dégâts du feu par seconde.
pub const FIRE_DPS: f32 = 38.0;

pub fn is_grenade(id: u8) -> bool {
    id >= GRENADE_BASE && id < GRENADE_BASE + GRENADES.len() as u8
}

/// Nom d'une arme ou d'une grenade à partir de son identifiant.
pub fn item_name(id: u8) -> &'static str {
    if is_grenade(id) {
        GRENADES[(id - GRENADE_BASE) as usize].name
    } else {
        weapon(id).name
    }
}

/// Vrai si le segment [a, b] traverse la sphère de centre `c` et de rayon `r`.
pub fn segment_hits_sphere(a: Vec3, b: Vec3, c: Vec3, r: f32) -> bool {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 1e-8 { ((c - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t).distance_squared(c) < r * r
}

/// Direction de lancer (trajectoire tendue) pour atteindre `to` à la vitesse `v`.
pub fn ballistic(from: Vec3, to: Vec3, v: f32, g: f32) -> Vec3 {
    let d = to - from;
    let x = (d.x * d.x + d.z * d.z).sqrt().max(0.1);
    let disc = v.powi(4) - g * (g * x * x + 2.0 * d.y * v * v);
    let tan = if disc >= 0.0 { (v * v - disc.sqrt()) / (g * x) } else { 1.0 };
    (Vec3::new(d.x / x, 0.0, d.z / x) + Vec3::Y * tan).normalize()
}
