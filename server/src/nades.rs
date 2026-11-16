//! Grenades côté serveur : trajectoire avec rebonds, explosions, fumée, feu
//! et aveuglement.

use crate::game::Server;
use cesi_common::glam::Vec3;
use cesi_common::grenades::*;
use cesi_common::physics::eye_height;
use cesi_common::protocol::{Phase, PlayerId, ServerMsg, Team};
use cesi_common::{dir_from_angles, hdist};

/// Rayon de collision d'une grenade.
const RADIUS: f32 = 0.09;
const BOUNCE: f32 = 0.42;
const FRICTION: f32 = 0.78;
/// Intervalle entre deux brûlures dans les flammes (s).
const BURN_TICK: f32 = 0.25;

/// Grenade en vol.
pub(crate) struct Nade {
    pub id: u16,
    pub kind: u8,
    pub pos: Vec3,
    vel: Vec3,
    age: f32,
    rest: bool,
    owner: PlayerId,
    team: Team,
}

/// Fumée ou flammes en cours.
pub(crate) struct Area {
    pub id: u16,
    pub kind: u8,
    pub pos: Vec3,
    pub time_left: f32,
    owner: PlayerId,
    team: Team,
    tick: f32,
}

impl Server {
    /// Le joueur `i` lance une grenade de type `kind`.
    pub(crate) fn throw_nade(&mut self, i: usize, kind: u8, origin: Vec3, dir: Vec3, strong: bool) {
        if !matches!(self.phase, Phase::Live | Phase::RoundEnd) || kind as usize >= GRENADES.len() {
            return;
        }
        let p = &mut self.players[i];
        if !p.alive || p.nades[kind as usize] == 0 || !dir.is_finite() || !origin.is_finite() || dir.length_squared() < 0.5 {
            return;
        }
        p.nades[kind as usize] -= 1;
        let dir = dir.normalize();
        let eye = p.pos + Vec3::Y * eye_height(if p.crouch { 1.0 } else { 0.0 });
        let origin = if origin.distance(eye) > 1.5 { eye } else { origin };
        // Départ un peu devant la main, sauf si un mur est collé au joueur.
        let reach = self.world.raycast(origin, dir, 0.6).map_or(0.45, |h| (h.dist - RADIUS * 2.0).max(0.0));
        let (owner, team) = (p.id, p.team);
        self.next_nade = self.next_nade.wrapping_add(1);
        self.nades.push(Nade {
            id: self.next_nade,
            kind,
            pos: origin + dir * reach,
            vel: dir * if strong { THROW_SPEED } else { LOB_SPEED },
            age: 0.0,
            rest: false,
            owner,
            team,
        });
    }

    pub(crate) fn update_nades(&mut self, dt: f32) {
        let mut done = Vec::new();
        for (idx, n) in self.nades.iter_mut().enumerate() {
            n.age += dt;
            let mut ground_hit = false;
            if !n.rest {
                n.vel.y -= NADE_GRAVITY * dt;
                let mut left = dt;
                for _ in 0..3 {
                    let step = n.vel * left;
                    let len = step.length();
                    if len < 1e-5 {
                        break;
                    }
                    let d = step / len;
                    let Some(hit) = self.world.raycast(n.pos, d, len + RADIUS) else {
                        n.pos += step;
                        break;
                    };
                    let travel = (hit.dist - RADIUS).max(0.0);
                    n.pos += d * travel;
                    let normal = hit.normal.as_vec3();
                    if normal == Vec3::ZERO {
                        // Coincée dans un bloc : elle reste où elle est.
                        n.rest = true;
                        break;
                    }
                    let vn = normal * n.vel.dot(normal);
                    n.vel = (n.vel - vn) * FRICTION - vn * BOUNCE;
                    left *= 1.0 - travel / len;
                    if normal.y > 0.5 {
                        ground_hit = true;
                        if n.vel.length() < 1.3 {
                            n.rest = true;
                            n.vel = Vec3::ZERO;
                            break;
                        }
                    }
                }
                if n.pos.y < -5.0 {
                    n.rest = true;
                }
            }
            let explode = match n.kind {
                FRAG => n.age >= FRAG_FUSE,
                FLASH => n.age >= FLASH_FUSE,
                // Le fumigène se déclenche une fois immobile, l'incendiaire au premier contact avec le sol.
                SMOKE => (n.rest && n.age > 0.6) || n.age > 3.5,
                _ => ground_hit || n.rest || n.age > 2.5,
            };
            if explode {
                done.push(idx);
            }
        }
        for idx in done.into_iter().rev() {
            let n = self.nades.remove(idx);
            self.detonate(n);
        }
        self.update_areas(dt);
    }

    fn area(&mut self, kind: u8, pos: Vec3, time: f32, n: &Nade) {
        self.next_nade = self.next_nade.wrapping_add(1);
        self.areas.push(Area { id: self.next_nade, kind, pos, time_left: time, owner: n.owner, team: n.team, tick: 0.0 });
    }

    fn detonate(&mut self, n: Nade) {
        let pos = n.pos;
        self.broadcast(ServerMsg::Detonate { kind: n.kind, pos });
        let wid = GRENADE_BASE + n.kind;
        match n.kind {
            FRAG => {
                let centre = pos + Vec3::Y * 0.2;
                for i in 0..self.players.len() {
                    let p = &self.players[i];
                    // Blesse les ennemis et le lanceur, pas ses coéquipiers.
                    if !p.alive || (p.team == n.team && p.id != n.owner) {
                        continue;
                    }
                    let chest = p.pos + Vec3::Y * if p.crouch { 0.6 } else { 1.0 };
                    let d = chest - centre;
                    let dist = d.length();
                    if dist >= FRAG_RADIUS {
                        continue;
                    }
                    // Un mur entre l'explosion et le joueur le protège.
                    if dist > 0.3 && self.world.raycast(centre, d / dist, dist - 0.2).is_some() {
                        continue;
                    }
                    let dmg = FRAG_DAMAGE * (1.0 - dist / FRAG_RADIUS).powf(1.3);
                    let attacker = self.idx(n.owner).filter(|a| *a != i);
                    self.damage(i, attacker, dmg, 0.6, 1, wid, pos);
                }
            }
            FLASH => {
                let centre = pos + Vec3::Y * 0.15;
                for i in 0..self.players.len() {
                    let p = &self.players[i];
                    if !p.alive {
                        continue;
                    }
                    let eye = p.pos + Vec3::Y * eye_height(if p.crouch { 1.0 } else { 0.0 });
                    let d = centre - eye;
                    let dist = d.length().max(0.01);
                    if dist > FLASH_RANGE || self.world.raycast(eye, d / dist, dist - 0.15).is_some() {
                        continue;
                    }
                    // Plus fort de près et quand on regarde l'explosion.
                    let facing = dir_from_angles(p.yaw, p.pitch).dot(d / dist);
                    let look = 0.3 + 0.7 * ((facing + 0.3) / 1.3).clamp(0.0, 1.0);
                    let duration = FLASH_TIME * (1.0 - dist / FLASH_RANGE).powf(0.7) * look;
                    if duration < 0.4 {
                        continue;
                    }
                    let (pid, bot) = (p.id, p.is_bot);
                    if bot {
                        if let Some(b) = self.players[i].bot.as_mut() {
                            b.blind(duration);
                        }
                    } else {
                        self.send_to(pid, ServerMsg::Flashed { duration });
                    }
                }
            }
            SMOKE => {
                // La fumée étouffe les flammes à proximité.
                self.areas.retain(|a| a.kind != FIRE || a.pos.distance(pos) > SMOKE_RADIUS + 0.5);
                self.area(SMOKE, pos, SMOKE_TIME, &n);
            }
            _ => {
                // Pas de feu dans la fumée, ni si la grenade a éclaté trop haut.
                let smothered = self.areas.iter().any(|a| a.kind == SMOKE && a.pos.distance(pos) < SMOKE_RADIUS);
                let ground = self.world.raycast(pos + Vec3::Y * 0.1, Vec3::NEG_Y, 3.0).map(|h| pos + Vec3::Y * (0.1 - h.dist));
                if let (false, Some(g)) = (smothered, ground) {
                    self.area(FIRE, g, FIRE_TIME, &n);
                }
            }
        }
    }

    fn update_areas(&mut self, dt: f32) {
        let mut burns = Vec::new();
        for a in &mut self.areas {
            a.time_left -= dt;
            if a.kind != FIRE {
                continue;
            }
            a.tick += dt;
            if a.tick >= BURN_TICK {
                a.tick -= BURN_TICK;
                for (i, p) in self.players.iter().enumerate() {
                    let inside = hdist(p.pos, a.pos) < FIRE_RADIUS && (p.pos.y - a.pos.y).abs() < 1.6;
                    if p.alive && inside && (p.team != a.team || p.id == a.owner) {
                        burns.push((i, a.owner, a.pos));
                    }
                }
            }
        }
        self.areas.retain(|a| a.time_left > 0.0);
        for (i, owner, pos) in burns {
            if self.players[i].alive {
                let attacker = self.idx(owner).filter(|a| *a != i);
                // Les flammes ignorent le gilet (zone « jambes », pénétration totale).
                self.damage(i, attacker, FIRE_DPS * BURN_TICK, 1.0, 2, GRENADE_BASE + FIRE, pos);
            }
        }
    }
}
