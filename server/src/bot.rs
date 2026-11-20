//! Intelligence des bots : perception, objectifs, déplacement et tir.

use crate::game::Player;
use crate::nav::Nav;
use cesi_common::glam::Vec3;
use cesi_common::grenades::*;
use cesi_common::map::{in_aabb, MapInfo};
use cesi_common::physics::{self, Body, MoveInput, BASE_SPEED};
use cesi_common::protocol::{BombState, Phase, PlayerId, Team};
use cesi_common::voxel::World;
use cesi_common::weapons::{weapon, Class};
use cesi_common::{angle_diff, dir_from_angles, hdist, perturb, Rng};
use std::f32::consts::PI;

/// Distance à un site en dessous de laquelle un ennemi repéré déclenche le repli des défenseurs.
const ROTATE_DIST: f32 = 13.0;
/// Part du temps passée à surveiller l'accès principal plutôt qu'une autre direction.
const WATCH_CHANCE: f32 = 0.75;
/// Facteur appliqué au temps de réaction d'un bot immobile.
const STILL_REACT: f32 = 0.68;

pub struct BotCtx<'a> {
    pub world: &'a World,
    pub nav: &'a Nav,
    pub info: &'a MapInfo,
    pub players: &'a [Player],
    pub me: usize,
    pub phase: Phase,
    pub bomb: BombState,
    /// Site visé par les attaquants ce round.
    pub t_site: u8,
    /// Position d'un ennemi actuellement vu par un coéquipier.
    pub alert: Option<Vec3>,
    /// Centres des nuages de fumée et des zones en flammes.
    pub smokes: &'a [Vec3],
    pub fires: &'a [Vec3],
    pub dt: f32,
}

pub struct BotOut {
    /// Tir : origine, direction, arme.
    pub shot: Option<(Vec3, Vec3, u8)>,
    pub using: bool,
    pub weapon: u8,
    /// Lancer de grenade : type, origine, direction.
    pub throw: Option<(u8, Vec3, Vec3)>,
}

pub struct Bot {
    pub body: Body,
    pub yaw: f32,
    pub pitch: f32,
    pub skill: f32,
    /// Site défendu (pour les défenseurs).
    pub site: u8,
    path: Vec<Vec3>,
    path_i: usize,
    path_goal: Vec3,
    repath_t: f32,
    spot: Option<Vec3>,
    spot_t: f32,
    target: Option<PlayerId>,
    visible: bool,
    seen_t: f32,
    lost_t: f32,
    last_seen: Option<Vec3>,
    aim_head: bool,
    aim_err: (f32, f32),
    aim_err_t: f32,
    vis_cd: f32,
    fire_cd: f32,
    burst: u32,
    burst_len: u32,
    burst_pause: f32,
    spray: f32,
    mag: u16,
    mag_weapon: u8,
    reload_t: f32,
    stuck_t: f32,
    stuck_n: u32,
    stuck_pos: Vec3,
    strafe: f32,
    strafe_t: f32,
    look_yaw: f32,
    look_t: f32,
    /// Direction d'où l'ennemi devrait arriver, calculée pour la position tenue.
    watch: Option<(Vec3, f32)>,
    /// Temps d'aveuglement restant.
    blind_t: f32,
    nade_cd: f32,
}

impl Bot {
    pub fn new(skill: f32) -> Bot {
        Bot {
            body: Body::new(Vec3::ZERO),
            yaw: 0.0,
            pitch: 0.0,
            skill: skill.clamp(0.0, 1.0),
            site: 0,
            path: Vec::new(),
            path_i: 0,
            path_goal: Vec3::ZERO,
            repath_t: 0.0,
            spot: None,
            spot_t: 0.0,
            target: None,
            visible: false,
            seen_t: 0.0,
            lost_t: 99.0,
            last_seen: None,
            aim_head: false,
            aim_err: (0.0, 0.0),
            aim_err_t: 0.0,
            vis_cd: 0.0,
            fire_cd: 0.0,
            burst: 0,
            burst_len: 4,
            burst_pause: 0.0,
            spray: 0.0,
            mag: 0,
            mag_weapon: 255,
            reload_t: 0.0,
            stuck_t: 0.0,
            stuck_n: 0,
            stuck_pos: Vec3::ZERO,
            strafe: 0.0,
            strafe_t: 0.0,
            look_yaw: 0.0,
            look_t: 0.0,
            watch: None,
            blind_t: 0.0,
            nade_cd: 3.0,
        }
    }

    /// Réinitialise le bot à son point d'apparition.
    pub fn reset(&mut self, pos: Vec3, yaw: f32, site: u8) {
        let skill = self.skill;
        *self = Bot::new(skill);
        self.body = Body::new(pos);
        self.yaw = yaw;
        self.look_yaw = yaw;
        self.site = site;
        self.stuck_pos = pos;
    }

    /// Aveugle le bot (grenade aveuglante).
    pub fn blind(&mut self, duration: f32) {
        self.blind_t = self.blind_t.max(duration * 0.8);
    }

    /// Oublie trajet et repères : à appeler quand la carte change.
    pub fn forget(&mut self) {
        self.path.clear();
        self.path_i = 0;
        self.repath_t = 0.0;
        self.spot = None;
        self.watch = None;
        self.last_seen = None;
        self.target = None;
        self.visible = false;
    }

    pub fn reloading(&self) -> bool {
        self.reload_t > 0.0
    }

    /// Position de l'ennemi que ce bot voit en ce moment (partagée avec son équipe).
    pub fn spotted(&self) -> Option<Vec3> {
        if self.visible {
            self.last_seen
        } else {
            None
        }
    }

    pub fn update(&mut self, c: &BotCtx, rng: &mut Rng) -> BotOut {
        let me = &c.players[c.me];
        let dt = c.dt;
        let wid = if me.primary != 0 { me.primary } else { me.secondary };
        let def = weapon(wid);
        if self.mag_weapon != wid {
            self.mag_weapon = wid;
            self.mag = def.mag;
            self.reload_t = 0.0;
        }
        let mut out = BotOut { shot: None, using: false, weapon: wid, throw: None };
        let eye = self.body.eye();
        let pos = self.body.pos;
        let frozen = c.phase == Phase::Freeze;

        let find = |id: Option<PlayerId>| -> Option<&Player> {
            id.and_then(|id| c.players.iter().find(|p| p.id == id && p.alive && p.team != me.team))
        };
        if find(self.target).is_none() {
            if self.target.is_some() {
                self.last_seen = None;
            }
            self.target = None;
            self.visible = false;
        }

        // --- Perception : ennemi visible le plus proche ---
        self.vis_cd -= dt;
        self.blind_t -= dt;
        if self.blind_t > 0.0 {
            // Aveuglé : ne voit plus personne.
            self.visible = false;
            self.vis_cd = 0.1;
        } else if self.vis_cd <= 0.0 {
            self.vis_cd = 0.08 + rng.f32() * 0.04;
            let fwd = dir_from_angles(self.yaw, self.pitch);
            let mut best: Option<(PlayerId, f32)> = None;
            for p in c.players {
                if !p.alive || p.team == me.team {
                    continue;
                }
                let chest = p.pos + Vec3::Y * if p.crouch { 0.75 } else { 1.2 };
                let d = chest - eye;
                let dist = d.length();
                if !(0.01..=80.0).contains(&dist) {
                    continue;
                }
                let dn = d / dist;
                if dn.dot(fwd) < 0.3 && dist > 4.0 {
                    continue;
                }
                // La fumée bloque la vue.
                if c.smokes.iter().any(|s| segment_hits_sphere(eye, chest, *s + Vec3::Y, SMOKE_RADIUS * 0.9)) {
                    continue;
                }
                let mut seen = c.world.raycast(eye, dn, dist).is_none();
                if !seen {
                    let d2 = p.pos + Vec3::Y * if p.crouch { 1.15 } else { 1.6 } - eye;
                    let l2 = d2.length().max(0.01);
                    seen = c.world.raycast(eye, d2 / l2, l2).is_none();
                }
                if seen && best.map_or(true, |b| dist < b.1) {
                    best = Some((p.id, dist));
                }
            }
            match best {
                Some((id, _)) => {
                    if self.target != Some(id) || (!self.visible && self.lost_t > 1.0) {
                        self.seen_t = 0.0;
                        self.aim_head = rng.chance(0.15 + 0.5 * self.skill);
                    }
                    self.target = Some(id);
                    self.visible = true;
                    self.lost_t = 0.0;
                }
                None => self.visible = false,
            }
        }
        let tgt = find(self.target);
        if self.visible {
            self.seen_t += dt;
            if let Some(t) = tgt {
                self.last_seen = Some(t.pos);
            }
        } else {
            self.lost_t += dt;
            if self.lost_t > 6.0 {
                self.target = None;
                self.last_seen = None;
            }
        }

        // --- Objectif ---
        let mut goal: Option<Vec3>;
        let mut hold = false;
        let mut duty = false;
        self.spot_t -= dt;
        match (me.team, c.bomb) {
            (Team::T, BombState::Dropped(b)) => goal = Some(b),
            (Team::CT, BombState::Planted { pos: b, time_left, .. }) => {
                goal = Some(b);
                duty = true;
                if hdist(pos, b) < 1.2 && (pos.y - b.y).abs() < 1.5 {
                    hold = true;
                    let need = if me.kit { 5.0 } else { 10.0 } * (1.0 - me.progress);
                    if !self.visible || time_left < need + 1.5 {
                        out.using = true;
                    }
                }
            }
            (team, bomb) => {
                // Les défenseurs se replient vers le site où un coéquipier voit l'ennemi.
                if let (Team::CT, false, Some(a)) = (team, self.visible, c.alert) {
                    let centre = |i: usize| (c.info.sites[i].0 + c.info.sites[i].1) * 0.5;
                    let near = if hdist(a, centre(0)) < hdist(a, centre(1)) { 0 } else { 1 };
                    if hdist(a, centre(near)) < ROTATE_DIST {
                        self.site = near as u8;
                    }
                }
                let site = match (team, bomb) {
                    (Team::T, BombState::Planted { site, .. }) => site,
                    (Team::T, _) => c.t_site,
                    _ => self.site,
                } as usize;
                let carrier = matches!(bomb, BombState::Carried(id) if id == me.id);
                let (smin, smax) = c.info.sites[site.min(1)];
                let valid = self.spot.is_some_and(|s| in_aabb(s, smin, smax));
                if !valid || (self.spot_t <= 0.0 && !carrier) {
                    self.spot = c.nav.random_in(smin, smax, rng);
                    self.spot_t = rng.range(6.0, 14.0);
                }
                goal = self.spot;
                duty = carrier;
                if let Some(s) = self.spot {
                    if hdist(pos, s) < 0.6 {
                        hold = true;
                        if carrier && !self.visible && c.info.site_of(pos).is_some() {
                            out.using = true;
                        }
                    }
                }
            }
        }
        // Traque du dernier ennemi aperçu.
        // Les défenseurs tiennent leur position au lieu de poursuivre.
        if !duty && !self.visible && self.lost_t > 0.5 && me.team == Team::T {
            if let Some(ls) = self.last_seen {
                goal = Some(ls);
                hold = false;
                if hdist(pos, ls) < 1.0 {
                    self.last_seen = None;
                }
            }
        }

        // Dans les flammes : sortir au plus vite.
        if let Some(f) = c.fires.iter().find(|f| hdist(pos, **f) < FIRE_RADIUS + 0.7 && (pos.y - f.y).abs() < 2.0) {
            let mut away = Vec3::new(pos.x - f.x, 0.0, pos.z - f.z);
            if away.length_squared() < 0.01 {
                away = dir_from_angles(self.yaw + PI, 0.0);
            }
            goal = Some(pos + away.normalize() * 5.0);
            hold = false;
            out.using = false;
        }

        // Grenade vers le dernier endroit où un ennemi a été aperçu.
        self.nade_cd -= dt;
        let live = matches!(c.phase, Phase::Live);
        if live && !self.visible && !out.using && self.blind_t <= 0.0 && self.nade_cd <= 0.0 && (0.7..3.5).contains(&self.lost_t) {
            if let Some(ls) = self.last_seen {
                let kind = [FRAG, FIRE, FLASH].into_iter().find(|k| me.nades[*k as usize] > 0);
                if let (Some(kind), true) = (kind, (7.0..24.0).contains(&hdist(pos, ls))) {
                    let dir = ballistic(eye, ls + Vec3::Y * 0.3, THROW_SPEED, NADE_GRAVITY);
                    if c.world.raycast(eye, dir, 3.5).is_none() {
                        out.throw = Some((kind, eye, dir));
                        self.nade_cd = rng.range(5.0, 9.0);
                    } else {
                        self.nade_cd = 1.0;
                    }
                }
            }
        }

        // --- Suivi du chemin ---
        let mut mv: Option<Vec3> = None;
        if let (Some(g), false, false) = (goal, hold, frozen) {
            self.repath_t -= dt;
            let need = self.path_i >= self.path.len() || hdist(g, self.path_goal) > 1.0;
            if need && self.repath_t <= 0.0 {
                self.repath_t = 0.4 + rng.f32() * 0.4;
                match c.nav.path(pos, g) {
                    Some(p) => {
                        self.path_i = if p.len() > 1 { 1 } else { 0 };
                        self.path = p;
                        self.path_goal = g;
                    }
                    None => {
                        self.path.clear();
                        self.spot = None;
                    }
                }
            }
            while self.path_i < self.path.len() {
                let n = self.path[self.path_i];
                if hdist(pos, n) < 0.3 {
                    self.path_i += 1;
                } else {
                    mv = Some(n - pos);
                    break;
                }
            }
        }

        // --- Déplacement ---
        let fighting = self.visible && tgt.is_some();
        // Au-delà de la portée utile de l'arme, le bot continue d'avancer en tirant.
        let engage = match def.class {
            Class::Pistol => 16.0,
            Class::Smg => 20.0,
            Class::Shotgun => 9.0,
            Class::Sniper => 90.0,
            _ => 38.0,
        };
        let in_range = tgt.is_some_and(|t| t.pos.distance(pos) < engage);
        let mut input = MoveInput::default();
        if frozen || out.using {
            // immobile
        } else if fighting && (in_range || mv.is_none()) {
            self.strafe_t -= dt;
            if self.strafe_t <= 0.0 {
                self.strafe_t = rng.range(0.35, 0.9);
                self.strafe = if def.class == Class::Sniper || rng.chance(0.3) {
                    0.0
                } else if rng.chance(0.5) {
                    1.0
                } else {
                    -1.0
                };
            }
            input.yaw = self.yaw;
            input.side = self.strafe;
        } else if let Some(d) = mv {
            input.yaw = d.x.atan2(d.z);
            input.fwd = 1.0;
        }

        if input.fwd > 0.0 {
            self.stuck_t += dt;
            if self.stuck_t > 0.5 {
                let moved = hdist(pos, self.stuck_pos);
                self.stuck_pos = pos;
                self.stuck_t = 0.0;
                if moved < 0.25 {
                    self.stuck_n += 1;
                    if self.stuck_n == 1 {
                        input.jump = true;
                    } else {
                        self.path.clear();
                        self.path_i = 0;
                        self.repath_t = 0.0;
                        if self.stuck_n > 3 {
                            self.spot = None;
                            self.stuck_n = 0;
                        }
                    }
                } else {
                    self.stuck_n = 0;
                }
            }
        } else {
            self.stuck_t = 0.0;
            self.stuck_pos = pos;
        }

        // --- Visée ---
        self.aim_err_t -= dt;
        if self.aim_err_t <= 0.0 {
            self.aim_err_t = rng.range(0.25, 0.55);
            let m = ((1.0 - self.skill) * 3.5 + 0.5).to_radians();
            self.aim_err = (rng.range(-m, m), rng.range(-m, m) * 0.6);
        }
        let aim_at = |p: Vec3| -> (f32, f32) {
            let d = p - eye;
            (d.x.atan2(d.z), (d.y / d.length().max(0.01)).clamp(-1.0, 1.0).asin())
        };
        let (ty, tp) = if let (true, Some(t)) = (self.visible, tgt) {
            let h = match (self.aim_head, t.crouch) {
                (true, false) => 1.6,
                (true, true) => 1.12,
                (false, false) => 1.15,
                (false, true) => 0.7,
            };
            let (y, p) = aim_at(t.pos + Vec3::Y * h);
            // La visée se stabilise tant que la cible reste en vue.
            let settle = (1.0 - self.seen_t * 0.35).max(0.35);
            (y + self.aim_err.0 * settle, p + self.aim_err.1 * settle)
        } else if let (true, Some(ls)) = (self.lost_t < if mv.is_some() { 0.8 } else { 3.0 }, self.last_seen) {
            aim_at(ls + Vec3::Y * 1.2)
        } else if mv.is_some() && !self.path.is_empty() {
            let ahead = self.path[(self.path_i + 2).min(self.path.len() - 1)];
            let d = ahead - pos;
            if d.x.abs() + d.z.abs() > 0.2 {
                (d.x.atan2(d.z), 0.0)
            } else {
                (self.yaw, 0.0)
            }
        } else {
            // Position tenue : surveiller l'accès par lequel l'ennemi doit arriver.
            if self.watch.map_or(true, |w| hdist(w.0, pos) > 1.5) {
                let from = if me.team == Team::CT { c.info.t_spawns[0].0 } else { c.info.ct_spawns[0].0 };
                let mut yaw = self.look_yaw;
                if let Some(path) = c.nav.path(pos, from) {
                    // Point le plus lointain du trajet encore visible d'ici.
                    for n in path.iter().take(60).skip(3) {
                        let d = *n + Vec3::Y * 1.3 - eye;
                        let l = d.length().max(0.01);
                        if c.world.raycast(eye, d / l, l).is_some() {
                            break;
                        }
                        yaw = d.x.atan2(d.z);
                    }
                }
                self.watch = Some((pos, yaw));
                self.look_t = 0.0;
            }
            self.look_t -= dt;
            if let (true, Some(w), true) = (self.look_t <= 0.0, self.watch, rng.chance(WATCH_CHANCE)) {
                self.look_t = rng.range(1.5, 4.0);
                self.look_yaw = w.1 + rng.range(-0.2, 0.2);
            }
            if self.look_t <= 0.0 {
                self.look_t = rng.range(1.5, 4.0);
                let mut opts: Vec<(f32, f32)> = (0..8)
                    .map(|i| {
                        let a = i as f32 * PI / 4.0 + rng.range(-0.2, 0.2);
                        let d = c.world.raycast(eye, dir_from_angles(a, 0.0), 60.0).map_or(60.0, |h| h.dist);
                        (d, a)
                    })
                    .collect();
                opts.sort_by(|a, b| b.0.total_cmp(&a.0));
                self.look_yaw = opts[rng.below(3)].1;
            }
            (self.look_yaw, 0.0)
        };
        let k = ((4.0 + 10.0 * self.skill) * dt).min(1.0);
        self.yaw += angle_diff(self.yaw, ty) * k;
        self.pitch += (tp - self.pitch) * k;

        // --- Tir et rechargement ---
        self.fire_cd -= dt;
        self.burst_pause -= dt;
        if self.fire_cd < -0.15 {
            self.spray = (self.spray - dt * 8.0).max(0.0);
        }
        if self.reload_t > 0.0 {
            self.reload_t -= dt;
            if self.reload_t <= 0.0 {
                self.mag = def.mag;
            }
        } else if self.mag == 0 || (!self.visible && self.lost_t > 2.0 && self.mag < def.mag / 3) {
            self.reload_t = def.reload;
        }
        let can_shoot = matches!(c.phase, Phase::Live | Phase::RoundEnd);
        if let (true, Some(t)) = (fighting && can_shoot && !out.using, tgt) {
            // Un bot immobile, déjà en joue, réagit plus vite qu'un bot en mouvement.
            let still = if mv.is_none() && self.body.hspeed() < 1.0 { STILL_REACT } else { 1.0 };
            let react = (0.2 + (1.0 - self.skill) * 0.45) * still;
            if self.seen_t >= react && self.fire_cd <= 0.0 && self.burst_pause <= 0.0 && self.reload_t <= 0.0 && self.mag > 0 {
                let view = dir_from_angles(self.yaw, self.pitch);
                let d = t.pos + Vec3::Y * if t.crouch { 0.7 } else { 1.1 } - eye;
                let dist = d.length().max(0.01);
                let off = view.dot(d / dist).clamp(-1.0, 1.0).acos();
                let tol = (0.5 / dist).atan() + (1.5 + (1.0 - self.skill) * 3.0).to_radians();
                if off < tol {
                    let moving = (self.body.hspeed() / (BASE_SPEED * def.speed)).min(1.0);
                    let spread = def.spread
                        + (1.0 - self.skill) * 1.2
                        + self.spray.min(10.0) * def.recoil_v * 0.35
                        + def.spread_move * moving * 0.6;
                    out.shot = Some((eye, perturb(view, spread.to_radians(), rng), wid));
                    self.mag -= 1;
                    self.spray += 1.0;
                    let interval = 60.0 / def.rpm;
                    if def.auto {
                        self.fire_cd = interval;
                        self.burst += 1;
                        if self.burst >= self.burst_len {
                            self.burst = 0;
                            self.burst_len = 3 + rng.below(6) as u32;
                            self.burst_pause = rng.range(0.25, 0.6);
                        }
                    } else {
                        self.fire_cd = interval.max(0.22 + rng.f32() * 0.25 + (1.0 - self.skill) * 0.2);
                    }
                }
            }
        }

        physics::step(c.world, &mut self.body, &input, BASE_SPEED * def.speed, dt);
        out
    }
}
