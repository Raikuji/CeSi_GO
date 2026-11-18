//! Armes au sol : lâchées par les joueurs éliminés ou jetées volontairement,
//! ramassées en passant dessus ou échangées contre l'arme portée.

use crate::game::Server;
use cesi_common::glam::{vec3, Vec3};
use cesi_common::physics::eye_height;
use cesi_common::protocol::{BombState, Phase, PlayerId, ServerMsg};
use cesi_common::weapons::{weapon, Class, PICKUP_RANGE};
use cesi_common::{dir_from_angles, hdist};

/// Au-delà, la plus ancienne arme au sol disparaît.
const MAX_DROPS: usize = 24;
/// Distance à laquelle on ramasse une arme en marchant dessus.
const AUTO_RANGE: f32 = 0.9;
/// Celui qui jette une arme ne la reprend pas en marchant dessus avant ce délai,
/// ni tant qu'il ne s'en est pas éloigné de `OWNER_CLEAR`.
const OWNER_DELAY: f64 = 1.0;
const OWNER_CLEAR: f32 = 1.3;

pub(crate) struct Drop {
    pub id: u16,
    pub weapon: u8,
    pub pos: Vec3,
    pub yaw: f32,
    pub mag: u16,
    pub reserve: u16,
    /// Joueur qui vient de la jeter, tant qu'il ne peut pas la reprendre en passant.
    pub owner: Option<PlayerId>,
    pub free_at: f64,
}

fn is_pistol(wid: u8) -> bool {
    weapon(wid).class == Class::Pistol
}

impl Server {
    /// Pose une arme au sol à l'aplomb de `from` (un point dégagé) ; sans `ammo`, elle est pleine.
    fn place(&mut self, wid: u8, from: Vec3, yaw: f32, ammo: Option<(u16, u16)>, owner: Option<PlayerId>) {
        let def = weapon(wid);
        if wid == 0 || def.class == Class::Knife {
            return;
        }
        let fall = self.world.raycast(from, -Vec3::Y, 40.0).map_or(0.0, |h| h.dist);
        let (mag, reserve) = ammo.map_or((def.mag, def.reserve), |(m, r)| (m.min(def.mag), r.min(def.reserve)));
        if self.drops.len() >= MAX_DROPS {
            self.drops.remove(0);
        }
        self.next_drop = self.next_drop.wrapping_add(1);
        self.drops.push(Drop {
            id: self.next_drop,
            weapon: wid,
            pos: from - Vec3::Y * fall,
            yaw,
            mag,
            reserve,
            owner,
            free_at: self.time + OWNER_DELAY,
        });
    }

    /// Arme lâchée près de `pos` (pieds d'un joueur), légèrement décalée pour ne pas les empiler.
    pub(crate) fn drop_weapon(&mut self, wid: u8, pos: Vec3, ammo: Option<(u16, u16)>) {
        let yaw = self.rng.range(0.0, std::f32::consts::TAU);
        let centre = pos + Vec3::Y * 0.9;
        let mut from = centre + vec3(yaw.cos(), 0.0, yaw.sin()) * 0.35;
        if self.world.aabb_hits(from - Vec3::splat(0.2), from + Vec3::splat(0.2)) {
            from = centre;
        }
        self.place(wid, from, yaw, ammo, None);
    }

    /// Le joueur `i` jette l'arme de l'emplacement `slot` (0 principale, 1 pistolet) devant lui.
    pub(crate) fn throw_weapon(&mut self, i: usize, slot: u8, mag: u16, reserve: u16) {
        let p = &mut self.players[i];
        if !p.alive || self.phase == Phase::Warmup {
            return;
        }
        let wid = match slot {
            0 => std::mem::take(&mut p.primary),
            1 => std::mem::take(&mut p.secondary),
            _ => 0,
        };
        if wid == 0 {
            return;
        }
        let (pid, yaw) = (p.id, p.yaw);
        let eye = p.pos + Vec3::Y * eye_height(if p.crouch { 1.0 } else { 0.0 });
        let dir = dir_from_angles(yaw, 0.0);
        // Elle retombe à 1,4 m, ou au pied de l'obstacle s'il y en a un devant.
        let reach = self.world.raycast(eye, dir, 1.7).map_or(1.4, |h| (h.dist - 0.3).max(0.0));
        // Une arme jetée ne peut plus être revendue (sinon on pourrait la donner puis se la faire rembourser).
        self.forget_weapon_purchases(i, slot == 1);
        self.place(wid, eye + dir * reach, yaw + 1.2, Some((mag, reserve)), Some(pid));
    }

    /// Point au sol à l'aplomb de `pos`.
    pub(crate) fn ground_below(&self, pos: Vec3) -> Vec3 {
        let from = pos + Vec3::Y * 0.3;
        from - Vec3::Y * self.world.raycast(from, -Vec3::Y, 40.0).map_or(0.3, |h| h.dist)
    }

    /// Le porteur `i` lâche la bombe devant lui ; un coéquipier la ramasse en passant dessus.
    pub(crate) fn drop_bomb(&mut self, i: usize) {
        let p = &mut self.players[i];
        if !p.alive || self.bomb != BombState::Carried(p.id) || !matches!(self.phase, Phase::Freeze | Phase::Live) {
            return;
        }
        p.action = 0;
        p.progress = 0.0;
        let (pid, yaw) = (p.id, p.yaw);
        let eye = p.pos + Vec3::Y * eye_height(if p.crouch { 1.0 } else { 0.0 });
        let dir = dir_from_angles(yaw, 0.0);
        let reach = self.world.raycast(eye, dir, 1.7).map_or(1.4, |h| (h.dist - 0.3).max(0.0));
        self.bomb = BombState::Dropped(self.ground_below(eye + dir * reach));
        self.bomb_block = Some((pid, self.time + OWNER_DELAY));
    }

    /// Le joueur `i` prend l'arme au sol `id`. S'il en portait une du même emplacement
    /// (principale ou pistolet), elle prend sa place au sol avec les munitions annoncées.
    /// `auto` : ramassée en passant dessus, et non avec la touche d'action.
    pub(crate) fn pickup(&mut self, i: usize, id: u16, mag: u16, reserve: u16, auto: bool) {
        let Some(k) = self.drops.iter().position(|d| d.id == id) else { return };
        let (p, d) = (&self.players[i], &self.drops[k]);
        if !p.alive || hdist(p.pos, d.pos) > PICKUP_RANGE + 0.5 || (p.pos.y - d.pos.y).abs() > 2.5 {
            return;
        }
        let d = self.drops.remove(k);
        self.forget_weapon_purchases(i, is_pistol(d.weapon));
        let p = &mut self.players[i];
        let slot = if is_pistol(d.weapon) { &mut p.secondary } else { &mut p.primary };
        let old = std::mem::replace(slot, d.weapon);
        let (pid, bot) = (p.id, p.is_bot);
        if old != 0 {
            let def = weapon(old);
            self.next_drop = self.next_drop.wrapping_add(1);
            self.drops.push(Drop {
                id: self.next_drop,
                weapon: old,
                pos: d.pos,
                yaw: d.yaw + 0.6,
                mag: mag.min(def.mag),
                reserve: reserve.min(def.reserve),
                owner: None,
                free_at: 0.0,
            });
        }
        if !bot {
            self.send_to(pid, ServerMsg::Picked { weapon: d.weapon, mag: d.mag, reserve: d.reserve, auto });
        }
    }

    /// Un joueur (ou un bot) qui passe sur une arme la ramasse si l'emplacement
    /// correspondant est vide ; sinon il faut l'échanger avec la touche d'action.
    pub(crate) fn auto_pickup(&mut self) {
        let now = self.time;
        // Celui qui a jeté une arme peut la reprendre une fois le délai passé et après s'en être éloigné.
        for d in &mut self.drops {
            if let Some(o) = d.owner {
                let near = self.players.iter().any(|p| p.id == o && p.alive && hdist(p.pos, d.pos) < OWNER_CLEAR);
                if !near && now >= d.free_at {
                    d.owner = None;
                }
            }
        }
        for i in 0..self.players.len() {
            let p = &self.players[i];
            if !p.alive {
                continue;
            }
            let found = self.drops.iter().find(|d| {
                let empty = if is_pistol(d.weapon) { p.secondary == 0 } else { p.primary == 0 };
                empty && d.owner != Some(p.id) && hdist(d.pos, p.pos) < AUTO_RANGE && (d.pos.y - p.pos.y).abs() < 1.0
            });
            if let Some(id) = found.map(|d| d.id) {
                self.pickup(i, id, 0, 0, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Player, ServerConfig};
    use cesi_common::protocol::Team;

    /// Serveur local avec un joueur vivant posé sur un point de départ.
    fn server() -> Server {
        let cfg = ServerConfig { port: 0, local_only: true, ..Default::default() };
        let mut s = Server::bind(cfg, None, Box::new(|_| {})).unwrap();
        let mut p = Player::new(1, "Test".into(), Team::T, 800, None);
        p.alive = true;
        p.pos = s.info.t_spawns[0].0;
        p.primary = 0;
        p.secondary = 1;
        s.players.push(p);
        s.phase = Phase::Live;
        s
    }

    #[test]
    fn walking_over_fills_an_empty_slot_only() {
        let mut s = server();
        let pos = s.players[0].pos;
        s.drop_weapon(7, pos, None);
        s.drop_weapon(2, pos, None);
        s.auto_pickup();
        assert_eq!(s.players[0].primary, 7, "l'arme principale est ramassée");
        assert_eq!(s.players[0].secondary, 1, "le pistolet porté n'est pas remplacé");
        assert_eq!(s.drops.len(), 1);
        assert!((s.drops[0].pos.y - pos.y).abs() < 0.1, "l'arme repose au sol");
    }

    #[test]
    fn thrown_weapon_lands_ahead_and_is_not_taken_back_at_once() {
        let mut s = server();
        s.players[0].primary = 7;
        let start = s.players[0].pos;
        s.throw_weapon(0, 0, 11, 40);
        assert_eq!(s.players[0].primary, 0);
        assert_eq!((s.drops[0].weapon, s.drops[0].mag, s.drops[0].reserve), (7, 11, 40));
        let spot = s.drops[0].pos;
        assert!(hdist(spot, start) > 1.0, "elle tombe devant le joueur");
        // Sur place, puis en marchant dessus tout de suite : rien.
        s.auto_pickup();
        s.players[0].pos = vec3(spot.x, start.y, spot.z);
        s.auto_pickup();
        assert_eq!(s.players[0].primary, 0);
        // Après s'être éloigné et le délai passé, on la reprend en repassant dessus.
        s.players[0].pos = start + vec3(0.0, 0.0, -3.0);
        s.time += 2.0;
        s.auto_pickup();
        s.players[0].pos = vec3(spot.x, start.y, spot.z);
        s.auto_pickup();
        assert_eq!(s.players[0].primary, 7);
        assert!(s.drops.is_empty());
    }

    #[test]
    fn dropped_bomb_goes_to_a_teammate_not_back_to_the_carrier() {
        let mut s = server();
        let mut mate = Player::new(2, "Ami".into(), Team::T, 800, None);
        mate.alive = true;
        mate.pos = s.players[0].pos + vec3(0.0, 0.0, -4.0);
        s.players.push(mate);
        s.bomb = BombState::Carried(1);
        s.drop_bomb(0);
        let BombState::Dropped(spot) = s.bomb else { panic!("la bombe doit être au sol : {:?}", s.bomb) };
        assert!((spot.y - s.players[0].pos.y).abs() < 0.1, "elle repose au sol");
        // Le porteur qui avance dessus ne la reprend pas tout de suite…
        s.players[0].pos = vec3(spot.x, spot.y, spot.z);
        s.update_bomb(0.016);
        assert!(matches!(s.bomb, BombState::Dropped(_)));
        // … mais un coéquipier qui passe dessus la ramasse.
        s.players[0].pos = spot + vec3(3.0, 0.0, 0.0);
        s.players[1].pos = spot + vec3(0.3, 0.0, 0.0);
        s.update_bomb(0.016);
        assert_eq!(s.bomb, BombState::Carried(2));
    }

    #[test]
    fn bomb_of_a_carrier_killed_in_the_air_falls_to_the_ground() {
        let mut s = server();
        let ground = s.players[0].pos;
        s.bomb = BombState::Carried(1);
        s.players[0].pos = ground + Vec3::Y * 1.5;
        s.damage(0, None, 500.0, 1.0, 1, 0, ground);
        assert_eq!(s.bomb, BombState::Dropped(s.ground_below(ground)));
        let BombState::Dropped(spot) = s.bomb else { unreachable!() };
        assert!((spot.y - ground.y).abs() < 0.1);
    }

    #[test]
    fn assist_needs_40_damage_on_an_enemy_finished_by_someone_else() {
        let mut s = server();
        let foe_spawn = s.info.ct_spawns[0].0;
        for (id, team, pos) in [(2, Team::T, s.players[0].pos), (3, Team::T, s.players[0].pos), (4, Team::CT, foe_spawn)] {
            let mut p = Player::new(id, format!("J{}", id), team, 800, None);
            p.alive = true;
            p.health = 100;
            p.pos = pos;
            s.players.push(p);
        }
        s.players[0].health = 100;
        // Le joueur 1 inflige 45, le joueur 2 en inflige 30, le joueur 3 achève l'adversaire.
        s.damage(3, Some(0), 45.0, 1.0, 1, 2, foe_spawn);
        s.damage(3, Some(1), 30.0, 1.0, 1, 2, foe_spawn);
        s.damage(3, Some(2), 60.0, 1.0, 1, 2, foe_spawn);
        assert!(!s.players[3].alive);
        assert_eq!((s.players[0].assists, s.players[1].assists, s.players[2].assists), (1, 0, 0));
        assert_eq!(s.players[2].kills, 1);
        // Les dégâts d'un coéquipier ne comptent pas.
        (s.players[3].alive, s.players[3].health) = (true, 100);
        s.damage(0, Some(1), 50.0, 1.0, 1, 2, foe_spawn);
        assert!(s.players[0].damage_from.is_empty());
    }

    #[test]
    fn exchange_leaves_the_old_weapon_with_its_ammo() {
        let mut s = server();
        s.players[0].primary = 7;
        let pos = s.players[0].pos;
        s.drop_weapon(10, pos, None);
        let id = s.drops[0].id;
        s.auto_pickup();
        assert_eq!(s.players[0].primary, 7, "pas d'échange automatique");
        s.pickup(0, id, 5, 30, false);
        assert_eq!(s.players[0].primary, 10);
        assert_eq!((s.drops[0].weapon, s.drops[0].mag, s.drops[0].reserve), (7, 5, 30));
    }
}
