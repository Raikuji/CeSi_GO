//! Déplacement du joueur : accélération/friction à la Quake, saut, accroupi,
//! collisions boîte contre voxels avec montée automatique des marches.

use crate::voxel::{World, VS};
use glam::{vec3, Vec3};

pub const RADIUS: f32 = 0.3;
pub const HEIGHT_STAND: f32 = 1.8;
pub const HEIGHT_CROUCH: f32 = 1.3;
pub const EYE_STAND: f32 = 1.62;
pub const EYE_CROUCH: f32 = 1.12;
/// Vitesse de course couteau en main (m/s).
pub const BASE_SPEED: f32 = 5.6;

const GRAVITY: f32 = 15.5;
const JUMP_SPEED: f32 = 5.95;
const ACCEL: f32 = 6.0;
const AIR_ACCEL: f32 = 12.0;
const AIR_CAP: f32 = 0.6;
const FRICTION: f32 = 5.2;
const STOP_SPEED: f32 = 1.6;
const STEP: f32 = VS + 0.02;
const EPS: f32 = 0.001;

#[derive(Clone, Copy, Default)]
pub struct MoveInput {
    pub fwd: f32,
    pub side: f32,
    pub yaw: f32,
    /// Vrai uniquement à l'instant où la touche est enfoncée.
    pub jump: bool,
    pub crouch: bool,
    pub walk: bool,
}

#[derive(Clone, Copy)]
pub struct Body {
    /// Position des pieds (centre de la boîte).
    pub pos: Vec3,
    pub vel: Vec3,
    pub on_ground: bool,
    pub crouched: bool,
    /// Progression de l'accroupissement (0 debout, 1 accroupi) pour la caméra.
    pub crouch_t: f32,
}

pub fn eye_height(crouch_t: f32) -> f32 {
    EYE_STAND + (EYE_CROUCH - EYE_STAND) * crouch_t
}

impl Body {
    pub fn new(pos: Vec3) -> Self {
        Body { pos, vel: Vec3::ZERO, on_ground: false, crouched: false, crouch_t: 0.0 }
    }

    pub fn height(&self) -> f32 {
        if self.crouched {
            HEIGHT_CROUCH
        } else {
            HEIGHT_STAND
        }
    }

    pub fn eye(&self) -> Vec3 {
        self.pos + Vec3::Y * eye_height(self.crouch_t)
    }

    pub fn hspeed(&self) -> f32 {
        (self.vel.x * self.vel.x + self.vel.z * self.vel.z).sqrt()
    }
}

fn hits(w: &World, p: Vec3, h: f32) -> bool {
    w.aabb_hits(vec3(p.x - RADIUS, p.y, p.z - RADIUS), vec3(p.x + RADIUS, p.y + h, p.z + RADIUS))
}

fn accelerate(vel: &mut Vec3, dir: Vec3, wish_speed: f32, cap: f32, accel: f32, dt: f32) {
    let cur = vel.dot(dir);
    let add = wish_speed.min(cap) - cur;
    if add <= 0.0 {
        return;
    }
    *vel += dir * (accel * dt * wish_speed).min(add);
}

/// Déplacement horizontal avec glissement le long des murs.
fn slide(w: &World, pos: Vec3, d: Vec3, h: f32) -> (Vec3, bool, bool) {
    let mut p = pos;
    let (mut hx, mut hz) = (false, false);
    if d.x != 0.0 {
        let mut n = p;
        n.x += d.x;
        if hits(w, n, h) {
            hx = true;
            n.x = if d.x > 0.0 {
                ((n.x + RADIUS) / VS).floor() * VS - RADIUS - EPS
            } else {
                (((n.x - RADIUS) / VS).floor() + 1.0) * VS + RADIUS + EPS
            };
            if hits(w, n, h) {
                n.x = p.x;
            }
        }
        p = n;
    }
    if d.z != 0.0 {
        let mut n = p;
        n.z += d.z;
        if hits(w, n, h) {
            hz = true;
            n.z = if d.z > 0.0 {
                ((n.z + RADIUS) / VS).floor() * VS - RADIUS - EPS
            } else {
                (((n.z - RADIUS) / VS).floor() + 1.0) * VS + RADIUS + EPS
            };
            if hits(w, n, h) {
                n.z = p.z;
            }
        }
        p = n;
    }
    (p, hx, hz)
}

/// Déplacement vertical ; renvoie la position et si un obstacle a été touché.
fn move_y(w: &World, pos: Vec3, dy: f32, h: f32) -> (Vec3, bool) {
    let mut n = pos;
    n.y += dy;
    if !hits(w, n, h) {
        return (n, false);
    }
    n.y = if dy < 0.0 {
        ((n.y / VS).floor() + 1.0) * VS + EPS
    } else {
        ((n.y + h) / VS).floor() * VS - h - EPS
    };
    if hits(w, n, h) {
        n.y = pos.y;
    }
    (n, true)
}

/// Avance la simulation de `dt` secondes (découpée en pas de 1/60 s au plus).
pub fn step(w: &World, b: &mut Body, inp: &MoveInput, max_speed: f32, dt: f32) {
    let n = (dt / (1.0 / 60.0)).ceil().clamp(1.0, 8.0) as i32;
    let sub = dt.min(0.15) / n as f32;
    let mut i = *inp;
    for _ in 0..n {
        step_once(w, b, &i, max_speed, sub);
        i.jump = false;
    }
}

fn step_once(w: &World, b: &mut Body, inp: &MoveInput, max_speed: f32, dt: f32) {
    if inp.crouch {
        b.crouched = true;
    } else if b.crouched && !hits(w, b.pos, HEIGHT_STAND) {
        b.crouched = false;
    }
    let target = if b.crouched { 1.0 } else { 0.0 };
    b.crouch_t += (target - b.crouch_t).clamp(-dt * 8.0, dt * 8.0);
    let h = b.height();

    let (s, c) = inp.yaw.sin_cos();
    let wish = vec3(s, 0.0, c) * inp.fwd + vec3(-c, 0.0, s) * inp.side;
    let wl = wish.length();
    let wish_dir = if wl > 1e-4 { wish / wl } else { Vec3::ZERO };
    let mut wish_speed = max_speed * wl.min(1.0);
    if b.crouched {
        wish_speed *= 0.34;
    } else if inp.walk {
        wish_speed *= 0.52;
    }

    if b.on_ground {
        let sp = b.hspeed();
        if sp > 1e-4 {
            let drop = sp.max(STOP_SPEED) * FRICTION * dt;
            let k = (sp - drop).max(0.0) / sp;
            b.vel.x *= k;
            b.vel.z *= k;
        }
        accelerate(&mut b.vel, wish_dir, wish_speed, wish_speed, ACCEL, dt);
        b.vel.y = 0.0;
        if inp.jump {
            b.vel.y = JUMP_SPEED;
            b.on_ground = false;
        }
    } else {
        accelerate(&mut b.vel, wish_dir, wish_speed, AIR_CAP, AIR_ACCEL, dt);
        b.vel.y -= GRAVITY * dt;
    }

    // Horizontal, avec montée de marche si on est au sol.
    let d = vec3(b.vel.x * dt, 0.0, b.vel.z * dt);
    let (p1, mut hx, mut hz) = slide(w, b.pos, d, h);
    let mut np = p1;
    if (hx || hz) && b.on_ground {
        let up = b.pos + Vec3::Y * STEP;
        if !hits(w, up, h) {
            let (p2, h2x, h2z) = slide(w, up, d, h);
            let (p2, _) = move_y(w, p2, -STEP, h);
            let d1 = (p1.x - b.pos.x).powi(2) + (p1.z - b.pos.z).powi(2);
            let d2 = (p2.x - b.pos.x).powi(2) + (p2.z - b.pos.z).powi(2);
            if d2 > d1 + 1e-7 {
                np = p2;
                hx = h2x;
                hz = h2z;
            }
        }
    }
    if hx {
        b.vel.x = 0.0;
    }
    if hz {
        b.vel.z = 0.0;
    }
    b.pos = np;

    // Vertical.
    if b.on_ground {
        // Reste collé au sol (descente de marches) ; sinon début de chute.
        let (p, landed) = move_y(w, b.pos, -STEP, h);
        if landed {
            b.pos = p;
        } else {
            b.on_ground = false;
        }
    } else {
        let (p, hit) = move_y(w, b.pos, b.vel.y * dt, h);
        b.pos = p;
        if hit {
            if b.vel.y < 0.0 {
                b.on_ground = true;
            }
            b.vel.y = 0.0;
        }
    }
}
