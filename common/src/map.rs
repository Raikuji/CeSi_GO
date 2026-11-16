//! Cartes du jeu : « Comptoir », « Entrepôt » et « Citadelle ».
//! Chaque carte est générée par le code, à l'identique sur le client et le serveur,
//! à partir d'un plan de 16 x 16 tuiles puis d'un décor posé à la main.
//!
//! Pour ajouter une carte : un plan, une fonction de décor, une entrée dans `MAPS`
//! et dans `build`. Deux règles tirées des simulations de bots (`--fast`) :
//! - le chemin le plus court des attaquants doit déboucher en vue des deux sites,
//!   sinon les défenseurs d'un site se retrouvent seuls et la carte penche nettement
//!   du côté des attaquants ;
//! - un passage doit faire au moins 3 voxels de large pour que les bots l'empruntent.

use crate::protocol::Team;
use crate::voxel::*;
use glam::{vec3, Vec3};
use std::f32::consts::PI;

/// Taille d'une tuile du plan en voxels (4 m).
const TS: i32 = 8;
/// Hauteur du sol (les voxels y < G sont pleins).
const G: i32 = 4;

pub struct MapDef {
    pub name: &'static str,
    pub desc: &'static str,
}

/// Cartes disponibles ; l'indice sert d'identifiant sur le réseau.
pub const MAPS: [MapDef; 3] = [
    MapDef { name: "Comptoir", desc: "Ville du désert : trois couloirs, les deux sites au nord." },
    MapDef { name: "Entrepôt", desc: "Zone portuaire : halle couverte pour B, quai à conteneurs pour A." },
    MapDef { name: "Citadelle", desc: "Forteresse enneigée : les deux sites dans la cour, sous le rempart." },
];

/// Minuscules sans accents, pour comparer les noms saisis à la console.
fn plain(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'ô' | 'ö' => 'o',
            'à' | 'â' => 'a',
            'î' | 'ï' => 'i',
            'û' | 'ù' | 'ü' => 'u',
            c => c,
        })
        .collect()
}

/// Retrouve une carte par son numéro (à partir de 1) ou par le début de son nom.
pub fn find(s: &str) -> Option<u8> {
    let key = plain(s);
    if key.is_empty() {
        return None;
    }
    if let Ok(n) = key.parse::<usize>() {
        return (1..=MAPS.len()).contains(&n).then(|| (n - 1) as u8);
    }
    MAPS.iter().position(|m| plain(m.name).starts_with(&key)).map(|i| i as u8)
}

pub struct MapInfo {
    pub id: u8,
    pub name: &'static str,
    /// Points d'apparition (position des pieds, lacet).
    pub t_spawns: Vec<(Vec3, f32)>,
    pub ct_spawns: Vec<(Vec3, f32)>,
    /// Zones de pose de la bombe : A puis B (min, max).
    pub sites: [(Vec3, Vec3); 2],
    /// Zones d'achat : Attaquants puis Défenseurs (min, max).
    pub buy_zones: [(Vec3, Vec3); 2],
}

impl MapInfo {
    pub fn site_of(&self, p: Vec3) -> Option<u8> {
        self.sites.iter().position(|(a, b)| in_aabb(p, *a, *b)).map(|i| i as u8)
    }

    /// Vrai si `p` est dans la zone d'achat de l'équipe (autour de son point de départ).
    pub fn in_buy_zone(&self, team: Team, p: Vec3) -> bool {
        let (a, b) = self.buy_zones[if team == Team::T { 0 } else { 1 }];
        in_aabb(p, a, b)
    }
}

pub fn in_aabb(p: Vec3, a: Vec3, b: Vec3) -> bool {
    p.x >= a.x && p.x <= b.x && p.y >= a.y && p.y <= b.y && p.z >= a.z && p.z <= b.z
}

/// Construit la carte demandée (la première si l'identifiant est inconnu).
pub fn build(id: u8) -> (World, MapInfo) {
    match id {
        1 => entrepot(),
        2 => citadelle(),
        _ => comptoir(),
    }
}

// ---------------------------------------------------------------------------
// Outils communs

const LETTER_A: [&str; 7] = [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"];
const LETTER_B: [&str; 7] = ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."];

/// Matériaux d'une carte pour la pose des tuiles.
struct Theme {
    ground: u8,
    walls: [u8; 4],
    /// Soubassement des bâtiments.
    base: u8,
    roof: u8,
    /// Hauteur des bâtiments : `h0 + (hasard % hn) * 2` voxels.
    h0: i32,
    hn: u32,
    site: u8,
    t_floor: u8,
    ct_floor: u8,
}

type Spawns = Vec<(Vec3, f32)>;

fn hash2(x: i32, z: i32) -> u32 {
    let h = (x as u32).wrapping_mul(73_856_093) ^ (z as u32).wrapping_mul(19_349_663);
    (h ^ (h >> 13)).wrapping_mul(0x5bd1_e995) >> 8
}

/// Pose les tuiles du plan vu de dessus, la première ligne étant au nord (z maximal).
/// `#` bâtiment, `.` sol, `A`/`B` sites, `C` départ défenseurs, `T` départ attaquants.
fn lay(layout: &[&str; 16], th: &Theme) -> (World, Spawns, Spawns) {
    let n = 16 * TS;
    let mut w = World::new(n, 40, n);
    w.fill(0, 0, 0, n - 1, G - 1, n - 1, th.ground);

    let mut t_spawns = Vec::new();
    let mut ct_spawns = Vec::new();

    for (r, row) in layout.iter().enumerate() {
        let tz = 15 - r as i32;
        for (tx, ch) in row.bytes().enumerate() {
            let tx = tx as i32;
            let (x0, z0) = (tx * TS, tz * TS);
            let (x1, z1) = (x0 + TS - 1, z0 + TS - 1);
            let centre = vec3((x0 + TS / 2) as f32 * VS, G as f32 * VS + 0.01, (z0 + TS / 2) as f32 * VS);
            match ch {
                b'#' => {
                    let border = tx == 0 || tz == 0 || tx == 15 || tz == 15;
                    let h = hash2(tx, tz);
                    let height = if border { 18 } else { th.h0 + (h % th.hn) as i32 * 2 };
                    let mat = th.walls[(h / 3 % 4) as usize];
                    w.fill(x0, G, z0, x1, G + height - 1, z1, mat);
                    w.fill(x0, G, z0, x1, G + 1, z1, th.base);
                    w.fill(x0, G + height - 1, z0, x1, G + height - 1, z1, th.roof);
                }
                b'A' | b'B' => w.fill(x0, G - 1, z0, x1, G - 1, z1, th.site),
                b'C' => {
                    w.fill(x0, G - 1, z0, x1, G - 1, z1, th.ct_floor);
                    ct_spawns.push((centre, PI));
                }
                b'T' => {
                    w.fill(x0, G - 1, z0, x1, G - 1, z1, th.t_floor);
                    t_spawns.push((centre, 0.0));
                }
                _ => {}
            }
        }
    }
    (w, t_spawns, ct_spawns)
}

/// Peint une lettre 5x7 sur le sol, lisible en regardant vers le nord.
fn letter(w: &mut World, pat: &[&str; 7], x_start: i32, y: i32, z_top: i32, b: u8) {
    for (r, row) in pat.iter().enumerate() {
        for (c, ch) in row.bytes().enumerate() {
            if ch == b'#' {
                let (x, z) = (x_start - c as i32 * 2, z_top - r as i32 * 2);
                w.fill(x - 1, y, z - 1, x, y, z, b);
            }
        }
    }
}

/// Contour d'une zone au sol.
fn outline(w: &mut World, x0: i32, z0: i32, x1: i32, z1: i32, y: i32, b: u8) {
    w.fill(x0, y, z0, x1, y, z0, b);
    w.fill(x0, y, z1, x1, y, z1, b);
    w.fill(x0, y, z0, x0, y, z1, b);
    w.fill(x1, y, z0, x1, y, z1, b);
}

/// Zone en mètres couvrant les voxels donnés (bornes incluses), entre deux hauteurs.
fn zone(x0: i32, z0: i32, x1: i32, z1: i32, y0: f32, y1: f32) -> (Vec3, Vec3) {
    (vec3(x0 as f32 * VS, y0, z0 as f32 * VS), vec3((x1 + 1) as f32 * VS, y1, (z1 + 1) as f32 * VS))
}

/// Escalier plein de `n` marches (un voxel de haut, deux de profondeur) partant de
/// (x, z) et montant vers (dx, dz) ; `width` est sa largeur sur l'autre axe.
#[allow(clippy::too_many_arguments)]
fn stairs(w: &mut World, x: i32, z: i32, dx: i32, dz: i32, n: i32, width: i32, b: u8) {
    for i in 0..n {
        for k in 0..2 {
            let a = 2 * i + k;
            if dx != 0 {
                w.fill(x + dx * a, G, z, x + dx * a, G + i, z + width - 1, b);
            } else {
                w.fill(x, G, z + dz * a, x + width - 1, G + i, z + dz * a, b);
            }
        }
    }
}

/// Conteneur de 6 m x 2,5 m x 2,5 m, portes métalliques aux deux bouts.
fn container(w: &mut World, x: i32, y: i32, z: i32, along_x: bool, color: u8) {
    if along_x {
        w.fill(x, y, z, x + 11, y + 4, z + 4, color);
        for xe in [x, x + 11] {
            w.fill(xe, y, z + 1, xe, y + 3, z + 3, METAL);
        }
    } else {
        w.fill(x, y, z, x + 4, y + 4, z + 11, color);
        for ze in [z, z + 11] {
            w.fill(x + 1, y, ze, x + 3, y + 3, ze, METAL);
        }
    }
}

/// Étagère d'entrepôt chargée de caisses, alignée nord-sud.
fn rack(w: &mut World, x: i32, z0: i32, z1: i32) {
    w.fill(x, G, z0, x + 1, G + 1, z1, CRATE);
    w.fill(x, G + 2, z0, x + 1, G + 2, z1, METAL);
    w.fill(x, G + 3, z0, x + 1, G + 4, z1, CRATE);
    w.fill(x, G + 5, z0, x + 1, G + 5, z1, METAL);
    w.fill(x, G + 3, z0 + 4, x + 1, G + 4, z0 + 5, 0);
    w.fill(x, G + 3, z1 - 2, x + 1, G + 4, z1 - 2, DARKWOOD);
}

/// Sapin enneigé : on passe sous les branches, seul le tronc fait obstacle.
fn pine(w: &mut World, x: i32, z: i32) {
    w.fill(x, G, z, x + 1, G + 9, z + 1, DARKWOOD);
    w.fill(x - 2, G + 5, z - 2, x + 3, G + 5, z + 3, PINE);
    w.fill(x - 2, G + 6, z - 2, x + 3, G + 6, z + 3, SNOW);
    w.fill(x - 1, G + 7, z - 1, x + 2, G + 7, z + 2, PINE);
    w.fill(x - 1, G + 8, z - 1, x + 2, G + 8, z + 2, SNOW);
    w.fill(x, G + 9, z, x + 1, G + 10, z + 1, PINE);
    w.fill(x, G + 11, z, x + 1, G + 11, z + 1, SNOW);
}

// ---------------------------------------------------------------------------
// Comptoir : trois couloirs, deux sites de bombe au nord.

const COMPTOIR: [&str; 16] = [
    "################",
    "#BBBB#CCCC#AAAA#",
    "#BBBB......AAAA#",
    "#BBBB#CCCC#AAAA#",
    "#BBBB#....#AAAA#",
    "#BBBB......AAAA#",
    "#..####..####..#",
    "#..#.##..##.#..#",
    "#..............#",
    "#..#.##..##.#..#",
    "#..####..####..#",
    "#..............#",
    "###..........###",
    "###.TTTTTTTT.###",
    "###.TTTTTTTT.###",
    "################",
];

fn comptoir() -> (World, MapInfo) {
    let theme = Theme {
        ground: SAND,
        walls: [BRICK, PLASTER, STONE, BRICK],
        base: STONE,
        roof: ROOF,
        h0: 12,
        hn: 3,
        site: CONCRETE,
        t_floor: GRASS,
        ct_floor: PAVE,
    };
    let (mut w, t_spawns, ct_spawns) = lay(&COMPTOIR, &theme);

    // --- Revêtements de sol ---
    w.fill(56, 3, 32, 71, 3, 87, ASPHALT); // couloir central
    w.fill(48, 3, 88, 79, 3, 95, PAVE); // cour des défenseurs
    w.fill(48, 3, 104, 79, 3, 111, PAVE);
    w.fill(8, 3, 40, 23, 3, 79, STONE); // tunnel ouest
    w.fill(24, 3, 56, 55, 3, 63, CONCRETE); // traverse
    w.fill(72, 3, 56, 103, 3, 63, CONCRETE);

    // --- Couloir central : porte au nord, caisses ---
    w.fill(56, 4, 80, 61, 15, 81, BRICK);
    w.fill(66, 4, 80, 71, 15, 81, BRICK);
    w.fill(62, 10, 80, 65, 15, 81, BRICK);
    w.fill(56, 4, 52, 59, 6, 55, CRATE);
    w.fill(68, 4, 66, 71, 7, 69, CRATE);
    w.fill(66, 4, 68, 67, 5, 69, DARKWOOD);

    // --- Tunnel ouest (vers B) : toit, porte, caisses ---
    w.fill(8, 13, 40, 23, 14, 79, ROOF);
    w.fill(8, 4, 78, 13, 12, 79, STONE);
    w.fill(19, 4, 78, 23, 12, 79, STONE);
    w.fill(14, 11, 78, 18, 12, 79, STONE);
    w.fill(8, 4, 48, 11, 6, 51, CRATE);
    w.fill(20, 4, 66, 23, 7, 69, CRATE);

    // --- Allée est (vers A) : couvertures ---
    w.fill(104, 4, 50, 107, 7, 53, CRATE);
    w.fill(108, 4, 50, 109, 5, 51, DARKWOOD);
    w.fill(112, 4, 68, 119, 5, 69, PLASTER);
    w.fill(104, 4, 78, 109, 15, 79, PLASTER);

    // --- Traverse couverte et alcôves ---
    w.fill(24, 11, 56, 55, 12, 63, ROOF);
    w.fill(72, 11, 56, 103, 12, 63, ROOF);
    for x0 in [32, 88] {
        w.fill(x0, 11, 48, x0 + 7, 12, 55, ROOF);
        w.fill(x0, 11, 64, x0 + 7, 12, 71, ROOF);
    }
    w.fill(40, 4, 56, 41, 6, 58, CRATE);
    w.fill(86, 4, 61, 87, 6, 63, CRATE);

    // --- Zone des attaquants ---
    w.fill(60, 4, 34, 67, 7, 37, CRATE);
    w.fill(26, 4, 34, 29, 6, 37, CRATE);
    w.fill(98, 4, 34, 101, 6, 37, CRATE);

    // --- Cour des défenseurs ---
    w.fill(61, 4, 90, 66, 6, 91, PLASTER);

    // --- Site A : plateforme surélevée d'un mètre ---
    w.fill(88, 4, 92, 119, 5, 119, CONCRETE);
    w.fill(88, 4, 91, 119, 4, 91, CONCRETE); // marche sud
    w.fill(87, 4, 104, 87, 4, 111, CONCRETE); // marche côté défenseurs
    outline(&mut w, 88, 92, 119, 119, 5, YELLOW);
    letter(&mut w, &LETTER_A, 108, 5, 113, MARK);
    w.fill(94, 6, 96, 97, 9, 99, CRATE);
    w.fill(98, 6, 96, 99, 7, 97, DARKWOOD);
    w.fill(112, 6, 114, 114, 8, 116, CRATE);
    w.fill(108, 6, 94, 117, 7, 94, PLASTER);
    w.fill(90, 6, 114, 91, 9, 117, BLUE);

    // --- Site B : conteneur, caisses, corniche nord avec escalier ---
    outline(&mut w, 8, 88, 39, 119, 3, YELLOW);
    letter(&mut w, &LETTER_B, 28, 3, 113, MARK);
    w.fill(14, 4, 90, 25, 9, 95, GREEN);
    w.fill(32, 4, 100, 35, 7, 103, CRATE);
    w.fill(30, 4, 100, 31, 5, 101, DARKWOOD);
    w.fill(10, 4, 104, 12, 6, 106, CRATE);
    w.fill(8, 4, 116, 31, 6, 119, CONCRETE);
    w.fill(32, 4, 116, 32, 6, 119, CONCRETE);
    w.fill(33, 4, 116, 33, 5, 119, CONCRETE);
    w.fill(34, 4, 116, 34, 4, 119, CONCRETE);
    w.fill(34, 4, 84, 37, 6, 87, BLUE);

    // --- Zones d'achat, délimitées par une ligne blanche au sol ---
    outline(&mut w, 24, 8, 103, 31, 3, WHITE);
    outline(&mut w, 48, 96, 79, 119, 3, WHITE);

    let info = MapInfo {
        id: 0,
        name: MAPS[0].name,
        t_spawns,
        ct_spawns,
        sites: [zone(88, 92, 119, 119, 2.8, 5.6), zone(8, 88, 39, 119, 1.8, 4.6)],
        buy_zones: [zone(24, 8, 103, 31, 1.5, 6.0), zone(48, 96, 79, 119, 1.5, 6.0)],
    };
    (w, info)
}

// ---------------------------------------------------------------------------
// Entrepôt : zone portuaire. À l'ouest, un hangar couvert mène à la halle du
// site B ; à l'est, une cour à conteneurs mène au quai du site A.

const ENTREPOT: [&str; 16] = [
    "################",
    "#BBBBB#CCCC#...#",
    "#BBBBB..CC.#AAA#",
    "#BBBBB#CCCC#AAA#",
    "#BBBBB......AAA#",
    "####..##.###AAA#",
    "#.....##.#.....#",
    "#.............##",
    "#.....##.##...##",
    "#.....##.##....#",
    "#........####..#",
    "##.#####..###..#",
    "#..............#",
    "###.TTTTTTTT.###",
    "###.TTTTTTTT.###",
    "################",
];

fn entrepot() -> (World, MapInfo) {
    let theme = Theme {
        ground: ASPHALT,
        walls: [METAL, CONCRETE, BRICK, METAL],
        base: CONCRETE,
        roof: ROOF,
        h0: 14,
        hn: 2,
        site: CONCRETE,
        t_floor: PAVE,
        ct_floor: PAVE,
    };
    let (mut w, t_spawns, ct_spawns) = lay(&ENTREPOT, &theme);

    // --- Sols ---
    w.fill(8, 3, 40, 47, 3, 87, CONCRETE); // hangar
    w.fill(48, 3, 40, 63, 3, 47, CONCRETE); // passages vers le couloir central
    w.fill(48, 3, 64, 63, 3, 71, CONCRETE);
    w.fill(64, 3, 32, 71, 3, 95, PAVE); // couloir central
    w.fill(56, 3, 88, 87, 3, 95, PAVE); // cour des défenseurs
    w.fill(56, 3, 104, 87, 3, 111, PAVE);

    // --- Hangar et halle B : toit, verrières, linteaux des portes ---
    w.fill(8, G + 13, 40, 47, G + 13, 119, METAL);
    for (x, z) in [(14, 52), (30, 52), (14, 100), (30, 100)] {
        w.fill(x, G + 13, z, x + 3, G + 13, z + 7, 0);
    }
    w.fill(16, G + 7, 32, 23, G + 13, 39, METAL); // porte sud du hangar
    w.fill(48, G + 7, 40, 63, G + 13, 47, CONCRETE); // passages vers le couloir central
    w.fill(48, G + 7, 64, 63, G + 13, 71, CONCRETE);
    w.fill(32, G + 8, 80, 47, G + 12, 87, CONCRETE); // cloison hangar / halle
    w.fill(32, G, 82, 39, G + 7, 85, CONCRETE); // porte de 3 m vers la halle
    w.fill(46, G, 82, 47, G + 7, 85, CONCRETE);
    w.fill(48, G + 7, 88, 55, G + 13, 95, CONCRETE); // porte de la halle sur la cour
    w.fill(48, G + 7, 104, 55, G + 13, 111, CONCRETE); // porte de la halle côté défenseurs

    // --- Hangar : étagères et palettes ---
    for x in [16, 26, 36] {
        rack(&mut w, x, 50, 61);
    }
    w.fill(8, G, 76, 11, G + 3, 79, CRATE);
    w.fill(12, G, 78, 13, G + 1, 79, DARKWOOD);
    w.fill(44, G, 72, 47, G + 2, 75, CRATE);
    w.fill(28, G, 42, 31, G + 1, 45, DARKWOOD);

    // --- Site B : mezzanine en L, escalier, conteneur ---
    outline(&mut w, 8, 88, 39, 119, 3, YELLOW);
    letter(&mut w, &LETTER_B, 36, 3, 109, MARK);
    w.fill(8, G + 5, 96, 12, G + 5, 119, METAL);
    w.fill(13, G + 5, 115, 30, G + 5, 119, METAL);
    w.fill(12, G + 5, 96, 12, G + 5, 114, YELLOW);
    w.fill(8, G + 5, 96, 11, G + 5, 96, YELLOW);
    w.fill(13, G + 5, 115, 30, G + 5, 115, YELLOW);
    for (x, z) in [(12, 96), (12, 106), (21, 115), (30, 115)] {
        w.fill(x, G, z, x, G + 4, z, METAL);
    }
    stairs(&mut w, 40, 115, -1, 0, 5, 5, METAL);
    // La rangée sud de la halle reste dégagée : elle donne sur la cour par la porte est.
    container(&mut w, 14, G, 100, true, GREEN);
    w.fill(34, G, 110, 37, G + 3, 113, CRATE);
    w.fill(32, G, 112, 33, G + 1, 113, DARKWOOD);
    w.fill(42, G, 98, 45, G + 2, 101, CRATE);

    // --- Couloir central : porte, caisses, conteneur devant les défenseurs ---
    w.fill(64, G, 84, 65, G + 11, 85, BRICK);
    w.fill(70, G, 84, 71, G + 11, 85, BRICK);
    w.fill(66, G + 6, 84, 69, G + 11, 85, BRICK);
    w.fill(68, G, 58, 71, G + 3, 61, CRATE);
    w.fill(76, G, 32, 79, G + 2, 35, CRATE);
    w.fill(60, G, 92, 63, G + 1, 95, DARKWOOD); // palettes basses dans la cour
    w.fill(74, G, 88, 77, G + 1, 91, DARKWOOD);

    // --- Cour à conteneurs ---
    container(&mut w, 96, G, 58, false, BLUE);
    container(&mut w, 96, G + 5, 58, false, RUST);
    container(&mut w, 104, G, 50, true, RUST);
    container(&mut w, 82, G, 75, true, GREEN);
    w.fill(116, G, 34, 119, G + 3, 37, CRATE);
    w.fill(108, G, 66, 111, G + 2, 69, CRATE);

    // --- Site A : quai, conteneurs, portique ---
    w.fill(88, G + 7, 88, 95, G + 9, 95, BRICK); // porche côté défenseurs, porte de 2 m
    w.fill(90, G, 88, 93, G + 6, 89, BRICK);
    w.fill(90, G, 94, 93, G + 6, 95, BRICK);
    w.fill(96, G, 78, 112, G + 8, 79, CONCRETE); // mur sud du quai, percé d'une porte
    w.fill(117, G, 78, 119, G + 8, 79, CONCRETE);
    w.fill(113, G + 7, 78, 116, G + 8, 79, CONCRETE);
    outline(&mut w, 96, 80, 119, 111, 3, YELLOW);
    letter(&mut w, &LETTER_A, 109, 3, 109, MARK);
    container(&mut w, 100, G, 82, true, BLUE);
    container(&mut w, 115, G, 98, false, GREEN);
    container(&mut w, 115, G + 5, 98, false, RUST);
    container(&mut w, 100, G, 114, true, RUST);
    w.fill(114, G, 90, 117, G + 2, 92, CRATE);
    w.fill(96, G, 94, 97, G + 24, 95, YELLOW);
    w.fill(118, G, 94, 119, G + 24, 95, YELLOW);
    w.fill(92, G + 25, 94, 122, G + 26, 95, YELLOW);
    w.fill(108, G + 22, 93, 111, G + 24, 96, METAL);
    w.fill(109, G + 14, 94, 110, G + 21, 95, DARKSTONE);
    container(&mut w, 104, G + 9, 92, true, BLUE);

    // --- Zone des attaquants ---
    w.fill(20, G, 26, 23, G + 3, 29, CRATE);
    w.fill(100, G, 26, 103, G + 2, 29, CRATE);

    // --- Zones d'achat ---
    outline(&mut w, 24, 8, 103, 31, 3, WHITE);
    outline(&mut w, 56, 96, 87, 119, 3, WHITE);

    let info = MapInfo {
        id: 1,
        name: MAPS[1].name,
        t_spawns,
        ct_spawns,
        sites: [zone(96, 80, 119, 111, 1.8, 4.6), zone(8, 88, 39, 119, 1.8, 4.6)],
        buy_zones: [zone(24, 8, 103, 31, 1.5, 6.0), zone(56, 96, 87, 119, 1.5, 6.0)],
    };
    (w, info)
}

// ---------------------------------------------------------------------------
// Citadelle : forteresse enneigée. Les deux sites sont dans la cour, de part et
// d'autre de la porte sud ; les défenseurs arrivent par le nord et tiennent le rempart.

const CITADELLE: [&str; 16] = [
    "################",
    "#####CCCCCC#####",
    "#....CCCCCC....#",
    "#..............#",
    "#.####.##.####.#",
    "#.#AAA....BBB#.#",
    "#..AAA.##.BBB..#",
    "#.#AAA....BBB#.#",
    "#.#####..#####.#",
    "#..............#",
    "#.###.#..#.###.#",
    "#..............#",
    "###..........###",
    "###.TTTTTTTT.###",
    "###.TTTTTTTT.###",
    "################",
];

fn citadelle() -> (World, MapInfo) {
    let theme = Theme {
        ground: SNOW,
        walls: [STONE, DARKSTONE, STONE, PLASTER],
        base: DARKSTONE,
        roof: ROOF,
        h0: 12,
        hn: 3,
        site: PAVE,
        t_floor: STONE,
        ct_floor: STONE,
    };
    let (mut w, t_spawns, ct_spawns) = lay(&CITADELLE, &theme);

    // --- Sols dégagés de la neige ---
    w.fill(24, 3, 8, 103, 3, 31, STONE); // camp des attaquants
    w.fill(40, 3, 104, 87, 3, 119, STONE); // cour des défenseurs
    w.fill(8, 3, 50, 119, 3, 53, PAVE); // rue du village
    w.fill(48, 3, 64, 79, 3, 95, STONE); // cour de la forteresse et portes nord
    w.fill(16, 3, 72, 23, 3, 79, STONE); // poternes
    w.fill(104, 3, 72, 111, 3, 79, STONE);
    w.fill(56, 3, 40, 71, 3, 63, PAVE); // allée de la porte sud
    // Mare gelée
    w.fill(54, 3, 33, 73, 3, 38, ICE);
    for (x, z) in [(54, 33), (72, 33), (54, 37), (72, 37)] {
        w.fill(x, 3, z, x + 1, 3, z + 1, SNOW);
    }

    // --- Rempart nord : chemin de ronde au-dessus des deux portes, créneaux, escaliers ---
    w.fill(16, G, 88, 111, G + 20, 95, 0);
    w.fill(16, G, 88, 111, G + 6, 95, STONE);
    w.fill(16, G, 88, 111, G + 1, 95, DARKSTONE);
    w.fill(16, G + 6, 89, 111, G + 6, 95, SNOW);
    for x0 in [48, 72] {
        w.fill(x0, G, 88, x0 + 7, G + 5, 95, 0);
    }
    for x in (16..112).step_by(4) {
        w.fill(x, G + 7, 88, x + 1, G + 8, 88, STONE);
    }
    stairs(&mut w, 16, 96, 1, 0, 6, 3, STONE);
    stairs(&mut w, 111, 96, -1, 0, 6, 3, STONE);

    // --- Tour centrale, face à la porte sud ---
    w.fill(56, G, 72, 71, G + 22, 79, STONE);
    w.fill(56, G, 72, 71, G + 1, 79, DARKSTONE);
    w.fill(56, G + 23, 72, 71, G + 23, 79, SNOW);
    for (x, z) in [(56, 72), (70, 72), (56, 78), (70, 78), (63, 72), (63, 78)] {
        w.fill(x, G + 23, z, x + 1, G + 24, z + 1, STONE);
    }
    for z in [72, 79] {
        w.fill(63, G + 12, z, 64, G + 15, z, DARKSTONE);
    }

    // --- Porte sud et poternes est / ouest ---
    w.fill(56, G, 56, 58, G + 11, 63, STONE);
    w.fill(69, G, 56, 71, G + 11, 63, STONE);
    w.fill(59, G + 7, 56, 68, G + 11, 63, STONE);
    w.fill(56, G + 12, 56, 71, G + 12, 63, SNOW);
    w.fill(59, G + 6, 59, 68, G + 6, 60, DARKWOOD); // herse relevée
    for x0 in [16, 104] {
        w.fill(x0, G, 72, x0 + 7, G + 11, 73, STONE);
        w.fill(x0, G, 78, x0 + 7, G + 11, 79, STONE);
        w.fill(x0, G + 6, 74, x0 + 7, G + 11, 77, STONE);
        w.fill(x0, G + 12, 72, x0 + 7, G + 12, 79, SNOW);
    }

    // --- Site A (aile ouest) : puits, muret face à la porte, caisses ---
    outline(&mut w, 24, 64, 47, 87, 3, YELLOW);
    letter(&mut w, &LETTER_A, 40, 3, 83, MARK);
    w.fill(25, G, 65, 29, G + 1, 69, STONE);
    w.fill(26, G + 1, 66, 28, G + 1, 68, 0);
    w.fill(26, G, 66, 28, G, 68, ICE);
    w.fill(44, G, 68, 45, G + 2, 75, STONE);
    w.fill(26, G, 82, 29, G + 3, 85, CRATE);
    w.fill(30, G, 84, 31, G + 1, 85, DARKWOOD);

    // --- Site B (aile est) : chapelle en ruine ---
    outline(&mut w, 80, 64, 103, 87, 3, YELLOW);
    letter(&mut w, &LETTER_B, 96, 3, 83, MARK);
    w.fill(82, G, 68, 83, G + 2, 75, STONE);
    w.fill(98, G, 82, 103, G + 4, 83, STONE);
    w.fill(100, G + 2, 82, 101, G + 3, 83, 0);
    w.fill(98, G, 65, 101, G + 3, 68, CRATE);
    w.fill(96, G, 65, 97, G + 1, 66, DARKWOOD);

    // --- Cour entre les deux sites ---
    w.fill(48, G, 80, 51, G + 2, 83, CRATE);
    w.fill(76, G, 80, 79, G + 2, 83, CRATE);
    w.fill(62, G, 68, 65, G + 1, 71, DARKWOOD); // palettes au pied de la tour
    w.fill(50, G, 64, 53, G + 3, 67, CRATE); // abris des attaquants de chaque côté de la porte
    w.fill(74, G, 64, 77, G + 3, 67, CRATE);

    // --- Ruelles extérieures, fermées chacune par une porte étroite ---
    for x0 in [8, 112] {
        w.fill(x0, G + 7, 58, x0 + 7, G + 9, 61, STONE);
        w.fill(x0, G + 10, 58, x0 + 7, G + 10, 61, SNOW);
        w.fill(x0, G, 58, x0 + 1, G + 6, 61, STONE);
        w.fill(x0 + 6, G, 58, x0 + 7, G + 6, 61, STONE);
    }
    w.fill(12, G, 64, 15, G + 3, 67, CRATE);
    w.fill(112, G, 42, 115, G + 3, 45, CRATE);

    // --- Rue nord, côté défenseurs ---
    w.fill(62, G, 100, 65, G + 3, 103, CRATE);
    w.fill(60, G, 102, 61, G + 1, 103, DARKWOOD);

    // --- Village : statue, passages couverts, caisses ---
    w.fill(62, G, 50, 65, G + 1, 53, DARKSTONE);
    w.fill(63, G + 2, 51, 64, G + 7, 52, STONE);
    w.fill(62, G + 5, 51, 65, G + 5, 52, STONE);
    for x0 in [40, 80] {
        w.fill(x0, G + 7, 42, x0 + 7, G + 8, 45, DARKWOOD);
        w.fill(x0, G + 9, 42, x0 + 7, G + 9, 45, SNOW);
    }
    w.fill(26, G, 34, 29, G + 2, 37, CRATE);
    w.fill(98, G, 34, 101, G + 2, 37, CRATE);
    w.fill(30, G, 50, 32, G + 2, 52, CRATE);
    w.fill(92, G, 51, 94, G + 2, 53, CRATE);

    // --- Sapins ---
    pine(&mut w, 27, 12);
    pine(&mut w, 99, 12);
    pine(&mut w, 12, 107);
    pine(&mut w, 114, 107);
    pine(&mut w, 36, 101);
    pine(&mut w, 90, 101);

    // --- Zones d'achat ---
    outline(&mut w, 24, 8, 103, 31, 3, WHITE);
    outline(&mut w, 40, 104, 87, 119, 3, WHITE);

    let info = MapInfo {
        id: 2,
        name: MAPS[2].name,
        t_spawns,
        ct_spawns,
        sites: [zone(24, 64, 47, 87, 1.8, 4.6), zone(80, 64, 103, 87, 1.8, 4.6)],
        buy_zones: [zone(24, 8, 103, 31, 1.5, 6.0), zone(40, 104, 87, 119, 1.5, 6.0)],
    };
    (w, info)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chaque carte doit offrir des départs dégagés, dans la zone d'achat de l'équipe.
    #[test]
    fn spawns_are_clear_and_in_buy_zone() {
        for id in 0..MAPS.len() as u8 {
            let (w, info) = build(id);
            assert_eq!(info.id, id);
            assert!(info.t_spawns.len() >= 8 && info.ct_spawns.len() >= 8, "{}", info.name);
            for (team, list) in [(Team::T, &info.t_spawns), (Team::CT, &info.ct_spawns)] {
                for (p, _) in list {
                    let min = *p + vec3(-0.3, 0.02, -0.3);
                    let max = *p + vec3(0.3, 1.8, 0.3);
                    assert!(!w.aabb_hits(min, max), "{} : départ encombré en {:?}", info.name, p);
                    assert!(info.in_buy_zone(team, *p), "{} : départ hors zone d'achat en {:?}", info.name, p);
                }
            }
        }
    }

    #[test]
    fn find_by_name_or_number() {
        assert_eq!(find("2"), Some(1));
        assert_eq!(find("entrepot"), Some(1));
        assert_eq!(find("Cita"), Some(2));
        assert_eq!(find("0"), None);
        assert_eq!(find("inconnue"), None);
    }
}
