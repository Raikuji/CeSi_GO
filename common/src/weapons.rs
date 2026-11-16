//! Arsenal : 3 pistolets, 3 pistolets-mitrailleurs, 3 fusils d'assaut,
//! 2 fusils de précision, 1 mitrailleuse et le couteau.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Knife,
    Pistol,
    Smg,
    Rifle,
    Sniper,
    Lmg,
}

impl Class {
    pub fn label(self) -> &'static str {
        match self {
            Class::Knife => "Couteau",
            Class::Pistol => "Pistolets",
            Class::Smg => "Pistolets-mitrailleurs",
            Class::Rifle => "Fusils d'assaut",
            Class::Sniper => "Fusils de précision",
            Class::Lmg => "Mitrailleuse",
        }
    }
}

pub struct WeaponDef {
    pub id: u8,
    pub name: &'static str,
    pub class: Class,
    pub price: u32,
    pub kill_reward: u32,
    /// Dégâts de base au corps.
    pub damage: f32,
    /// Part des dégâts qui traverse le gilet (0..1).
    pub armor_pen: f32,
    pub head_mult: f32,
    /// Multiplicateur de dégâts tous les 12 m.
    pub range_mod: f32,
    pub rpm: f32,
    pub auto: bool,
    pub mag: u16,
    pub reserve: u16,
    /// Durée du rechargement (s).
    pub reload: f32,
    /// Dispersion à l'arrêt (degrés).
    pub spread: f32,
    /// Dispersion ajoutée en pleine course (degrés).
    pub spread_move: f32,
    /// Recul vertical par tir (degrés).
    pub recoil_v: f32,
    /// Recul horizontal par tir (degrés).
    pub recoil_h: f32,
    /// Vitesse de déplacement (fraction de la vitesse de base).
    pub speed: f32,
    pub scope: bool,
}

pub const KNIFE: u8 = 0;
pub const DEFAULT_PISTOL: u8 = 1;
pub const KNIFE_RANGE: f32 = 1.9;
/// Délai entre deux coups de couteau rapides (s).
pub const KNIFE_COOLDOWN: f32 = 0.5;
/// Délai après un coup puissant (s).
pub const KNIFE_HEAVY_COOLDOWN: f32 = 1.3;
/// Multiplicateur de dégâts du coup puissant.
pub const KNIFE_HEAVY_MULT: f32 = 2.0;

pub const PRICE_KEVLAR: u32 = 650;
pub const PRICE_KEVLAR_HELMET: u32 = 1000;
pub const PRICE_HELMET_ONLY: u32 = 350;
pub const PRICE_KIT: u32 = 400;

macro_rules! w {
    ($id:expr, $name:expr, $class:ident, $price:expr, $reward:expr, $dmg:expr, $pen:expr, $rm:expr,
     $rpm:expr, $auto:expr, $mag:expr, $res:expr, $rel:expr, $sp:expr, $spm:expr, $rv:expr, $rh:expr,
     $speed:expr, $scope:expr) => {
        WeaponDef {
            id: $id,
            name: $name,
            class: Class::$class,
            price: $price,
            kill_reward: $reward,
            damage: $dmg,
            armor_pen: $pen,
            head_mult: 4.0,
            range_mod: $rm,
            rpm: $rpm,
            auto: $auto,
            mag: $mag,
            reserve: $res,
            reload: $rel,
            spread: $sp,
            spread_move: $spm,
            recoil_v: $rv,
            recoil_h: $rh,
            speed: $speed,
            scope: $scope,
        }
    };
}

#[rustfmt::skip]
pub const WEAPONS: [WeaponDef; 13] = [
    //  id  nom           classe  prix  prime dégâts pén.  portée cad.   auto   charg rés. rech. disp. mouv. recV recH  vit.  lunette
    w!(0,  "Couteau",     Knife,  0,    1500, 45.0,  0.85, 1.0,   120.0, true,  0,    0,   0.0,  0.0,  0.0,  0.0,  0.0, 1.0,  false),
    w!(1,  "P-9",         Pistol, 200,  300,  26.0,  0.50, 0.85,  400.0, false, 15,   60,  2.2,  0.35, 2.0,  1.6,  0.5, 0.96, false),
    w!(2,  "Vipère",      Pistol, 300,  300,  35.0,  0.64, 0.85,  400.0, false, 13,   39,  2.2,  0.40, 2.2,  2.0,  0.6, 0.96, false),
    w!(3,  "Rhino .50",   Pistol, 700,  300,  55.0,  0.93, 0.82,  267.0, false, 7,    35,  2.2,  0.30, 3.5,  5.5,  1.5, 0.92, false),
    w!(4,  "Frelon",      Smg,    1050, 600,  27.0,  0.57, 0.75,  800.0, true,  30,   100, 2.6,  0.90, 1.2,  0.90, 0.50, 0.96, false),
    w!(5,  "Vecteur",     Smg,    1250, 600,  25.0,  0.60, 0.80,  857.0, true,  30,   120, 2.1,  0.70, 1.0,  0.80, 0.45, 0.96, false),
    w!(6,  "Essaim",      Smg,    2350, 300,  26.0,  0.69, 0.86,  857.0, true,  50,   100, 3.3,  0.80, 0.9,  0.75, 0.40, 0.92, false),
    w!(7,  "Kodiak",      Rifle,  2700, 300,  36.0,  0.775, 0.98, 600.0, true,  30,   90,  2.4,  0.25, 4.5,  1.50, 0.70, 0.86, false),
    w!(8,  "Faucon",      Rifle,  3000, 300,  33.0,  0.70, 0.97,  666.0, true,  30,   90,  3.1,  0.20, 4.0,  1.25, 0.55, 0.90, false),
    w!(9,  "Mistral",     Rifle,  2000, 300,  30.0,  0.70, 0.96,  666.0, true,  25,   90,  3.2,  0.30, 4.2,  1.30, 0.65, 0.88, false),
    w!(10, "Éclaireur",   Sniper, 1700, 300,  88.0,  0.85, 0.98,  48.0,  false, 10,   90,  3.7,  0.05, 3.0,  3.0,  0.5, 0.92, true),
    w!(11, "Léviathan",   Sniper, 4750, 100,  115.0, 0.975, 0.99, 41.0,  false, 5,    30,  3.7,  0.02, 5.0,  4.5,  0.8, 0.80, true),
    w!(12, "Mammouth",    Lmg,    5200, 300,  32.0,  0.80, 0.97,  750.0, true,  100,  200, 5.7,  0.50, 5.0,  1.10, 0.60, 0.78, false),
];

pub fn weapon(id: u8) -> &'static WeaponDef {
    // Identifiant inconnu (grenade en main, par exemple) : caractéristiques du couteau.
    WEAPONS.get(id as usize).unwrap_or(&WEAPONS[0])
}
