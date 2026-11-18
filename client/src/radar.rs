//! Minimap : plan de la carte vu du dessus, avec les joueurs, la bombe, les
//! sites et les fumées. Elle montre les alentours du joueur (`VIEW` mètres de
//! côté, centrés sur lui), le nord de la carte (côté Défenseurs) en haut.
//!
//! Vue du dessus, le monde est droitier : pour ne pas inverser la gauche et la
//! droite par rapport à la vue du joueur, les x croissent vers la gauche quand
//! les z croissent vers le haut.

use crate::game::Game;
use crate::ui::{Ui, CT_COLOR, T_COLOR};
use cesi_common::grenades::{FIRE, FIRE_RADIUS, SMOKE, SMOKE_RADIUS, SMOKE_TIME};
use cesi_common::map::MapInfo;
use cesi_common::protocol::{BombState, Team};
use cesi_common::voxel::{block_color, World, VS};
use macroquad::prelude::*;

/// Côté de la zone montrée par la minimap, en mètres.
pub const VIEW: f32 = 68.0;

/// Hauteur franchissable d'une case à l'autre (en voxels), saut compris.
const CLIMB: i32 = 2;

/// Plan de la carte : image carrée `n`×`n` (RGBA), recadrée sur la partie praticable.
pub struct Plan {
    pub rgba: Vec<u8>,
    /// Coin du cadrage, en voxels.
    pub x0: i32,
    pub z0: i32,
    pub n: i32,
    /// Cases où l'on peut marcher (pour les tests).
    #[cfg_attr(not(test), allow(dead_code))]
    pub floor_px: usize,
}

/// Plan prêt à dessiner, pour la carte `map`.
pub struct Baked {
    map: u8,
    tex: Texture2D,
    x0: i32,
    z0: i32,
    n: i32,
}

/// Sol praticable le plus bas d'une colonne : un voxel plein avec la place de se tenir debout au-dessus.
fn floor_of(w: &World, x: i32, z: i32) -> Option<i32> {
    (0..w.sy - 4).find(|&y| w.solid(x, y, z) && (1..=4).all(|d| !w.solid(x, y + d, z)))
}

/// Dessine le plan : sol accessible depuis les points de départ, murs et obstacles détourés.
pub fn bake(w: &World, info: &MapInfo) -> Plan {
    let (sx, sz) = (w.sx, w.sz);
    let idx = |x: i32, z: i32| (z * sx + x) as usize;
    let inside = |x: i32, z: i32| x >= 0 && z >= 0 && x < sx && z < sz;
    let floor: Vec<Option<i32>> = (0..sz).flat_map(|z| (0..sx).map(move |x| floor_of(w, x, z))).collect();

    // Cases accessibles en marchant (on monte d'au plus CLIMB, on peut toujours descendre).
    let mut reach = vec![false; (sx * sz) as usize];
    let mut stack: Vec<(i32, i32)> = Vec::new();
    for (p, _) in info.t_spawns.iter().chain(&info.ct_spawns) {
        let (x, z) = ((p.x / VS) as i32, (p.z / VS) as i32);
        if inside(x, z) && floor[idx(x, z)].is_some() && !reach[idx(x, z)] {
            reach[idx(x, z)] = true;
            stack.push((x, z));
        }
    }
    while let Some((x, z)) = stack.pop() {
        let h = floor[idx(x, z)].unwrap_or(0);
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (x + dx, z + dz);
            if !inside(nx, nz) || reach[idx(nx, nz)] {
                continue;
            }
            if let Some(nh) = floor[idx(nx, nz)] {
                if nh - h <= CLIMB {
                    reach[idx(nx, nz)] = true;
                    stack.push((nx, nz));
                }
            }
        }
    }
    let at = |x: i32, z: i32| inside(x, z) && reach[idx(x, z)];

    // Cadrage carré sur la zone praticable.
    let (mut bx0, mut bz0, mut bx1, mut bz1) = (sx, sz, 0, 0);
    for z in 0..sz {
        for x in 0..sx {
            if reach[idx(x, z)] {
                (bx0, bz0, bx1, bz1) = (bx0.min(x), bz0.min(z), bx1.max(x), bz1.max(z));
            }
        }
    }
    if bx0 > bx1 {
        (bx0, bz0, bx1, bz1) = (0, 0, sx - 1, sz - 1);
    }
    let n = (bx1 - bx0).max(bz1 - bz0) + 1 + 4;
    let x0 = (bx0 + bx1 + 1 - n) / 2;
    let z0 = (bz0 + bz1 + 1 - n) / 2;

    let base = info.t_spawns.first().and_then(|(p, _)| floor_of(w, (p.x / VS) as i32, (p.z / VS) as i32)).unwrap_or(0);
    let mut rgba = vec![0u8; (n * n * 4) as usize];
    let mut floor_px = 0;
    for j in 0..n {
        for i in 0..n {
            let (x, z) = (x0 + n - 1 - i, z0 + n - 1 - j);
            let px: [u8; 4] = if at(x, z) {
                floor_px += 1;
                let h = floor[idx(x, z)].unwrap_or(base);
                let c = block_color(w.block(x, h, z));
                // Un rebord (on domine une case plus basse) est éclairci.
                let lip = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dz)| {
                    at(x + dx, z + dz) && floor[idx(x + dx, z + dz)].map_or(false, |nh| h - nh >= CLIMB)
                });
                let k = (0.78 + 0.05 * (h - base).clamp(0, 8) as f32) * if lip { 1.18 } else { 1.0 };
                let f = |v: u8| ((v as f32 * 0.4 + 118.0 * 0.6) * k).min(255.0) as u8;
                [f(c[0]), f(c[1]), f(c[2]), 235]
            } else if inside(x, z) && (-1..=1).any(|dz| (-1..=1).any(|dx| at(x + dx, z + dz))) {
                // Bord d'un mur ou d'un obstacle.
                [214, 218, 226, 255]
            } else {
                [22, 24, 30, 160]
            };
            let o = ((j * n + i) * 4) as usize;
            rgba[o..o + 4].copy_from_slice(&px);
        }
    }
    Plan { rgba, x0, z0, n, floor_px }
}

/// Emplacement de la minimap à l'écran : coin haut droit, côté `s` (pixels).
pub fn rect(w: f32, h: f32) -> (f32, f32, f32) {
    let k = h / 720.0;
    let s = 180.0 * k;
    (w - s - 16.0 * k, 16.0 * k, s)
}

/// Triangle pointant dans la direction `dir` (écran).
fn arrow(c: Vec2, dir: Vec2, size: f32, fill: Color) {
    let side = vec2(-dir.y, dir.x);
    let (tip, l, r) = (c + dir * size, c - dir * size * 0.6 + side * size * 0.7, c - dir * size * 0.6 - side * size * 0.7);
    let back = c - dir * size * 0.25;
    let edge = Color::new(0.0, 0.0, 0.0, 0.8);
    draw_triangle(tip + dir * 1.5, l + (l - c).normalize_or_zero() * 1.5, r + (r - c).normalize_or_zero() * 1.5, edge);
    draw_triangle(tip, l, back, fill);
    draw_triangle(tip, back, r, fill);
}

fn dot(c: Vec2, r: f32, fill: Color) {
    draw_circle(c.x, c.y, r + 1.5, Color::new(0.0, 0.0, 0.0, 0.8));
    draw_circle(c.x, c.y, r, fill);
}

impl Game {
    /// Un ennemi en `target` est-il vu par un membre vivant de l'équipe `team` ?
    /// Il faut une ligne de vue dégagée, hors fumée, dans le champ de vision.
    fn spotted(&self, target: Vec3, team: Team) -> bool {
        let Some(snap) = self.snap() else { return false };
        let smokes: Vec<Vec3> = snap
            .areas
            .iter()
            .filter(|a| a.kind == SMOKE && SMOKE_TIME - a.time_left > 1.0)
            .map(|a| a.pos + Vec3::Y * 1.0)
            .collect();
        self.views.iter().filter(|v| v.alive && v.team == team).any(|v| {
            let (pos, yaw) = if Some(v.id) == self.my_id { (self.body.pos, self.yaw) } else { (v.pos, v.yaw) };
            let eye = pos + Vec3::Y * 1.6;
            let d = target + Vec3::Y * 1.1 - eye;
            let dist = d.length();
            if dist < 0.01 {
                return true;
            }
            let hd = vec2(d.x, d.z).normalize_or_zero();
            if dist > 3.0 && vec2(yaw.sin(), yaw.cos()).dot(hd) < 0.34 {
                return false;
            }
            if self.world.raycast(eye, d / dist, dist).is_some() {
                return false;
            }
            !smokes.iter().any(|c| {
                let t = ((*c - eye).dot(d) / (dist * dist)).clamp(0.0, 1.0);
                (eye + d * t).distance(*c) < SMOKE_RADIUS * 0.85
            })
        })
    }

    /// Dessine la minimap dans le carré (`x`, `y`, côté `s`).
    pub fn draw_radar(&self, ui: &Ui, x: f32, y: f32, s: f32) {
        let Some(snap) = self.snap() else { return };
        let mut cache = self.radar.borrow_mut();
        if cache.as_ref().map_or(true, |b| b.map != self.info.id) {
            let p = bake(&self.world, &self.info);
            let tex = Texture2D::from_rgba8(p.n as u16, p.n as u16, &p.rgba);
            // Lissé : à cette échelle un voxel fait moins de 2 pixels en 720p.
            tex.set_filter(FilterMode::Linear);
            *cache = Some(Baked { map: self.info.id, tex, x0: p.x0, z0: p.z0, n: p.n });
        }
        let b = cache.as_ref().unwrap();
        let k = screen_height() / 720.0;
        let n = b.n as f32;

        // Centre de la vue : soi-même, le joueur suivi une fois éliminé, sinon le milieu de la carte.
        let me = self.me();
        let centre = if me.is_some_and(|m| m.alive) {
            self.body.pos
        } else if let Some(v) = self.spectate.and_then(|id| self.views.iter().find(|v| v.id == id && v.alive)) {
            v.pos
        } else {
            vec3((b.x0 as f32 + n / 2.0) * VS, 0.0, (b.z0 as f32 + n / 2.0) * VS)
        };
        // Coordonnées dans l'image du plan (en voxels), puis à l'écran.
        let plan = |p: Vec3| vec2((b.x0 + b.n) as f32 - p.x / VS, (b.z0 + b.n) as f32 - p.z / VS);
        let span = VIEW / VS;
        let origin = plan(centre) - Vec2::splat(span / 2.0);
        let per_px = s / span;
        let to = |p: Vec3| vec2(x, y) + (plan(p) - origin) * per_px;
        let box_ = Rect::new(x, y, s, s);
        let inside = |c: Vec2| box_.contains(c);
        // Direction à l'écran d'un regard de lacet `yaw`.
        let screen_dir = |yaw: f32| vec2(-yaw.sin(), -yaw.cos());
        let per_m = s / VIEW;

        draw_rectangle(x, y, s, s, Color::new(0.03, 0.035, 0.05, 0.85));
        // Partie du plan qui tombe dans la vue (rien au-delà des bords de la carte).
        let (u0, v0) = (origin.x.max(0.0), origin.y.max(0.0));
        let (u1, v1) = ((origin.x + span).min(n), (origin.y + span).min(n));
        if u1 > u0 && v1 > v0 {
            draw_texture_ex(
                &b.tex,
                x + (u0 - origin.x) * per_px,
                y + (v0 - origin.y) * per_px,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2((u1 - u0) * per_px, (v1 - v0) * per_px)),
                    source: Some(Rect::new(u0, v0, u1 - u0, v1 - v0)),
                    ..Default::default()
                },
            );
        }

        // Sites de pose, découpés au bord de la vue.
        for (i, (lo, hi)) in self.info.sites.iter().enumerate() {
            let (a, c) = (to(*lo), to(*hi));
            let r = Rect::new(a.x.min(c.x), a.y.min(c.y), (a.x - c.x).abs(), (a.y - c.y).abs());
            if let Some(r) = r.intersect(box_) {
                draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.95, 0.55, 0.15, 0.2));
                if r.w > 14.0 * k && r.h > 18.0 * k {
                    let label = if i == 0 { "A" } else { "B" };
                    ui.shadow_px_c(label, r.x + r.w / 2.0, r.y + r.h / 2.0 + 7.0 * k, 20.0 * k, Color::new(1.0, 0.82, 0.4, 0.95));
                }
            }
        }

        // Fumées et flammes.
        for a in &snap.areas {
            let c = to(a.pos);
            if !inside(c) {
                continue;
            }
            match a.kind {
                SMOKE => draw_circle(c.x, c.y, SMOKE_RADIUS * per_m, Color::new(0.78, 0.78, 0.8, 0.8)),
                FIRE => draw_circle(c.x, c.y, FIRE_RADIUS * per_m, Color::new(1.0, 0.42, 0.1, 0.55)),
                _ => {}
            }
        }

        let my_team = me.map_or(Team::T, |m| m.team);
        let blink = (self.now * 4.0) as i64 % 2 == 0;

        // Bombe au sol ou posée.
        match snap.bomb {
            BombState::Dropped(p) if my_team == Team::T && inside(to(p)) => {
                let c = to(p);
                draw_rectangle(c.x - 4.0 * k, c.y - 3.0 * k, 8.0 * k, 6.0 * k, Color::new(1.0, 0.5, 0.2, 1.0));
            }
            BombState::Planted { pos, .. } => {
                // Toujours signalée : au bord de la vue si elle est plus loin.
                let c = to(pos).clamp(vec2(x + 6.0 * k, y + 5.0 * k), vec2(x + s - 6.0 * k, y + s - 5.0 * k));
                let col = if blink { Color::new(1.0, 0.2, 0.15, 1.0) } else { Color::new(0.5, 0.05, 0.05, 1.0) };
                draw_rectangle(c.x - 5.0 * k, c.y - 4.0 * k, 10.0 * k, 8.0 * k, col);
            }
            _ => {}
        }

        // Joueurs : coéquipiers, puis ennemis repérés, puis soi-même par-dessus.
        let carrier = match snap.bomb {
            BombState::Carried(id) if my_team == Team::T => Some(id),
            _ => None,
        };
        for v in self.views.iter().filter(|v| v.alive && Some(v.id) != self.my_id) {
            let c = to(v.pos);
            if !inside(c) {
                continue;
            }
            if v.team == my_team {
                let col = if v.team == Team::T { T_COLOR } else { CT_COLOR };
                if carrier == Some(v.id) {
                    draw_circle_lines(c.x, c.y, 7.5 * k, 2.0 * k, Color::new(1.0, 0.45, 0.15, 1.0));
                }
                if self.spectate == Some(v.id) && me.map_or(false, |m| !m.alive) {
                    arrow(c, screen_dir(v.yaw), 7.0 * k, WHITE);
                } else {
                    dot(c, 3.6 * k, col);
                    let d = screen_dir(v.yaw);
                    draw_line(c.x, c.y, c.x + d.x * 7.0 * k, c.y + d.y * 7.0 * k, 1.6 * k, col);
                }
            } else if self.spotted(v.pos, my_team) {
                dot(c, 3.6 * k, Color::new(0.95, 0.22, 0.18, 1.0));
            }
        }
        if let Some(m) = me.filter(|m| m.alive) {
            if carrier == Some(m.id) {
                draw_circle_lines(to(self.body.pos).x, to(self.body.pos).y, 8.5 * k, 2.0 * k, Color::new(1.0, 0.45, 0.15, 1.0));
            }
            arrow(to(self.body.pos), screen_dir(self.yaw), 7.0 * k, Color::new(1.0, 0.95, 0.6, 1.0));
        }

        draw_rectangle_lines(x, y, s, s, 2.0 * k.max(1.0), Color::new(1.0, 1.0, 1.0, 0.3));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cesi_common::map::{build, MAPS};

    #[test]
    fn every_map_has_a_readable_plan() {
        for id in 0..MAPS.len() as u8 {
            let (w, info) = build(id);
            let p = bake(&w, &info);
            assert_eq!(p.rgba.len(), (p.n * p.n * 4) as usize);
            let share = p.floor_px as f32 / (p.n * p.n) as f32;
            assert!((0.25..0.85).contains(&share), "carte {} : {:.0} % de sol", id, share * 100.0);
            assert!(p.n < w.sx, "carte {} : le plan est recadré ({} voxels)", id, p.n);
            // Les sites et les points de départ sont sur du sol praticable (pixel opaque, non blanc).
            let probe = |pos: Vec3| {
                let (x, z) = ((pos.x / VS) as i32, (pos.z / VS) as i32);
                let (i, j) = (p.x0 + p.n - 1 - x, p.z0 + p.n - 1 - z);
                let o = ((j * p.n + i) * 4) as usize;
                p.rgba[o + 3] == 235
            };
            for (s, _) in info.t_spawns.iter().chain(&info.ct_spawns) {
                assert!(probe(*s), "carte {} : départ hors du plan", id);
            }
            // Un site peut porter des caisses, mais l'essentiel est praticable.
            for (lo, hi) in info.sites {
                let cells: Vec<bool> = (0..10)
                    .flat_map(|a| (0..10).map(move |b| (a, b)))
                    .map(|(a, b)| probe(lo + (hi - lo) * vec3(a as f32 + 0.5, 0.0, b as f32 + 0.5) / 10.0))
                    .collect();
                let share = cells.iter().filter(|c| **c).count();
                assert!(share >= 50, "carte {} : site à {} % praticable", id, share);
            }
        }
    }
}
