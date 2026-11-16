//! Code partagé entre le client et le serveur : monde voxel, carte, armes,
//! physique du joueur et protocole réseau.

pub mod grenades;
pub mod map;
pub mod net;
pub mod physics;
pub mod protocol;
pub mod voxel;
pub mod weapons;

pub use glam;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

/// Petit générateur pseudo-aléatoire (xorshift64*), suffisant pour le jeu.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng((seed ^ 0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn from_time() -> Self {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        Self::new(n)
    }

    pub fn u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    /// Flottant uniforme dans [0, 1[.
    pub fn f32(&mut self) -> f32 {
        (self.u32() >> 8) as f32 / 16_777_216.0
    }

    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f32()
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            self.u32() as usize % n
        }
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }
}

/// Direction de visée. Lacet 0 = +Z, tangage positif = vers le haut.
pub fn dir_from_angles(yaw: f32, pitch: f32) -> Vec3 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    Vec3::new(sy * cp, sp, cy * cp)
}

/// Plus petit écart signé pour aller de `from` à `to`.
pub fn angle_diff(from: f32, to: f32) -> f32 {
    let mut d = (to - from) % TAU;
    if d > PI {
        d -= TAU;
    }
    if d < -PI {
        d += TAU;
    }
    d
}

/// Dévie `dir` aléatoirement dans un cône de demi-angle `angle` (radians).
pub fn perturb(dir: Vec3, angle: f32, rng: &mut Rng) -> Vec3 {
    if angle <= 0.0 {
        return dir;
    }
    let right = if dir.y.abs() > 0.99 { Vec3::X } else { dir.cross(Vec3::Y).normalize() };
    let up = right.cross(dir);
    let a = rng.f32() * TAU;
    let r = angle.tan() * rng.f32().sqrt();
    (dir + right * (a.cos() * r) + up * (a.sin() * r)).normalize()
}

/// Distance horizontale entre deux points.
pub fn hdist(a: Vec3, b: Vec3) -> f32 {
    let (dx, dz) = (a.x - b.x, a.z - b.z);
    (dx * dx + dz * dz).sqrt()
}
