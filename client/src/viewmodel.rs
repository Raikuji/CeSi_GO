//! Arme à la première personne : balancement à la marche, inertie quand la
//! vue tourne, recul, sortie de l'arme, rechargement en plusieurs étapes.
//!
//! Le modèle est réduit et placé tout près de l'œil pour ne jamais traverser
//! les murs ; les positions ci-dessous sont exprimées avant cette réduction.

use crate::game::Game;
use crate::render::{self, grip, limb, reload_pose, ss, Batch, WeaponPose};
use cesi_common::grenades::{is_grenade, GRENADE_BASE, THROW_WINDUP};
use cesi_common::physics::{Body, BASE_SPEED};
use cesi_common::protocol::Team;
use cesi_common::weapons::{weapon, Class};
use cesi_common::{angle_diff, Rng};
use macroquad::prelude::*;
use std::f32::consts::PI;

const SCALE: f32 = 0.25;
const SKIN: Color = Color::new(0.87, 0.68, 0.53, 1.0);

/// État lissé des mouvements de l'arme en main.
#[derive(Default)]
pub struct ViewAnim {
    /// Phase du pas (un demi-tour par pas).
    pub phase: f32,
    /// Amplitude du balancement, de 0 (immobile ou en l'air) à 1 (course).
    pub amp: f32,
    /// Retard de l'arme sur la vue (lacet vers la droite, tangage vers le haut).
    pub sway: Vec2,
    last: Option<(f32, f32)>,
    /// Enfoncement à la réception d'un saut.
    pub land: f32,
    /// Décalage vertical en l'air.
    pub air: f32,
    /// Inclinaison latérale en pas de côté.
    pub tilt: f32,
    /// Recul du dernier tir, de 1 à 0.
    pub kick: f32,
    /// Variation aléatoire du recul et de la flamme, tirée à chaque tir.
    pub roll: f32,
    pub flash: f32,
    was_ground: bool,
    fall: f32,
    steps: i32,
}

pub struct StepEvents {
    /// Un pied vient de toucher le sol.
    pub step: bool,
    /// Vitesse de chute à l'atterrissage (0 sinon).
    pub landed: f32,
}

impl ViewAnim {
    pub fn update(&mut self, body: &Body, yaw: f32, pitch: f32, dt: f32) -> StepEvents {
        let k = |rate: f32| (rate * dt).min(1.0);
        let hs = body.hspeed();
        let mut ev = StepEvents { step: false, landed: 0.0 };
        let target = if body.on_ground { (hs / BASE_SPEED).min(1.0) } else { 0.0 };
        self.amp += (target - self.amp) * k(9.0);
        if body.on_ground {
            self.phase = (self.phase + hs * dt * 1.45) % (PI * 64.0);
            let n = (self.phase / PI).floor() as i32;
            if n != self.steps {
                self.steps = n;
                ev.step = hs > 0.5;
            }
        }
        // L'arme suit la vue avec un léger retard.
        if let Some((ly, lp)) = self.last {
            self.sway += vec2(-angle_diff(ly, yaw), pitch - lp) * 0.6;
        }
        self.last = Some((yaw, pitch));
        self.sway *= (-10.0 * dt).exp();
        self.sway = self.sway.clamp_length_max(0.09);
        // Saut : l'arme monte pendant la chute, s'enfonce à la réception.
        if !body.on_ground {
            self.fall = body.vel.y;
        } else if !self.was_ground {
            let f = (-self.fall).max(0.0);
            self.land = (f * 0.012).min(0.1);
            ev.landed = f;
        }
        self.was_ground = body.on_ground;
        self.land *= (-8.0 * dt).exp();
        let air = if body.on_ground { 0.0 } else { (-body.vel.y * 0.004).clamp(-0.02, 0.035) };
        self.air += (air - self.air) * k(10.0);
        let (s, c) = yaw.sin_cos();
        let side = -body.vel.x * c + body.vel.z * s;
        self.tilt += ((side / BASE_SPEED).clamp(-1.0, 1.0) * 0.05 - self.tilt) * k(8.0);
        self.kick = (self.kick - dt * 8.0).max(0.0);
        ev
    }

    pub fn fired(&mut self, rng: &mut Rng) {
        self.kick = 1.0;
        self.roll = rng.range(-1.0, 1.0);
        self.flash = rng.range(0.0, 6.28);
    }
}

pub fn draw(g: &Game, b: &mut Batch, eye: Vec3, yaw: f32, pitch: f32) {
    let id = if g.debug_weapon != 0 { g.debug_weapon } else { g.weapon_id() };
    let def = weapon(id);
    let va = &g.va;
    let t = g.now as f32;
    let sleeve = if g.me().is_some_and(|m| m.team == Team::T) { Color::from_rgba(196, 150, 78, 255) } else { Color::from_rgba(58, 88, 140, 255) };
    let view = render::view_basis(eye, yaw, pitch) * Mat4::from_scale(Vec3::splat(SCALE));
    let side = view.x_axis.truncate().normalize();
    let crouch = g.body.crouch_t;

    // Sortie de l'arme : elle monte depuis le bas de l'écran en pivotant.
    let e = (g.draw_t.max(0.0) / 0.4).min(1.0);
    let draw = e * e * (3.0 - 2.0 * e);

    // Mouvements communs : pas, respiration, inertie, saut, accroupi.
    let mut off = vec3(
        va.phase.cos() * 0.012 * va.amp + (t * 0.9).sin() * 0.002 - va.sway.x * 0.35,
        -va.phase.sin().abs() * 0.014 * va.amp + (t * 1.7).sin() * 0.003 - va.land * 0.6 + va.air - 0.012 * crouch - va.sway.y * 0.3 - 0.34 * draw,
        -0.012 * va.amp,
    );
    let mut rx = va.sway.y * 0.9 + 1.0 * draw;
    let mut ry = -va.sway.x * 0.9;
    let mut rz = -va.tilt + 0.05 * crouch + va.phase.cos() * 0.02 * va.amp - 0.45 * draw;

    if is_grenade(id) {
        // Grenade en main : le bras part en arrière à l'armement, puis fouette vers l'avant.
        let mut pos = vec3(0.21, -0.2, 0.5) + off;
        let released = g.throw_t <= 0.0 && g.throw_anim < 0.45;
        if g.throw_t > 0.0 {
            let w = ss(0.0, 1.0, 1.0 - g.throw_t / THROW_WINDUP);
            pos += vec3(0.07, 0.11, -0.24) * w;
            rx -= 0.7 * w;
        } else if released {
            let a = (g.throw_anim / 0.3).min(1.0);
            pos += vec3(-0.12, 0.08, 0.32) * (a * PI).sin() + vec3(0.0, -0.3, 0.0) * a;
            rx += 1.0 * a;
        }
        let hm = view * Mat4::from_translation(pos) * Mat4::from_rotation_y(ry - 0.2) * Mat4::from_rotation_x(rx) * Mat4::from_rotation_z(rz);
        if !released {
            render::grenade_model(b, &(hm * Mat4::from_translation(vec3(0.0, 0.06, 0.0))), id - GRENADE_BASE);
        }
        b.cube(&hm, vec3(-0.036, -0.04, -0.04), vec3(0.036, 0.045, 0.04), SKIN);
        limb(b, hm.transform_point3(vec3(0.0, -0.04, -0.02)), view.transform_point3(vec3(0.36, -0.55, 0.16)), side, 0.042 * SCALE, sleeve);
        return;
    }

    if def.class == Class::Knife {
        // Karambit : lame dressée, griffe vers l'avant, flanc tourné vers le joueur.
        let frozen = g.debug & 16 != 0;
        let heavy = if frozen { g.debug & 32 != 0 } else { g.knife_heavy };
        let dur = if heavy { 0.55 } else { 0.32 };
        let a = if frozen { 0.4 } else { (g.knife_t / dur).min(1.0) };
        let s = if a < 1.0 { (a * PI).sin() } else { 0.0 };
        let mut pos = vec3(0.19, -0.15, 0.46) + off;
        ry -= 0.6;
        rx -= 1.3;
        if a < 1.0 && heavy {
            // Coup puissant : la griffe plonge vers l'avant, au centre de l'écran.
            pos += vec3(-0.13 * s, 0.03 * s, 0.34 * s);
            rx += 0.45 * s;
            ry += 0.15 * s;
        } else if a < 1.0 {
            // Coup rapide : balayage de droite à gauche puis retour.
            let x = if a < 0.6 { 0.1 - 0.42 * (a / 0.6) } else { -0.32 * (1.0 - (a - 0.6) / 0.4) };
            pos += vec3(x, -0.03 * s, 0.14 * s);
            rz -= 1.1 * s;
            rx += 0.35 * s;
        }
        let base = view * Mat4::from_translation(pos);
        let km = base * Mat4::from_rotation_y(ry) * Mat4::from_rotation_z(rz) * Mat4::from_rotation_x(rx) * Mat4::from_scale(Vec3::splat(1.2));
        render::weapon_model(b, &km, id, &WeaponPose::default());
        b.cube(&km, vec3(-0.032, -0.034, -0.05), vec3(0.032, 0.034, 0.04), SKIN);
        b.cube(&km, vec3(-0.034, -0.02, -0.03), vec3(0.0, 0.04, 0.03), SKIN);
        let arm = base * Mat4::from_rotation_y(ry * 0.5) * Mat4::from_rotation_x(-0.45 + (rx + 1.3) * 0.4);
        b.cube(&arm, vec3(-0.04, -0.015, -0.6), vec3(0.045, 0.07, -0.06), sleeve);
        return;
    }

    // Position de repos, recul en profondeur et relevé du canon selon l'arme.
    let (base, kz, kr) = match def.class {
        Class::Pistol => (vec3(0.16, -0.165, 0.5), 0.03, 0.16),
        Class::Smg => (vec3(0.16, -0.18, 0.44), 0.035, 0.05),
        Class::Rifle => (vec3(0.16, -0.19, 0.4), 0.045, 0.06),
        Class::Sniper => (vec3(0.16, -0.19, 0.38), 0.1, 0.12),
        _ => (vec3(0.17, -0.21, 0.4), 0.04, 0.05),
    };

    // Rechargement : arme inclinée, chargeur retiré puis remplacé, culasse armée.
    let r = if g.debug_f > 0.0 {
        g.debug_f
    } else if g.reload_t > 0.0 {
        1.0 - g.reload_t / def.reload.max(0.1)
    } else {
        0.0
    };
    let rp = reload_pose(r);
    let seat = if r > 0.0 { 0.014 * (-((r - 0.69) / 0.025).powi(2)).exp() } else { 0.0 };
    let k = va.kick;
    off += vec3(-0.06 * rp.tilt, 0.08 * rp.tilt + seat, -kz * k - 0.04 * rp.tilt - 0.03 * rp.slide);
    rx += -kr * k - 0.34 * rp.tilt;
    ry += -0.22 * rp.tilt;
    rz += va.roll * 0.035 * k - 0.55 * rp.tilt;
    let mut slide = rp.slide;
    if def.scope && g.reload_t <= 0.0 && g.fire_cd > 0.0 {
        // Fusil à verrou : la culasse est manœuvrée entre deux tirs.
        let u = 1.0 - g.fire_cd / (60.0 / def.rpm);
        let w = ss(0.25, 0.42, u) * (1.0 - ss(0.62, 0.82, u));
        slide = slide.max(w);
        rz += 0.22 * w;
        rx -= 0.07 * w;
        off.y -= 0.025 * w;
    }
    if def.class == Class::Pistol {
        slide = slide.max(k);
        // Chargeur vide : la culasse reste en arrière.
        if g.debug_weapon == 0 && g.slot < 2 && g.ammo[g.slot].mag == 0 && r < 0.78 {
            slide = 1.0;
        }
    }
    let vm = view * Mat4::from_translation(base + off) * Mat4::from_rotation_y(ry) * Mat4::from_rotation_x(rx) * Mat4::from_rotation_z(rz);
    render::weapon_model(b, &vm, id, &WeaponPose { mag: rp.mag, mag_on: rp.mag_on, slide });

    // Main droite sur la poignée, avant-bras vers le coin de l'écran.
    let gr = grip(id);
    let w = 0.042 * SCALE;
    b.cube(&vm, gr.hand - vec3(0.03, 0.05, 0.034), gr.hand + vec3(0.03, 0.04, 0.03), SKIN);
    limb(b, vm.transform_point3(gr.hand + vec3(0.0, -0.03, -0.03)), view.transform_point3(vec3(0.36, -0.55, 0.16)), side, w, sleeve);

    // Main gauche : sur le garde-main, puis elle accompagne le chargeur et arme la culasse.
    let mag = if id == 6 { vec3(rp.mag.x, -rp.mag.y, rp.mag.z) } else { rp.mag };
    let rack = gr.rack + vec3(-0.02, 0.0, -0.06 * rp.slide);
    let lp = (gr.fore.lerp(gr.mag + mag, rp.follow) + vec3(-0.14, -0.3, -0.12) * rp.away).lerp(rack, rp.rack);
    b.cube(&vm, lp - vec3(0.032, 0.03, 0.04), lp + vec3(0.032, 0.035, 0.04), SKIN);
    limb(b, vm.transform_point3(lp + vec3(-0.01, -0.03, -0.02)), view.transform_point3(vec3(-0.2, -0.58, 0.22)), side, w, sleeve);

    if g.muzzle_t > 0.0 {
        let fm = vm * Mat4::from_translation(vec3(0.0, 0.03, gr.muzzle));
        render::view_flash(b, &fm, va.flash, 0.9 + 0.4 * va.roll.abs());
    }
}
