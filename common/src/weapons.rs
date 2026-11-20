//! Arsenal : 3 pistolets, 3 pistolets-mitrailleurs, 2 fusils à pompe,
//! 3 fusils d'assaut, 2 fusils de précision, 1 mitrailleuse et le couteau.

use glam::Vec3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Knife,
    Pistol,
    Smg,
    Rifle,
    Sniper,
    Lmg,
    Shotgun,
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
            Class::Shotgun => "Fusils à pompe",
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
    /// Poids de l'arme (kg).
    pub weight: f32,
    /// Vitesse de déplacement, en fraction de la vitesse couteau en main :
    /// elle baisse avec le poids (`SLOWDOWN_PER_KG`).
    pub speed: f32,
    pub scope: bool,
    /// Plombs par cartouche (1 pour une arme à balle) ; `damage` est par plomb.
    pub pellets: u8,
    /// Rayon de la gerbe de plombs (degrés).
    pub pellet_cone: f32,
}

pub const KNIFE: u8 = 0;
pub const DEFAULT_PISTOL: u8 = 1;
pub const KNIFE_RANGE: f32 = 1.9;
/// Délai entre deux coups de couteau rapides (s).
pub const KNIFE_COOLDOWN: f32 = 0.5;
/// Délai après un coup puissant (s).
pub const KNIFE_HEAVY_COOLDOWN: f32 = 1.3;
/// Multiplicateur de dégâts du coup puissant.
/// Distance maximale pour ramasser une arme au sol.
pub const PICKUP_RANGE: f32 = 2.6;
pub const KNIFE_HEAVY_MULT: f32 = 2.0;

/// Perte de vitesse par kilogramme au-delà du couteau (fraction de la vitesse de base).
pub const SLOWDOWN_PER_KG: f32 = 0.045;
/// Poids du couteau, l'arme la plus légère : pleine vitesse.
const WEAPONS_LIGHTEST: f32 = 0.3;

pub const PRICE_KEVLAR: u32 = 650;
pub const PRICE_KEVLAR_HELMET: u32 = 1000;
pub const PRICE_HELMET_ONLY: u32 = 350;
pub const PRICE_KIT: u32 = 400;

macro_rules! w {
    ($id:expr, $name:expr, $class:ident, $price:expr, $reward:expr, $dmg:expr, $pen:expr, $rm:expr,
     $rpm:expr, $auto:expr, $mag:expr, $res:expr, $rel:expr, $sp:expr, $spm:expr, $rv:expr, $rh:expr,
     $weight:expr, $scope:expr) => {
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
            weight: $weight,
            speed: 1.0 - SLOWDOWN_PER_KG * ($weight - WEAPONS_LIGHTEST),
            scope: $scope,
            pellets: 1,
            pellet_cone: 0.0,
        }
    };
}

#[rustfmt::skip]
pub const WEAPONS: [WeaponDef; 15] = [
    //  id  nom           classe  prix  prime dégâts pén.  portée cad.   auto   charg rés. rech. disp. mouv. recV recH  kg    lunette
    w!(0,  "Couteau",     Knife,  0,    1500, 45.0,  0.85, 1.0,   120.0, true,  0,    0,   0.0,  0.0,  0.0,  0.0,  0.0, 0.3,  false),
    w!(1,  "P-9",         Pistol, 200,  300,  26.0,  0.50, 0.85,  400.0, false, 15,   60,  2.2,  0.35, 2.0,  1.6,  0.5, 0.7,  false),
    w!(2,  "Vipère",      Pistol, 300,  300,  35.0,  0.64, 0.85,  400.0, false, 13,   39,  2.2,  0.40, 2.2,  2.0,  0.6, 0.8,  false),
    w!(3,  "Rhino .50",   Pistol, 700,  300,  55.0,  0.93, 0.82,  267.0, false, 7,    35,  2.2,  0.30, 3.5,  5.5,  1.5, 2.0,  false),
    w!(4,  "Frelon",      Smg,    1050, 600,  27.0,  0.57, 0.75,  800.0, true,  30,   100, 2.6,  0.90, 1.2,  0.90, 0.50, 2.6,  false),
    w!(5,  "Vecteur",     Smg,    1250, 600,  25.0,  0.60, 0.80,  857.0, true,  30,   120, 2.1,  0.70, 1.0,  0.80, 0.45, 2.5,  false),
    w!(6,  "Essaim",      Smg,    2350, 300,  26.0,  0.69, 0.86,  857.0, true,  50,   100, 3.3,  0.80, 0.9,  0.75, 0.40, 3.4,  false),
    w!(7,  "Kodiak",      Rifle,  2700, 300,  36.0,  0.775, 0.98, 600.0, true,  30,   90,  2.4,  0.25, 4.5,  1.50, 0.70, 3.9,  false),
    w!(8,  "Faucon",      Rifle,  3000, 300,  33.0,  0.70, 0.97,  666.0, true,  30,   90,  3.1,  0.20, 4.0,  1.25, 0.55, 3.4,  false),
    w!(9,  "Mistral",     Rifle,  2000, 300,  30.0,  0.70, 0.96,  666.0, true,  25,   90,  3.2,  0.30, 4.2,  1.30, 0.65, 3.0,  false),
    w!(10, "Éclaireur",   Sniper, 1700, 300,  88.0,  0.85, 0.98,  48.0,  false, 10,   90,  3.7,  0.05, 3.0,  3.0,  0.5, 3.0,  true),
    w!(11, "Léviathan",   Sniper, 4750, 100,  115.0, 0.975, 0.99, 41.0,  false, 5,    30,  3.7,  0.02, 5.0,  4.5,  0.8, 6.5,  true),
    w!(12, "Mammouth",    Lmg,    5200, 300,  32.0,  0.80, 0.97,  750.0, true,  100,  200, 5.7,  0.50, 5.0,  1.10, 0.60, 8.0,  false),
    // Fusils à pompe : dégâts par plomb, très forts de près, presque rien au-delà de 20 m.
    WeaponDef { pellets: 9, pellet_cone: 3.0, head_mult: 2.5,
    ..w!(13, "Sanglier",    Shotgun, 900, 900,  26.0,  0.60, 0.60,  68.0,  false, 6,    32,  2.8,  0.80, 1.0,  7.0,  1.2,  3.6,  false) },
    WeaponDef { pellets: 7, pellet_cone: 3.5, head_mult: 2.5,
    ..w!(14, "Ouragan",     Shotgun, 2000, 600, 20.0,  0.75, 0.55,  200.0, true,  7,    32,  3.0,  1.00, 1.2,  3.5,  0.9,  4.0,  false) },
];

/// Angle d'or : écarte régulièrement les plombs successifs de la gerbe.
const GOLDEN_ANGLE: f32 = 2.399_963;

/// Directions des plombs d'un tir visant `dir` : un plomb au centre, les autres en
/// spirale régulière dans le cône `pellet_cone`. Le motif est fixe, donc identique
/// pour le tireur, le serveur et les autres joueurs. Une arme à balle donne `dir`.
pub fn pellet_dirs(dir: Vec3, def: &WeaponDef) -> impl Iterator<Item = Vec3> {
    let n = def.pellets.max(1) as usize;
    let right = if dir.y.abs() > 0.99 { Vec3::X } else { dir.cross(Vec3::Y).normalize() };
    let up = right.cross(dir);
    let r = def.pellet_cone.to_radians().tan();
    (0..n).map(move |k| {
        if k == 0 {
            return dir;
        }
        let rad = r * (k as f32 / (n - 1) as f32).sqrt();
        let a = k as f32 * GOLDEN_ANGLE;
        (dir + right * (a.cos() * rad) + up * (a.sin() * rad)).normalize()
    })
}

pub fn weapon(id: u8) -> &'static WeaponDef {
    // Identifiant inconnu (grenade en main, par exemple) : caractéristiques du couteau.
    WEAPONS.get(id as usize).unwrap_or(&WEAPONS[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le couteau est l'arme la plus rapide ; plus une arme est lourde, plus on est lent.
    #[test]
    fn heavier_weapons_are_slower_and_the_knife_is_fastest() {
        let knife = weapon(KNIFE).speed;
        assert_eq!(knife, 1.0);
        for a in WEAPONS.iter().skip(1) {
            assert!(a.speed < knife, "{} plus rapide que le couteau", a.name);
            for b in WEAPONS.iter() {
                if a.weight > b.weight {
                    assert!(a.speed < b.speed, "{} ({} kg) pas plus lent que {} ({} kg)", a.name, a.weight, b.name, b.weight);
                }
            }
        }
        // La plus lourde reste jouable.
        assert!(weapon(12).speed > 0.6);
    }

    #[test]
    fn shotgun_pellets_spread_evenly_inside_the_cone() {
        let dir = Vec3::new(0.3, -0.2, 1.0).normalize();
        for d in WEAPONS.iter() {
            let p: Vec<Vec3> = pellet_dirs(dir, d).collect();
            assert_eq!(p.len(), d.pellets as usize, "{}", d.name);
            assert_eq!(p[0], dir, "{} : le premier plomb part dans l'axe", d.name);
            for v in &p {
                assert!((v.length() - 1.0).abs() < 1e-4);
                assert!(v.angle_between(dir).to_degrees() <= d.pellet_cone + 0.01, "{}", d.name);
            }
        }
        // Gerbe bien remplie : des plombs au bord et autour, pas tous d'un côté.
        let p: Vec<Vec3> = pellet_dirs(dir, weapon(13)).collect();
        let edge = p.iter().map(|v| v.angle_between(dir).to_degrees()).fold(0.0f32, f32::max);
        assert!(edge > 2.9);
        let mean = p.iter().copied().sum::<Vec3>().normalize();
        assert!(mean.angle_between(dir).to_degrees() < 0.6);
    }

    /// Le moins cher tire lentement et frappe plus fort ; l'autre tire plus vite.
    #[test]
    fn the_two_shotguns_trade_price_for_rate_of_fire() {
        let (pump, auto) = (weapon(13), weapon(14));
        assert!(pump.class == Class::Shotgun && auto.class == Class::Shotgun);
        assert!(pump.price < auto.price);
        assert!(auto.rpm > pump.rpm * 2.5);
        assert!(pump.damage * pump.pellets as f32 > auto.damage * auto.pellets as f32);
    }
}
