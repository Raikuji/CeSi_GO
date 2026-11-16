use glam::{IVec3, Vec3};

/// Taille d'un voxel en mètres.
pub const VS: f32 = 0.5;
/// Bloc renvoyé hors des limites de la carte (côtés et dessous).
pub const VOID: u8 = 255;

pub const SAND: u8 = 1;
pub const ASPHALT: u8 = 2;
pub const CONCRETE: u8 = 3;
pub const BRICK: u8 = 4;
pub const PLASTER: u8 = 5;
pub const STONE: u8 = 6;
pub const CRATE: u8 = 7;
pub const BLUE: u8 = 8;
pub const MARK: u8 = 9;
pub const GRASS: u8 = 10;
pub const ROOF: u8 = 11;
pub const PAVE: u8 = 12;
pub const YELLOW: u8 = 13;
pub const GREEN: u8 = 14;
pub const DARKWOOD: u8 = 15;
pub const WHITE: u8 = 16;
pub const METAL: u8 = 17;
pub const RUST: u8 = 18;
pub const SNOW: u8 = 19;
pub const ICE: u8 = 20;
pub const DARKSTONE: u8 = 21;
pub const PINE: u8 = 22;

const PALETTE: [[u8; 3]; 23] = [
    [0, 0, 0],
    [214, 190, 142],
    [98, 98, 104],
    [152, 152, 150],
    [168, 88, 66],
    [218, 208, 186],
    [92, 90, 98],
    [166, 120, 62],
    [62, 98, 156],
    [204, 74, 52],
    [104, 156, 76],
    [62, 56, 58],
    [178, 172, 162],
    [228, 192, 52],
    [72, 124, 84],
    [112, 78, 42],
    [236, 236, 236],
    [138, 148, 160],
    [176, 84, 54],
    [224, 231, 240],
    [150, 196, 222],
    [72, 74, 86],
    [46, 92, 62],
];

pub fn block_color(b: u8) -> [u8; 3] {
    PALETTE.get(b as usize).copied().unwrap_or([40, 40, 40])
}

pub struct World {
    pub sx: i32,
    pub sy: i32,
    pub sz: i32,
    data: Vec<u8>,
}

pub struct RayHit {
    pub dist: f32,
    pub block: u8,
    pub normal: IVec3,
    pub cell: IVec3,
}

impl World {
    pub fn new(sx: i32, sy: i32, sz: i32) -> Self {
        World { sx, sy, sz, data: vec![0; (sx * sy * sz) as usize] }
    }

    /// Bloc à la position donnée. Au-dessus de la carte : air ; ailleurs hors limites : `VOID`.
    #[inline]
    pub fn block(&self, x: i32, y: i32, z: i32) -> u8 {
        if y >= self.sy {
            return 0;
        }
        if x < 0 || y < 0 || z < 0 || x >= self.sx || z >= self.sz {
            return VOID;
        }
        self.data[((y * self.sz + z) * self.sx + x) as usize]
    }

    #[inline]
    pub fn solid(&self, x: i32, y: i32, z: i32) -> bool {
        self.block(x, y, z) != 0
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, b: u8) {
        if x < 0 || y < 0 || z < 0 || x >= self.sx || y >= self.sy || z >= self.sz {
            return;
        }
        self.data[((y * self.sz + z) * self.sx + x) as usize] = b;
    }

    /// Remplit un pavé (bornes incluses).
    #[allow(clippy::too_many_arguments)]
    pub fn fill(&mut self, x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32, b: u8) {
        for y in y0..=y1 {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    self.set(x, y, z, b);
                }
            }
        }
    }

    /// Vrai si la boîte (en mètres) touche un voxel plein.
    pub fn aabb_hits(&self, min: Vec3, max: Vec3) -> bool {
        let x0 = (min.x / VS).floor() as i32;
        let y0 = (min.y / VS).floor() as i32;
        let z0 = (min.z / VS).floor() as i32;
        let x1 = (max.x / VS).floor() as i32;
        let y1 = (max.y / VS).floor() as i32;
        let z1 = (max.z / VS).floor() as i32;
        for y in y0..=y1 {
            for z in z0..=z1 {
                for x in x0..=x1 {
                    if self.solid(x, y, z) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Lancer de rayon (DDA). `dir` doit être normalisé, distances en mètres.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit> {
        let p = (origin / VS).to_array();
        let d = dir.to_array();
        let mut c = [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32];
        if self.solid(c[0], c[1], c[2]) {
            return Some(RayHit {
                dist: 0.0,
                block: self.block(c[0], c[1], c[2]),
                normal: IVec3::ZERO,
                cell: IVec3::from_array(c),
            });
        }
        let mut step = [0i32; 3];
        let mut t_max = [f32::INFINITY; 3];
        let mut t_delta = [f32::INFINITY; 3];
        for i in 0..3 {
            if d[i] > 1e-8 {
                step[i] = 1;
                t_delta[i] = 1.0 / d[i];
                t_max[i] = ((c[i] + 1) as f32 - p[i]) / d[i];
            } else if d[i] < -1e-8 {
                step[i] = -1;
                t_delta[i] = -1.0 / d[i];
                t_max[i] = (p[i] - c[i] as f32) / -d[i];
            }
        }
        let max_t = max_dist / VS;
        loop {
            let axis = if t_max[0] <= t_max[1] && t_max[0] <= t_max[2] {
                0
            } else if t_max[1] <= t_max[2] {
                1
            } else {
                2
            };
            let t = t_max[axis];
            if t > max_t || !t.is_finite() {
                return None;
            }
            c[axis] += step[axis];
            t_max[axis] += t_delta[axis];
            if c[1] >= self.sy && step[1] >= 0 {
                return None;
            }
            let b = self.block(c[0], c[1], c[2]);
            if b != 0 {
                let mut n = [0i32; 3];
                n[axis] = -step[axis];
                return Some(RayHit {
                    dist: t * VS,
                    block: b,
                    normal: IVec3::from_array(n),
                    cell: IVec3::from_array(c),
                });
            }
        }
    }
}
