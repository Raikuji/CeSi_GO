//! Carte « Comptoir » : trois couloirs, deux sites de bombe (A et B).
//! La carte est générée par le code, à l'identique sur le client et le serveur.

use crate::voxel::*;
use glam::{vec3, Vec3};
use std::f32::consts::PI;

/// Taille d'une tuile du plan en voxels (4 m).
const TS: i32 = 8;
/// Hauteur du sol (les voxels y < G sont pleins).
const G: i32 = 4;

/// Plan vu de dessus, la première ligne est au nord (z maximal).
/// `#` bâtiment, `.` sol, `A`/`B` sites, `C` départ défenseurs, `T` départ attaquants.
const LAYOUT: [&str; 16] = [
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

const LETTER_A: [&str; 7] = [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"];
const LETTER_B: [&str; 7] = ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."];

pub struct MapInfo {
    pub name: &'static str,
    /// Points d'apparition (position des pieds, lacet).
    pub t_spawns: Vec<(Vec3, f32)>,
    pub ct_spawns: Vec<(Vec3, f32)>,
    /// Zones de pose de la bombe : A puis B (min, max).
    pub sites: [(Vec3, Vec3); 2],
}

impl MapInfo {
    pub fn site_of(&self, p: Vec3) -> Option<u8> {
        self.sites.iter().position(|(a, b)| in_aabb(p, *a, *b)).map(|i| i as u8)
    }
}

pub fn in_aabb(p: Vec3, a: Vec3, b: Vec3) -> bool {
    p.x >= a.x && p.x <= b.x && p.y >= a.y && p.y <= b.y && p.z >= a.z && p.z <= b.z
}

fn hash2(x: i32, z: i32) -> u32 {
    let h = (x as u32).wrapping_mul(73_856_093) ^ (z as u32).wrapping_mul(19_349_663);
    (h ^ (h >> 13)).wrapping_mul(0x5bd1_e995) >> 8
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

pub fn build() -> (World, MapInfo) {
    let n = 16 * TS;
    let mut w = World::new(n, 40, n);
    w.fill(0, 0, 0, n - 1, G - 1, n - 1, SAND);

    let mut t_spawns = Vec::new();
    let mut ct_spawns = Vec::new();

    for (r, row) in LAYOUT.iter().enumerate() {
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
                    let height = if border { 18 } else { 12 + (h % 3) as i32 * 2 };
                    let mat = [BRICK, PLASTER, STONE, BRICK][(h / 3 % 4) as usize];
                    w.fill(x0, G, z0, x1, G + height - 1, z1, mat);
                    w.fill(x0, G, z0, x1, G + 1, z1, STONE);
                    w.fill(x0, G + height - 1, z0, x1, G + height - 1, z1, ROOF);
                }
                b'A' | b'B' => w.fill(x0, G - 1, z0, x1, G - 1, z1, CONCRETE),
                b'C' => {
                    w.fill(x0, G - 1, z0, x1, G - 1, z1, PAVE);
                    ct_spawns.push((centre, PI));
                }
                b'T' => {
                    w.fill(x0, G - 1, z0, x1, G - 1, z1, GRASS);
                    t_spawns.push((centre, 0.0));
                }
                _ => {}
            }
        }
    }

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

    let info = MapInfo {
        name: "Comptoir",
        t_spawns,
        ct_spawns,
        sites: [
            (vec3(44.0, 2.8, 46.0), vec3(60.0, 5.6, 60.0)),
            (vec3(4.0, 1.8, 44.0), vec3(20.0, 4.6, 60.0)),
        ],
    };
    (w, info)
}
