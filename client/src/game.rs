//! Partie en cours côté client : entrées, déplacement prédit localement,
//! armes, interpolation des autres joueurs, effets et rendu de la scène.

use crate::audio::Audio;
use crate::netclient::NetClient;
use crate::render::{self, Batch, PlayerView, Puppet, WorldRender};
use crate::viewmodel::{self, ViewAnim};
use crate::settings::{Action, Settings};
use crate::ui;
use cesi_common::map::MapInfo;
use cesi_common::physics::{self, eye_height, Body, MoveInput, BASE_SPEED};
use cesi_common::protocol::*;
use cesi_common::voxel::{block_color, World};
use cesi_common::weapons::{weapon, Class, WeaponDef, KNIFE, KNIFE_COOLDOWN, KNIFE_HEAVY_COOLDOWN, KNIFE_RANGE};
use cesi_common::{angle_diff, dir_from_angles, hdist, perturb, Rng};
use macroquad::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

/// Retard d'affichage des autres joueurs (interpolation entre deux photos).
const INTERP: f64 = 0.1;

#[derive(PartialEq, Clone, Copy)]
pub enum Overlay {
    None,
    Pause,
    Settings,
    Buy,
    Chat,
}

pub enum Status {
    Connecting,
    Connected,
    Failed(String),
}

#[derive(Clone, Copy, Default)]
pub struct Ammo {
    pub id: u8,
    pub mag: u16,
    pub reserve: u16,
}

pub struct Particle {
    pos: Vec3,
    vel: Vec3,
    life: f32,
    size: f32,
    color: Color,
}

pub struct Tracer {
    from: Vec3,
    to: Vec3,
    life: f32,
}

pub struct Corpse {
    pos: Vec3,
    yaw: f32,
    team: Team,
    t0: f64,
    weapon: u8,
}

pub struct KillLine {
    pub killer: String,
    pub kteam: Option<Team>,
    pub victim: String,
    pub vteam: Team,
    pub weapon: u8,
    pub headshot: bool,
    pub time: f64,
    pub mine: bool,
}

pub struct Game {
    pub net: NetClient,
    pub world: Rc<World>,
    pub info: Rc<MapInfo>,
    pub status: Status,
    pub my_id: Option<PlayerId>,
    pub cfg: MatchCfg,
    pub now: f64,
    pub start: f64,
    pub last_recv: f64,
    pub snaps: VecDeque<Snapshot>,
    pub time_offset: Option<f64>,
    pub send_acc: f32,
    pub send_n: u32,
    // Joueur local
    pub body: Body,
    pub yaw: f32,
    pub pitch: f32,
    pub recoil: Vec2,
    pub spray: f32,
    pub last_shot: f64,
    pub slot: usize,
    pub ammo: [Ammo; 2],
    pub refill: bool,
    pub fire_cd: f32,
    pub reload_t: f32,
    pub draw_t: f32,
    pub zoom: u8,
    pub rescope: (f32, u8),
    pub using: bool,
    pub step_smooth: f32,
    pub muzzle_t: f32,
    /// Temps écoulé depuis le dernier coup de couteau, et s'il était puissant.
    pub knife_t: f32,
    pub knife_heavy: bool,
    // Souris
    pub grab_frames: u32,
    pub last_mouse: Vec2,
    // Monde
    pub views: Vec<PlayerView>,
    pub steps: HashMap<PlayerId, f32>,
    pub alive_prev: HashMap<PlayerId, bool>,
    pub particles: Vec<Particle>,
    pub tracers: Vec<Tracer>,
    pub corpses: Vec<Corpse>,
    pub spectate: Option<PlayerId>,
    pub last_beep: f64,
    // Caméra de la frame
    pub cam_pos: Vec3,
    pub cam_yaw: f32,
    pub cam_pitch: f32,
    pub cam_fovy: f32,
    // Interface
    pub overlay: Overlay,
    pub overlay_since: f64,
    /// Diagnostic : 1 = tir automatique, 2 = tableau des scores, 4 = couteau en main,
    /// 8 = coup puissant automatique, 16 = animation du couteau figée (32 = coup puissant).
    pub debug: u8,
    /// Diagnostic : arme affichée de force et progression de rechargement figée.
    pub debug_weapon: u8,
    pub debug_f: f32,
    pub va: ViewAnim,
    pub puppets: HashMap<PlayerId, Puppet>,
    pub reload_stage: u8,
    pub chat_input: String,
    pub killfeed: Vec<KillLine>,
    pub chat: Vec<(String, Option<Team>, String, f64)>,
    pub center: Option<(String, f64, Color)>,
    pub hitmarker_t: f32,
    pub hit_kill: bool,
    pub hurt_t: f32,
    pub flash_t: f32,
    pub rng: Rng,
}

fn fovy_from(fov: f32) -> f32 {
    2.0 * ((fov.to_radians() / 2.0).tan() * 0.75).atan()
}

impl Game {
    pub fn connect(addr: &str, name: &str, world: Rc<World>, info: Rc<MapInfo>) -> Result<Game, String> {
        let net = NetClient::connect(addr, name)?;
        let now = get_time();
        let cam = info.t_spawns[0].0 + vec3(0.0, 9.0, -6.0);
        Ok(Game {
            net,
            world,
            info,
            status: Status::Connecting,
            my_id: None,
            cfg: MatchCfg::default(),
            now,
            start: now,
            last_recv: now,
            snaps: VecDeque::new(),
            time_offset: None,
            send_acc: 0.0,
            send_n: 0,
            body: Body::new(cam),
            yaw: 0.0,
            pitch: 0.0,
            recoil: Vec2::ZERO,
            spray: 0.0,
            last_shot: 0.0,
            slot: 1,
            ammo: [Ammo::default(); 2],
            refill: true,
            fire_cd: 0.0,
            reload_t: 0.0,
            draw_t: 0.0,
            zoom: 0,
            rescope: (0.0, 0),
            using: false,
            step_smooth: 0.0,
            muzzle_t: 0.0,
            knife_t: 9.0,
            knife_heavy: false,
            grab_frames: 0,
            last_mouse: Vec2::ZERO,
            views: Vec::new(),
            steps: HashMap::new(),
            alive_prev: HashMap::new(),
            particles: Vec::new(),
            tracers: Vec::new(),
            corpses: Vec::new(),
            spectate: None,
            last_beep: 0.0,
            cam_pos: cam,
            cam_yaw: 0.0,
            cam_pitch: -0.4,
            cam_fovy: fovy_from(90.0),
            overlay: Overlay::None,
            overlay_since: 0.0,
            debug: 0,
            debug_weapon: 0,
            debug_f: 0.0,
            va: ViewAnim::default(),
            puppets: HashMap::new(),
            reload_stage: 0,
            chat_input: String::new(),
            killfeed: Vec::new(),
            chat: Vec::new(),
            center: None,
            hitmarker_t: 0.0,
            hit_kill: false,
            hurt_t: 0.0,
            flash_t: 0.0,
            rng: Rng::from_time(),
        })
    }

    pub fn snap(&self) -> Option<&Snapshot> {
        self.snaps.back()
    }

    pub fn me(&self) -> Option<&PlayerSnap> {
        let id = self.my_id?;
        self.snaps.back()?.players.iter().find(|p| p.id == id)
    }

    pub fn player(&self, id: PlayerId) -> Option<&PlayerSnap> {
        self.snaps.back()?.players.iter().find(|p| p.id == id)
    }

    pub fn weapon_id(&self) -> u8 {
        if self.slot < 2 {
            self.ammo[self.slot].id
        } else {
            KNIFE
        }
    }

    fn render_time(&self) -> f64 {
        self.now + self.time_offset.unwrap_or(0.0) - INTERP
    }

    /// Angles de vue, recul compris.
    pub fn view_angles(&self) -> (f32, f32) {
        (self.yaw + self.recoil.y.to_radians(), (self.pitch + self.recoil.x.to_radians()).clamp(-1.55, 1.55))
    }

    pub fn want_grab(&self) -> bool {
        self.overlay == Overlay::None && matches!(self.status, Status::Connected)
    }

    /// Achats possibles : pendant la préparation et le début du round.
    pub fn can_buy(&self) -> bool {
        self.buy_time_left().is_some() && self.in_buy_zone()
    }

    /// Secondes restantes pour acheter, ou `None` si les achats sont fermés.
    pub fn buy_time_left(&self) -> Option<f32> {
        let s = self.snap()?;
        if !self.me()?.alive {
            return None;
        }
        let left = match s.phase {
            Phase::Freeze => s.time_left + self.cfg.buy_time,
            Phase::Live if !matches!(s.bomb, BombState::Planted { .. }) => s.time_left - (self.cfg.round_time - self.cfg.buy_time),
            _ => return None,
        };
        (left > 0.0).then_some(left)
    }

    /// Vrai si le joueur se trouve dans la zone d'achat de son équipe.
    pub fn in_buy_zone(&self) -> bool {
        self.me().is_some_and(|m| self.info.in_buy_zone(m.team, self.body.pos))
    }

    pub fn has_bomb(&self) -> bool {
        matches!((self.snap().map(|s| s.bomb), self.my_id), (Some(BombState::Carried(c)), Some(id)) if c == id)
    }

    /// Dispersion actuelle du tir, en degrés.
    pub fn spread(&self, def: &WeaponDef) -> f32 {
        if def.class == Class::Knife {
            return 0.0;
        }
        let frac = (self.body.hspeed() / (BASE_SPEED * def.speed)).min(1.0);
        let mut s = def.spread;
        if def.scope && self.zoom == 0 {
            s += 3.0;
        }
        s += def.spread_move * ((frac - 0.3) / 0.7).max(0.0);
        if !self.body.on_ground {
            s += def.spread_move * 1.5 + 2.0;
        }
        if self.body.crouched {
            s *= 0.75;
        }
        s + self.spray.min(10.0) * def.recoil_v * 0.08
    }

    fn switch_to(&mut self, slot: usize) {
        if slot != self.slot {
            self.slot = slot;
            self.draw_t = 0.4;
            self.reload_t = 0.0;
            self.zoom = 0;
            self.rescope = (0.0, 0);
        }
    }

    fn sync_inventory(&mut self, me: &PlayerSnap) {
        let ids = [me.primary, me.secondary];
        for s in 0..2 {
            if self.ammo[s].id != ids[s] {
                let d = weapon(ids[s]);
                self.ammo[s] = Ammo { id: ids[s], mag: d.mag, reserve: d.reserve };
                if ids[s] != 0 && !self.refill {
                    self.switch_to(s);
                }
            }
        }
        if self.refill {
            for s in 0..2 {
                let d = weapon(ids[s]);
                self.ammo[s] = Ammo { id: ids[s], mag: d.mag, reserve: d.reserve };
            }
            self.slot = if ids[0] != 0 { 0 } else { 1 };
            self.draw_t = 0.0;
            self.refill = false;
        }
        if self.slot < 2 && self.ammo[self.slot].id == 0 {
            self.slot = if self.ammo[1].id != 0 { 1 } else { 2 };
        }
    }

    fn name_team(&self, id: PlayerId) -> (String, Team) {
        self.player(id).map_or(("?".into(), Team::T), |p| (p.name.clone(), p.team))
    }

    fn burst(&mut self, pos: Vec3, dir: Vec3, n: usize, speed: f32, size: f32, color: Color) {
        for _ in 0..n {
            let v = dir * speed * self.rng.range(0.3, 1.0)
                + vec3(self.rng.range(-1.0, 1.0), self.rng.range(-0.2, 1.2), self.rng.range(-1.0, 1.0)) * speed * 0.5;
            let k = self.rng.range(0.75, 1.1);
            self.particles.push(Particle {
                pos,
                vel: v,
                life: self.rng.range(0.25, 0.6),
                size: size * self.rng.range(0.6, 1.2),
                color: Color::new(color.r * k, color.g * k, color.b * k, 1.0),
            });
        }
        if self.particles.len() > 600 {
            let extra = self.particles.len() - 600;
            self.particles.drain(0..extra);
        }
    }

    /// Éclats à l'impact d'une balle sur le décor.
    fn impact(&mut self, end: Vec3, dir: Vec3) {
        let block = self.world.raycast(end - dir * 0.3, dir, 0.8).map_or(3, |h| h.block);
        let c = block_color(block);
        self.burst(end - dir * 0.05, -dir, 5, 2.4, 0.05, Color::from_rgba(c[0], c[1], c[2], 255));
    }

    fn on_msg(&mut self, m: ServerMsg, audio: &Audio) {
        match m {
            ServerMsg::Welcome { id, cfg } => {
                self.my_id = Some(id);
                self.cfg = cfg;
                self.status = Status::Connected;
            }
            ServerMsg::Reject(r) => self.status = Status::Failed(r),
            ServerMsg::Spawn { pos, yaw } => {
                self.body = Body::new(pos);
                self.yaw = yaw;
                self.pitch = 0.0;
                self.recoil = Vec2::ZERO;
                self.spray = 0.0;
                self.reload_t = 0.0;
                self.zoom = 0;
                self.step_smooth = 0.0;
                self.refill = true;
                self.spectate = None;
            }
            ServerMsg::Shot { shooter, weapon: wid, origin, end, hit_player } => {
                self.puppets.entry(shooter).or_default().shot = 1.0;
                audio.play_at(Audio::shot_name(weapon(wid).class), self.cam_pos, origin, 75.0, 0.9);
                if wid != KNIFE {
                    let d = (end - origin).normalize_or_zero();
                    self.tracers.push(Tracer { from: origin + d * 0.9 - Vec3::Y * 0.1, to: end, life: 0.07 });
                    if hit_player {
                        self.burst(end, -d, 6, 2.0, 0.05, Color::new(0.7, 0.05, 0.05, 1.0));
                    } else {
                        self.impact(end, d);
                    }
                }
            }
            ServerMsg::Hit { headshot, killed, pos, .. } => {
                self.hitmarker_t = 0.25;
                self.hit_kill = killed;
                audio.play(if headshot { "headshot" } else { "hit" }, 0.6);
                let d = (pos - self.cam_pos).normalize_or_zero();
                self.burst(pos, -d, 8, 2.2, 0.05, Color::new(0.7, 0.05, 0.05, 1.0));
            }
            ServerMsg::Hurt { damage, .. } => {
                self.hurt_t = 0.6;
                self.recoil.x += (damage as f32 * 0.03).min(1.5);
                audio.play("hurt", 0.8);
            }
            ServerMsg::Kill { killer, victim, weapon: wid, headshot } => {
                let (vname, vteam) = self.name_team(victim);
                let (kname, kteam) = match killer {
                    Some(k) => {
                        let (n, t) = self.name_team(k);
                        (n, Some(t))
                    }
                    None => (String::new(), None),
                };
                let mine = self.my_id.is_some() && (killer == self.my_id || Some(victim) == self.my_id);
                self.killfeed.push(KillLine { killer: kname, kteam, victim: vname, vteam, weapon: wid, headshot, time: self.now, mine });
                if self.killfeed.len() > 6 {
                    self.killfeed.remove(0);
                }
            }
            ServerMsg::RoundStart { round } => {
                self.corpses.clear();
                self.particles.clear();
                self.tracers.clear();
                self.center = Some((format!("Round {}", round), self.now + 2.5, ui::TEXT));
            }
            ServerMsg::RoundEnd { winner, reason } => {
                let why = match reason {
                    EndReason::Elimination => "équipe adverse éliminée",
                    EndReason::Time => "temps écoulé",
                    EndReason::Exploded => "la bombe a explosé",
                    EndReason::Defused => "bombe désamorcée",
                };
                let color = if winner == Team::T { ui::T_COLOR } else { ui::CT_COLOR };
                self.center = Some((format!("Victoire des {} — {}", winner.label(), why), self.now + 4.5, color));
                let won = self.me().is_some_and(|m| m.team == winner);
                audio.play(if won { "win" } else { "lose" }, 0.6);
                if self.overlay == Overlay::Buy {
                    self.overlay = Overlay::None;
                }
            }
            ServerMsg::BombPlanted { site } => {
                let s = if site == 0 { "A" } else { "B" };
                self.center = Some((format!("La bombe a été posée sur le site {}", s), self.now + 3.0, Color::new(1.0, 0.45, 0.3, 1.0)));
                audio.play("planted", 0.7);
            }
            ServerMsg::BombDefused => audio.play("defused", 0.7),
            ServerMsg::BombExploded { pos } => {
                audio.play_at("explosion", self.cam_pos, pos, 160.0, 1.0);
                self.flash_t = (1.2 - self.cam_pos.distance(pos) / 40.0).clamp(0.15, 1.0);
                self.burst(pos + Vec3::Y * 0.5, Vec3::Y, 90, 9.0, 0.28, Color::new(1.0, 0.6, 0.15, 1.0));
                self.burst(pos + Vec3::Y * 0.5, Vec3::Y, 60, 5.0, 0.35, Color::new(0.25, 0.24, 0.23, 1.0));
            }
            ServerMsg::MatchEnd { winner } => {
                let (text, color) = match winner {
                    Some(Team::T) => ("Fin du match — victoire des Attaquants".to_string(), ui::T_COLOR),
                    Some(Team::CT) => ("Fin du match — victoire des Défenseurs".to_string(), ui::CT_COLOR),
                    None => ("Fin du match — égalité".to_string(), ui::TEXT),
                };
                self.center = Some((text, self.now + 11.0, color));
            }
            ServerMsg::Chat { from, team, text } => {
                self.chat.push((from, team, text, self.now));
                if self.chat.len() > 7 {
                    self.chat.remove(0);
                }
            }
        }
    }

    fn on_snapshot(&mut self, s: Snapshot) {
        if self.snaps.back().is_some_and(|l| s.time <= l.time) {
            return;
        }
        let sample = s.time - self.now;
        self.time_offset = Some(match self.time_offset {
            Some(o) if (sample - o).abs() < 0.25 => o + (sample - o) * 0.05,
            _ => sample,
        });
        for p in &s.players {
            if self.alive_prev.insert(p.id, p.alive) == Some(true) && !p.alive {
                self.corpses.push(Corpse { pos: p.pos, yaw: p.yaw, team: p.team, t0: self.now, weapon: p.weapon });
            }
        }
        self.snaps.push_back(s);
        while self.snaps.len() > 40 {
            self.snaps.pop_front();
        }
    }

    /// Positions interpolées de tous les joueurs à l'instant affiché.
    fn compute_views(&mut self, dt: f32, audio: &Audio) {
        self.views.clear();
        let n = self.snaps.len();
        if n == 0 {
            return;
        }
        let rt = self.render_time();
        let bi = (0..n).find(|i| self.snaps[*i].time >= rt).unwrap_or(n - 1);
        let b = &self.snaps[bi];
        let a = &self.snaps[bi.saturating_sub(1)];
        let span = (b.time - a.time) as f32;
        let k = if span > 0.0 { (((rt - a.time) as f32) / span).clamp(0.0, 1.0) } else { 1.0 };
        let carrier = match self.snaps[n - 1].bomb {
            BombState::Carried(id) => Some(id),
            _ => None,
        };
        for p in &b.players {
            let (mut pos, mut yaw, mut pitch, mut speed, mut vel) = (p.pos, p.yaw, p.pitch, 0.0, Vec3::ZERO);
            if let Some(q) = a.players.iter().find(|q| q.id == p.id) {
                if q.alive == p.alive && q.pos.distance(p.pos) < 4.0 {
                    pos = q.pos.lerp(p.pos, k);
                    yaw = q.yaw + angle_diff(q.yaw, p.yaw) * k;
                    pitch = q.pitch + (p.pitch - q.pitch) * k;
                    if span > 0.0 {
                        speed = hdist(q.pos, p.pos) / span;
                        vel = (p.pos - q.pos) / span;
                    }
                }
            }
            self.views.push(PlayerView {
                id: p.id,
                team: p.team,
                alive: p.alive,
                pos,
                vel,
                yaw,
                pitch,
                crouch: p.crouch,
                weapon: p.weapon,
                has_bomb: carrier == Some(p.id),
                speed,
                reloading: p.reloading,
                air: p.air,
                action: p.action,
            });
        }
        for v in &self.views {
            self.puppets.entry(v.id).or_default().update(v, dt);
            // Bruits de pas des autres joueurs (course uniquement).
            if v.alive && Some(v.id) != self.my_id && v.speed > 3.0 {
                let acc = self.steps.entry(v.id).or_insert(0.0);
                *acc += v.speed * dt;
                if *acc > 2.3 {
                    *acc = 0.0;
                    audio.play_at("step", self.cam_pos, v.pos, 22.0, 0.7);
                }
            }
        }
    }

    fn fire(&mut self, def: &'static WeaponDef, audio: &Audio, heavy: bool) {
        let (yaw, pitch) = self.view_angles();
        let base = dir_from_angles(yaw, pitch);
        let dir = perturb(base, self.spread(def).to_radians(), &mut self.rng);
        let origin = self.body.eye();
        let heavy = heavy && def.class == Class::Knife;
        self.net.reliable(ClientMsg::Fire { weapon: def.id, origin, dir, render_time: self.render_time(), heavy });
        self.last_shot = self.now;
        if def.class == Class::Knife {
            // Coup rapide ou coup puissant : ce dernier frappe plus fort mais immobilise la lame plus longtemps.
            self.fire_cd = if heavy { KNIFE_HEAVY_COOLDOWN } else { KNIFE_COOLDOWN };
            self.knife_t = 0.0;
            self.knife_heavy = heavy;
            audio.play("knife", if heavy { 1.0 } else { 0.6 });
            if let Some(h) = self.world.raycast(origin, dir, KNIFE_RANGE) {
                self.impact(origin + dir * h.dist, dir);
            }
            return;
        }
        self.va.fired(&mut self.rng);
        audio.play(Audio::shot_name(def.class), 0.8);
        // Douille éjectée sur la droite.
        let vb = render::view_basis(origin, yaw, pitch);
        let (right, up, fwd) = (vb.x_axis.truncate(), vb.y_axis.truncate(), vb.z_axis.truncate());
        self.particles.push(Particle {
            pos: vb.transform_point3(vec3(0.07, -0.05, 0.32)),
            vel: self.body.vel + right * self.rng.range(1.6, 2.4) + up * self.rng.range(1.0, 1.8) + fwd * self.rng.range(-0.3, 0.3),
            life: 0.7,
            size: 0.011,
            color: Color::new(0.85, 0.68, 0.25, 1.0),
        });
        self.ammo[self.slot].mag -= 1;
        self.fire_cd = 60.0 / def.rpm;
        self.muzzle_t = 0.05;
        // Effets locaux immédiats ; le serveur décide des dégâts.
        let hit = self.world.raycast(origin, dir, 250.0);
        let end = origin + dir * hit.as_ref().map_or(250.0, |h| h.dist);
        let m = render::view_basis(origin, yaw, pitch);
        self.tracers.push(Tracer { from: m.transform_point3(vec3(0.06, -0.07, 0.5)), to: end, life: 0.06 });
        if hit.is_some() {
            self.impact(end, dir);
        }
        // Recul : montée sur les premières balles, puis balancement latéral.
        let n = self.spray;
        let climb = if n < 8.0 { 0.7 + 0.3 * (n / 3.0).min(1.0) } else { 0.22 };
        let side = if n < 3.0 { self.rng.range(-0.3, 0.3) } else { (n * 0.45 + def.id as f32).sin() + self.rng.range(-0.3, 0.3) };
        self.recoil.x = (self.recoil.x + def.recoil_v * climb).min(14.0);
        self.recoil.y += def.recoil_h * side;
        self.spray += 1.0;
        if def.scope {
            self.rescope = (60.0 / def.rpm * 0.85, self.zoom);
            self.zoom = 0;
        }
    }

    pub fn update(&mut self, dt: f32, st: &Settings, audio: &Audio) {
        self.now = get_time();
        let (msgs, snaps) = self.net.poll();
        if !msgs.is_empty() || !snaps.is_empty() {
            self.last_recv = self.now;
        }
        for m in msgs {
            self.on_msg(m, audio);
        }
        for s in snaps {
            self.on_snapshot(s);
        }
        match self.status {
            Status::Connecting if self.now - self.start > 5.0 => self.status = Status::Failed("Le serveur ne répond pas".into()),
            Status::Connected if self.now - self.last_recv > 8.0 => self.status = Status::Failed("Connexion au serveur perdue".into()),
            _ => {}
        }

        // Souris : différence de position tant que le curseur est capturé.
        // Mouvements bruts de la souris, indépendants de la mise à l'échelle de l'écran.
        let mp = Vec2::from(mouse_position()) * miniquad::window::dpi_scale();
        let md = if self.grab_frames >= 2 { mp - self.last_mouse } else { Vec2::ZERO };
        self.last_mouse = mp;

        // Effets
        self.fire_cd -= dt;
        self.draw_t -= dt;
        self.muzzle_t -= dt;
        self.knife_t += dt;
        self.hitmarker_t -= dt;
        self.hurt_t -= dt;
        self.flash_t -= dt * 0.8;
        self.step_smooth *= (-14.0 * dt).exp();
        for p in &mut self.particles {
            p.vel.y -= 12.0 * dt;
            p.pos += p.vel * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for t in &mut self.tracers {
            t.life -= dt;
        }
        self.tracers.retain(|t| t.life > 0.0);
        self.compute_views(dt, audio);

        let Some(me) = self.me().cloned() else {
            self.send_state(dt, None);
            return;
        };
        let (phase, bomb, time_left) = self.snap().map_or((Phase::Warmup, BombState::None, 0.0), |s| (s.phase, s.bomb, s.time_left));
        self.sync_inventory(&me);
        if self.debug & 4 != 0 && self.slot != 2 {
            self.switch_to(2);
        }
        let can_act = me.alive && self.overlay == Overlay::None;
        let def = weapon(self.weapon_id());

        if me.alive {
            // Regard
            if self.grab_frames >= 2 && self.overlay == Overlay::None {
                let zoomf = if self.zoom > 0 { self.zoom_fov(def) / st.fov * st.zoom_sensitivity } else { 1.0 };
                let k = st.sensitivity * 0.022f32.to_radians() * zoomf;
                self.yaw -= md.x * k;
                self.pitch = (self.pitch - md.y * k).clamp(-1.55, 1.55);
            }
            // Déplacement
            let mut inp = MoveInput { yaw: self.yaw, ..Default::default() };
            let mut acting = false;
            self.using = false;
            if can_act && phase != Phase::Freeze {
                let axis = |a: Action, b: Action| (st.down(a) as i32 - st.down(b) as i32) as f32;
                inp.fwd = axis(Action::Forward, Action::Back);
                inp.side = axis(Action::Right, Action::Left);
                inp.jump = st.pressed(Action::Jump);
                inp.crouch = st.down(Action::Crouch);
                inp.walk = st.down(Action::Walk);
                self.using = st.down(Action::Use);
                let planting = self.has_bomb() && self.info.site_of(self.body.pos).is_some();
                let defusing = me.team == Team::CT
                    && matches!(bomb, BombState::Planted { pos, .. } if pos.distance(self.body.pos) < 1.8);
                if self.using && (planting || defusing) {
                    inp.fwd = 0.0;
                    inp.side = 0.0;
                    inp.jump = false;
                    acting = true;
                }
            }
            let speed = BASE_SPEED * def.speed * if self.zoom > 0 { 0.55 } else { 1.0 };
            let (y0, ground0) = (self.body.pos.y, self.body.on_ground);
            physics::step(&self.world, &mut self.body, &inp, speed, dt);
            if ground0 && self.body.on_ground {
                let dy = self.body.pos.y - y0;
                if dy.abs() > 0.05 {
                    self.step_smooth -= dy;
                }
            }
            if self.body.pos.y < -20.0 {
                self.body.pos = self.info.t_spawns[0].0;
            }
            // Animation de l'arme en main ; les bruits de pas suivent le balancement.
            let ev = self.va.update(&self.body, self.yaw, self.pitch, dt);
            if ev.step && self.body.hspeed() > BASE_SPEED * 0.6 {
                audio.play("step", 0.35);
            }
            if ev.landed > 3.0 {
                audio.play("step", (ev.landed / 10.0).clamp(0.3, 0.9));
                self.step_smooth -= (ev.landed * 0.012).min(0.12);
            }

            // Armes
            if can_act {
                if st.pressed(Action::Primary) && self.ammo[0].id != 0 {
                    self.switch_to(0);
                }
                if st.pressed(Action::Secondary) && self.ammo[1].id != 0 {
                    self.switch_to(1);
                }
                if st.pressed(Action::Knife) {
                    self.switch_to(2);
                }
                let wheel = mouse_wheel().1;
                if wheel != 0.0 {
                    let avail: Vec<usize> = (0..3).filter(|s| *s == 2 || self.ammo[*s].id != 0).collect();
                    let i = avail.iter().position(|s| *s == self.slot).unwrap_or(0) as i32;
                    let n = avail.len() as i32;
                    self.switch_to(avail[((i + if wheel < 0.0 { 1 } else { -1 }) + n) as usize % n as usize]);
                }
            }
            let def = weapon(self.weapon_id());
            if self.reload_t > 0.0 {
                // Bruitages calés sur les étapes de l'animation.
                let r = 1.0 - self.reload_t / def.reload.max(0.1);
                for (stage, at, name) in [(1u8, 0.16, "mag_out"), (2, 0.66, "mag_in"), (3, 0.80, "rack")] {
                    if self.reload_stage < stage && r >= at {
                        self.reload_stage = stage;
                        audio.play(name, 0.55);
                    }
                }
                self.reload_t -= dt;
                if self.reload_t <= 0.0 && self.slot < 2 {
                    let a = &mut self.ammo[self.slot];
                    let take = (def.mag - a.mag).min(a.reserve);
                    a.mag += take;
                    a.reserve -= take;
                }
            }
            if !def.scope {
                self.zoom = 0;
            }
            if self.rescope.0 > 0.0 {
                self.rescope.0 -= dt;
                if self.rescope.0 <= 0.0 && def.scope && self.reload_t <= 0.0 {
                    self.zoom = self.rescope.1;
                }
            }
            if can_act {
                if def.scope && st.pressed(Action::Aim) && self.reload_t <= 0.0 && self.draw_t <= 0.0 {
                    self.zoom = (self.zoom + 1) % 3;
                    self.rescope = (0.0, 0);
                    audio.play("click", 0.3);
                }
                let a = if self.slot < 2 { self.ammo[self.slot] } else { Ammo::default() };
                let can_reload = self.slot < 2 && self.reload_t <= 0.0 && a.mag < def.mag && a.reserve > 0;
                // Couteau : le clic droit (touche de visée) déclenche le coup puissant.
                let heavy = self.slot == 2 && (st.down(Action::Aim) || self.debug & 8 != 0);
                let trigger = self.debug & 1 != 0 || heavy || if def.auto { st.down(Action::Fire) } else { st.pressed(Action::Fire) };
                let live = matches!(phase, Phase::Live | Phase::RoundEnd);
                if st.pressed(Action::Reload) && can_reload {
                    self.start_reload(def, audio);
                } else if trigger && live && !acting && self.fire_cd <= 0.0 && self.draw_t <= 0.0 && self.reload_t <= 0.0 {
                    if self.slot == 2 || a.mag > 0 {
                        self.fire(def, audio, heavy);
                    } else if can_reload {
                        self.start_reload(def, audio);
                    } else if st.pressed(Action::Fire) {
                        audio.play("click", 0.5);
                    }
                }
                if st.pressed(Action::Buy) {
                    if self.can_buy() {
                        self.overlay = Overlay::Buy;
                        self.overlay_since = self.now;
                    } else {
                        let why = if self.buy_time_left().is_some() {
                            "Retournez dans la zone d'achat pour acheter"
                        } else {
                            "Le temps d'achat est écoulé"
                        };
                        self.center = Some((why.to_string(), self.now + 2.0, Color::new(1.0, 0.6, 0.4, 1.0)));
                        audio.play("click", 0.4);
                    }
                }
                if st.pressed(Action::Chat) {
                    self.overlay = Overlay::Chat;
                    self.chat_input.clear();
                }
            }
            // Retour du recul une fois la rafale terminée.
            if self.now - self.last_shot > (60.0 / def.rpm * 1.4) as f64 {
                self.spray = (self.spray - dt * 12.0).max(0.0);
                self.recoil *= (-9.0 * dt).exp();
            }
            let (yaw, pitch) = self.view_angles();
            // Léger balancement de la caméra à chaque pas.
            self.cam_pos = self.body.eye() + Vec3::Y * (self.step_smooth - self.va.phase.sin().abs() * 0.012 * self.va.amp);
            self.cam_yaw = yaw;
            self.cam_pitch = pitch;
            self.cam_fovy = fovy_from(if self.zoom > 0 { self.zoom_fov(def) } else { st.fov });
            self.send_state(
                dt,
                Some(ClientState {
                    n: 0,
                    pos: self.body.pos,
                    yaw,
                    pitch,
                    crouch: self.body.crouched,
                    weapon: self.weapon_id(),
                    using: self.using,
                    reloading: self.reload_t > 0.0,
                    air: !self.body.on_ground,
                }),
            );
        } else {
            // Mort : caméra sur un coéquipier vivant, clic pour changer.
            self.zoom = 0;
            self.reload_t = 0.0;
            self.recoil = Vec2::ZERO;
            let mut cands: Vec<PlayerId> = self.views.iter().filter(|v| v.alive && v.team == me.team).map(|v| v.id).collect();
            if cands.is_empty() {
                cands = self.views.iter().filter(|v| v.alive).map(|v| v.id).collect();
            }
            let cur = self.spectate.and_then(|s| cands.iter().position(|c| *c == s));
            match cur {
                Some(i) if self.overlay == Overlay::None && st.pressed(Action::Fire) => {
                    self.spectate = Some(cands[(i + 1) % cands.len()]);
                }
                Some(_) => {}
                None => self.spectate = cands.first().copied(),
            }
            if let Some(v) = self.spectate.and_then(|s| self.views.iter().find(|v| v.id == s)) {
                self.cam_pos = v.pos + Vec3::Y * eye_height(if v.crouch { 1.0 } else { 0.0 });
                self.cam_yaw = v.yaw;
                self.cam_pitch = v.pitch;
            }
            self.cam_fovy = fovy_from(st.fov);
            if self.overlay == Overlay::None && st.pressed(Action::Chat) {
                self.overlay = Overlay::Chat;
                self.chat_input.clear();
            }
            self.send_state(dt, None);
        }
        if self.overlay == Overlay::Buy && !self.can_buy() {
            self.overlay = Overlay::None;
        }

        // Bip de la bombe, de plus en plus rapide.
        if let (Phase::Live, BombState::Planted { pos, .. }) = (phase, bomb) {
            let interval = (0.12 + 0.9 * (time_left / self.cfg.bomb_time.max(1.0)).clamp(0.0, 1.0)) as f64;
            if self.now - self.last_beep > interval {
                self.last_beep = self.now;
                audio.play_at("beep", self.cam_pos, pos, 45.0, 0.8);
            }
        }
    }

    fn zoom_fov(&self, def: &WeaponDef) -> f32 {
        match (self.zoom, def.id) {
            (1, _) => 40.0,
            (_, 11) => 10.0,
            _ => 15.0,
        }
    }

    fn start_reload(&mut self, def: &WeaponDef, audio: &Audio) {
        self.reload_t = def.reload;
        self.zoom = 0;
        self.rescope = (0.0, 0);
        self.reload_stage = 0;
        let _ = audio;
    }

    /// Envoie l'état au serveur, 60 fois par seconde au plus.
    fn send_state(&mut self, dt: f32, state: Option<ClientState>) {
        self.send_acc += dt;
        if self.send_acc < 1.0 / 60.0 {
            return;
        }
        self.send_acc = (self.send_acc - 1.0 / 60.0).min(1.0 / 60.0);
        self.send_n += 1;
        let n = self.send_n;
        self.net.send(state.map(|s| ClientState { n, ..s }));
    }

    pub fn buy(&mut self, item: BuyItem, audio: &Audio) {
        self.net.reliable(ClientMsg::Buy(item));
        audio.play("buy", 0.5);
    }

    /// Dessine la scène 3D.
    pub fn draw_world(&mut self, wr: &WorldRender, batch: &mut Batch) {
        clear_background(render::SKY);
        let (eye, yaw, pitch) = (self.cam_pos, self.cam_yaw, self.cam_pitch);
        let fwd = dir_from_angles(yaw, pitch);
        set_camera(&Camera3D {
            position: eye,
            target: eye + fwd,
            up: Vec3::Y,
            fovy: self.cam_fovy,
            z_near: 0.03,
            z_far: 400.0,
            ..Default::default()
        });
        wr.begin(eye);
        wr.draw(eye, fwd, self.cam_fovy);

        let me_alive = self.me().is_some_and(|m| m.alive);
        let hidden = if me_alive { self.my_id } else { self.spectate };
        let idle = Puppet::default();
        for v in &self.views {
            if v.alive && Some(v.id) != hidden {
                render::draw_player(batch, v, self.puppets.get(&v.id).unwrap_or(&idle));
            }
        }
        for c in &self.corpses {
            render::draw_corpse(batch, c.pos, c.yaw, c.team, ((self.now - c.t0) / 0.6) as f32, c.weapon);
        }
        match self.snap().map(|s| s.bomb) {
            Some(BombState::Planted { pos, .. }) | Some(BombState::Dropped(pos)) | Some(BombState::Defused(pos)) => {
                let blink = matches!(self.snap().map(|s| s.bomb), Some(BombState::Planted { .. })) && self.now - self.last_beep < 0.12;
                render::bomb_model(batch, &Mat4::from_translation(pos), blink);
            }
            _ => {}
        }
        for t in &self.tracers {
            render::beam(batch, t.from, t.to, 0.012, Color::new(1.0, 0.92, 0.6, 1.0));
        }
        for p in &self.particles {
            let s = p.size * (p.life * 4.0).min(1.0);
            batch.cube(&Mat4::from_translation(p.pos), Vec3::splat(-s), Vec3::splat(s), p.color);
        }
        batch.flush();

        // Diagnostic (bit 64) : planche de poses devant la caméra.
        if self.debug & 64 != 0 {
            let flat = vec3(yaw.sin(), 0.0, yaw.cos());
            let side = vec3(-yaw.cos(), 0.0, yaw.sin());
            let w = if self.debug_weapon != 0 { self.debug_weapon } else { 7 };
            let mk = |phase: f32, moving: f32, dir: f32, crouch: f32, air: f32, kneel: f32, reload: f32, shot: f32| Puppet { phase, moving, dir, crouch, air, kneel, reload, shot, ..Default::default() };
            let sheet = [
                (mk(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), w, Team::T),
                (mk(0.4, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), w, Team::T),
                (mk(1.9, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0), w, Team::CT),
                (mk(1.0, 0.5, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0), w, Team::CT),
                (mk(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.3, 0.0), w, Team::T),
                (mk(0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0), 1, Team::CT),
                (mk(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0), w, Team::T),
                (mk(0.8, 1.0, 1.57, 0.0, 0.0, 0.0, 0.0, 0.0), 0, Team::CT),
            ];
            for (i, (p, weapon_id, team)) in sheet.iter().enumerate() {
                let pos = vec3(eye.x, self.body.pos.y, eye.z) + flat * 5.5 + side * ((i as f32 - 3.5) * 1.25);
                let v = PlayerView {
                    id: 250,
                    team: *team,
                    alive: true,
                    pos,
                    vel: Vec3::ZERO,
                    yaw: yaw - 1.2,
                    pitch: 0.0,
                    crouch: p.crouch > 0.5,
                    weapon: *weapon_id,
                    has_bomb: false,
                    speed: 0.0,
                    reloading: false,
                    air: false,
                    action: 0,
                };
                render::draw_player(batch, &v, p);
            }
        }
        batch.flush();
        if me_alive && self.zoom == 0 {
            viewmodel::draw(self, batch, eye, yaw, pitch);
            batch.flush();
        }
        set_default_camera();
        gl_use_default_material();
    }

    /// Déconnexion propre.
    pub fn leave(&mut self) {
        self.net.leave();
    }
}
