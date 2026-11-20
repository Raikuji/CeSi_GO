//! Rendu 3D : maillage du monde voxel (fusion des faces + occlusion ambiante),
//! shader, et modèles en boîtes des joueurs, des armes et de la bombe.

use cesi_common::grenades::*;
use cesi_common::{angle_diff, dir_from_angles};
use cesi_common::protocol::Team;
use cesi_common::voxel::{block_color, World, VOID, VS};
use cesi_common::weapons::{weapon, Class};
use macroquad::prelude::*;

pub const SKY: Color = Color::new(0.56, 0.74, 0.92, 1.0);

const VERT_SRC: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec4 color;
varying vec2 uv;
varying vec3 wpos;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    vec4 w = Model * vec4(position, 1.0);
    gl_Position = Projection * w;
    color = color0 / 255.0;
    uv = texcoord;
    wpos = w.xyz;
}"#;

// uv.x >= 0 : face du monde (indice de la normale) -> grain et joints par voxel.
// uv.x < 0  : objet dynamique, couleur unie.
const FRAG_SRC: &str = r#"#version 100
precision highp float;
varying lowp vec4 color;
varying vec2 uv;
varying vec3 wpos;
uniform vec3 CamPos;
uniform vec3 FogColor;
float hash(vec3 p) {
    return fract(sin(dot(p, vec3(12.9898, 78.233, 37.719))) * 43758.5453);
}
void main() {
    vec3 c = color.rgb;
    float dist = length(wpos - CamPos);
    if (uv.x > -0.5) {
        vec3 n = vec3(0.0);
        if (uv.x < 0.5) n.x = 1.0;
        else if (uv.x < 1.5) n.x = -1.0;
        else if (uv.x < 2.5) n.y = 1.0;
        else if (uv.x < 3.5) n.y = -1.0;
        else if (uv.x < 4.5) n.z = 1.0;
        else n.z = -1.0;
        vec3 p = wpos * 2.0;
        float h = hash(floor(p - n * 0.5));
        c *= 0.90 + 0.20 * h;
        vec3 f = fract(p);
        vec3 e = min(f, 1.0 - f) + abs(n);
        float edge = min(e.x, min(e.y, e.z));
        float fade = clamp(dist / 40.0, 0.0, 1.0);
        c *= mix(mix(0.80, 1.0, smoothstep(0.0, 0.07, edge)), 1.0, fade);
    }
    float fog = clamp((dist - 45.0) / 140.0, 0.0, 0.5);
    gl_FragColor = vec4(mix(c, FogColor, fog), 1.0);
}"#;

/// Éclairage par face : +X, -X, +Y, -Y, +Z, -Z.
const FACE_SHADE: [f32; 6] = [0.80, 0.70, 1.0, 0.50, 0.88, 0.62];
const AO_SHADE: [f32; 4] = [0.52, 0.70, 0.86, 1.0];
const CHUNK: i32 = 16;
const MAX_QUADS: usize = 1800;

struct Region {
    center: Vec3,
    radius: f32,
    meshes: Vec<Mesh>,
}

pub struct WorldRender {
    regions: Vec<Region>,
    pub material: Material,
}

struct MeshBuilder {
    verts: Vec<Vertex>,
    idx: Vec<u16>,
    out: Vec<Mesh>,
}

impl MeshBuilder {
    fn flush(&mut self) {
        if !self.verts.is_empty() {
            self.out.push(Mesh {
                vertices: std::mem::take(&mut self.verts),
                indices: std::mem::take(&mut self.idx),
                texture: None,
            });
        }
    }

    fn quad(&mut self, pts: [[f32; 3]; 4], ao: [u32; 4], block: u8, face: usize) {
        if self.verts.len() >= MAX_QUADS * 4 {
            self.flush();
        }
        let base = self.verts.len() as u16;
        let rgb = block_color(block);
        for k in 0..4 {
            let s = FACE_SHADE[face] * AO_SHADE[ao[k] as usize];
            self.verts.push(Vertex {
                position: vec3(pts[k][0] * VS, pts[k][1] * VS, pts[k][2] * VS),
                uv: vec2(face as f32, 0.0),
                color: [(rgb[0] as f32 * s) as u8, (rgb[1] as f32 * s) as u8, (rgb[2] as f32 * s) as u8, 255],
                normal: Vec4::ZERO,
            });
        }
        // Diagonale choisie pour une interpolation symétrique de l'occlusion.
        let order: [u16; 6] = if ao[0] + ao[2] > ao[1] + ao[3] { [0, 1, 3, 1, 2, 3] } else { [0, 1, 2, 0, 2, 3] };
        self.idx.extend(order.iter().map(|i| base + i));
    }
}

fn mesh_chunk(w: &World, lo: [i32; 3], hi: [i32; 3], mb: &mut MeshBuilder) {
    for a in 0..3 {
        let (u, v) = ((a + 1) % 3, (a + 2) % 3);
        let (nu, nv) = ((hi[u] - lo[u]) as usize, (hi[v] - lo[v]) as usize);
        let mut mask = vec![0u32; nu * nv];
        for side in 0..2usize {
            let sign = if side == 0 { 1 } else { -1 };
            for s in lo[a]..hi[a] {
                let mut any = false;
                for j in 0..nv {
                    for i in 0..nu {
                        let mut p = [0i32; 3];
                        p[a] = s;
                        p[u] = lo[u] + i as i32;
                        p[v] = lo[v] + j as i32;
                        let b = w.block(p[0], p[1], p[2]);
                        let mut q = p;
                        q[a] += sign;
                        let mut key = 0;
                        if b != 0 && b != VOID && w.block(q[0], q[1], q[2]) == 0 {
                            let occ = |du: i32, dv: i32| -> u32 {
                                let mut r = q;
                                r[u] += du;
                                r[v] += dv;
                                (w.block(r[0], r[1], r[2]) != 0) as u32
                            };
                            let ao = |du: i32, dv: i32| -> u32 {
                                let (s1, s2, c) = (occ(du, 0), occ(0, dv), occ(du, dv));
                                if s1 == 1 && s2 == 1 {
                                    0
                                } else {
                                    3 - (s1 + s2 + c)
                                }
                            };
                            key = b as u32 | ao(-1, -1) << 8 | ao(1, -1) << 10 | ao(1, 1) << 12 | ao(-1, 1) << 14 | 1 << 16;
                            any = true;
                        }
                        mask[j * nu + i] = key;
                    }
                }
                if !any {
                    continue;
                }
                for j in 0..nv {
                    let mut i = 0;
                    while i < nu {
                        let k = mask[j * nu + i];
                        if k == 0 {
                            i += 1;
                            continue;
                        }
                        let ao = [(k >> 8) & 3, (k >> 10) & 3, (k >> 12) & 3, (k >> 14) & 3];
                        let (mut wd, mut ht) = (1usize, 1usize);
                        // Seules les faces à occlusion uniforme sont fusionnées.
                        if ao[0] == ao[1] && ao[1] == ao[2] && ao[2] == ao[3] {
                            while i + wd < nu && mask[j * nu + i + wd] == k {
                                wd += 1;
                            }
                            'grow: while j + ht < nv {
                                for x in 0..wd {
                                    if mask[(j + ht) * nu + i + x] != k {
                                        break 'grow;
                                    }
                                }
                                ht += 1;
                            }
                        }
                        for y in 0..ht {
                            for x in 0..wd {
                                mask[(j + y) * nu + i + x] = 0;
                            }
                        }
                        let plane = (s + if sign > 0 { 1 } else { 0 }) as f32;
                        let corner = |cu: usize, cv: usize| -> [f32; 3] {
                            let mut c = [0f32; 3];
                            c[a] = plane;
                            c[u] = (lo[u] + (i + cu) as i32) as f32;
                            c[v] = (lo[v] + (j + cv) as i32) as f32;
                            c
                        };
                        let pts = [corner(0, 0), corner(wd, 0), corner(wd, ht), corner(0, ht)];
                        mb.quad(pts, ao, (k & 255) as u8, a * 2 + side);
                        i += wd;
                    }
                }
            }
        }
    }
}

impl WorldRender {
    pub fn new(world: &World) -> WorldRender {
        let material = load_material(
            ShaderSource::Glsl { vertex: VERT_SRC, fragment: FRAG_SRC },
            MaterialParams {
                pipeline_params: PipelineParams {
                    depth_write: true,
                    depth_test: Comparison::LessOrEqual,
                    ..Default::default()
                },
                uniforms: vec![
                    UniformDesc::new("CamPos", UniformType::Float3),
                    UniformDesc::new("FogColor", UniformType::Float3),
                ],
                ..Default::default()
            },
        )
        .expect("compilation du shader");
        let mut wr = WorldRender { regions: Vec::new(), material };
        wr.set_world(world);
        wr
    }

    /// Remaille le monde (au chargement d'une autre carte).
    pub fn set_world(&mut self, world: &World) {
        let mut regions = Vec::new();
        let half_h = world.sy as f32 * VS / 2.0;
        for cz in (0..world.sz).step_by(CHUNK as usize) {
            for cx in (0..world.sx).step_by(CHUNK as usize) {
                let mut mb = MeshBuilder { verts: Vec::new(), idx: Vec::new(), out: Vec::new() };
                mesh_chunk(world, [cx, 0, cz], [cx + CHUNK, world.sy, cz + CHUNK], &mut mb);
                mb.flush();
                if !mb.out.is_empty() {
                    let half = CHUNK as f32 * VS / 2.0;
                    regions.push(Region {
                        center: vec3(cx as f32 * VS + half, half_h, cz as f32 * VS + half),
                        radius: (half * half * 2.0 + half_h * half_h).sqrt(),
                        meshes: mb.out,
                    });
                }
            }
        }
        self.regions = regions;
    }

    /// Active le shader du monde pour la caméra donnée.
    pub fn begin(&self, eye: Vec3) {
        gl_use_material(&self.material);
        self.material.set_uniform("CamPos", [eye.x, eye.y, eye.z]);
        self.material.set_uniform("FogColor", [SKY.r, SKY.g, SKY.b]);
    }

    /// Dessine les régions visibles (test sphère contre cône de vision).
    pub fn draw(&self, eye: Vec3, fwd: Vec3, fovy: f32) {
        let aspect = screen_width() / screen_height().max(1.0);
        let half = ((fovy / 2.0).tan() * (1.0 + aspect * aspect).sqrt()).atan();
        let (sin, cos) = half.sin_cos();
        for r in &self.regions {
            let d = r.center - eye;
            let z = d.dot(fwd);
            let lateral = (d.length_squared() - z * z).max(0.0).sqrt();
            if lateral * cos - z * sin > r.radius {
                continue;
            }
            for m in &r.meshes {
                draw_mesh(m);
            }
        }
    }
}

// ---------------------------------------------------------------------------

/// Tampon de boîtes orientées pour les objets dynamiques.
pub struct Batch {
    mesh: Mesh,
}

const FACES: [([usize; 4], [f32; 3]); 6] = [
    ([1, 3, 7, 5], [1.0, 0.0, 0.0]),
    ([0, 4, 6, 2], [-1.0, 0.0, 0.0]),
    ([2, 6, 7, 3], [0.0, 1.0, 0.0]),
    ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
    ([4, 5, 7, 6], [0.0, 0.0, 1.0]),
    ([0, 2, 3, 1], [0.0, 0.0, -1.0]),
];

impl Batch {
    pub fn new() -> Batch {
        Batch { mesh: Mesh { vertices: Vec::with_capacity(6000), indices: Vec::with_capacity(9000), texture: None } }
    }

    /// Ajoute une boîte (coins `min`/`max` en repère local) transformée par `m`.
    pub fn cube(&mut self, m: &Mat4, min: Vec3, max: Vec3, color: Color) {
        if self.mesh.vertices.len() + 24 > 5000 {
            self.flush();
        }
        let corner = |i: usize| {
            m.transform_point3(vec3(
                if i & 1 != 0 { max.x } else { min.x },
                if i & 2 != 0 { max.y } else { min.y },
                if i & 4 != 0 { max.z } else { min.z },
            ))
        };
        let pts: [Vec3; 8] = std::array::from_fn(corner);
        for (ids, n) in FACES {
            let n = m.transform_vector3(Vec3::from_array(n)).normalize_or_zero();
            let s = (0.74 + 0.26 * n.y + 0.08 * n.z + 0.04 * n.x).clamp(0.35, 1.0);
            let col = [(color.r * s * 255.0) as u8, (color.g * s * 255.0) as u8, (color.b * s * 255.0) as u8, 255];
            let base = self.mesh.vertices.len() as u16;
            for i in ids {
                self.mesh.vertices.push(Vertex { position: pts[i], uv: vec2(-1.0, 0.0), color: col, normal: Vec4::ZERO });
            }
            self.mesh.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    pub fn flush(&mut self) {
        if !self.mesh.vertices.is_empty() {
            draw_mesh(&self.mesh);
            self.mesh.vertices.clear();
            self.mesh.indices.clear();
        }
    }
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}

/// Repère local (droite, haut, avant) d'un personnage tourné de `yaw`.
pub fn basis(pos: Vec3, yaw: f32) -> Mat4 {
    let (s, c) = yaw.sin_cos();
    Mat4::from_cols(vec4(-c, 0.0, s, 0.0), vec4(0.0, 1.0, 0.0, 0.0), vec4(s, 0.0, c, 0.0), pos.extend(1.0))
}

/// Repère de la caméra (droite, haut, avant).
pub fn view_basis(eye: Vec3, yaw: f32, pitch: f32) -> Mat4 {
    let f = dir_from_angles(yaw, pitch);
    let r = f.cross(Vec3::Y).normalize();
    let u = r.cross(f);
    Mat4::from_cols(r.extend(0.0), u.extend(0.0), f.extend(0.0), eye.extend(1.0))
}

/// Joueur vu par les autres, déjà interpolé.
#[derive(Clone)]
pub struct PlayerView {
    pub id: u8,
    pub team: Team,
    pub alive: bool,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub crouch: bool,
    pub weapon: u8,
    pub has_bomb: bool,
    pub speed: f32,
    pub reloading: bool,
    pub air: bool,
    /// 0 rien, 1 pose la bombe, 2 désamorce.
    pub action: u8,
}

/// Transition douce entre 0 (x <= a) et 1 (x >= b).
pub fn ss(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// État d'animation d'un personnage, lissé d'une image à l'autre.
#[derive(Clone, Copy, Default)]
pub struct Puppet {
    /// Phase du cycle de marche (un tour = deux pas).
    pub phase: f32,
    /// Intensité du déplacement, de 0 (immobile) à 1 (course).
    pub moving: f32,
    /// Direction du déplacement par rapport au regard (0 = avant, positif = droite).
    pub dir: f32,
    pub crouch: f32,
    pub air: f32,
    /// Agenouillé pour poser ou désamorcer la bombe.
    pub kneel: f32,
    /// Progression du rechargement (0 = aucun).
    pub reload: f32,
    pub reload_t: f32,
    /// Recul du dernier tir, de 1 à 0.
    pub shot: f32,
}

impl Puppet {
    pub fn update(&mut self, v: &PlayerView, dt: f32) {
        let k = |rate: f32| (rate * dt).min(1.0);
        let sp = (v.vel.x * v.vel.x + v.vel.z * v.vel.z).sqrt();
        self.moving += ((sp / 4.5).min(1.0) - self.moving) * k(10.0);
        self.phase = (self.phase + sp * dt * 3.0) % (std::f32::consts::TAU * 16.0);
        if sp > 0.6 {
            let (s, c) = v.yaw.sin_cos();
            let (f, r) = (v.vel.x * s + v.vel.z * c, -v.vel.x * c + v.vel.z * s);
            self.dir += angle_diff(self.dir, r.atan2(f)) * k(12.0);
        }
        self.crouch += (v.crouch as u8 as f32 - self.crouch) * k(11.0);
        self.air += (v.air as u8 as f32 - self.air) * k(12.0);
        self.kneel += ((v.action != 0) as u8 as f32 - self.kneel) * k(8.0);
        if v.reloading {
            self.reload_t += dt;
            self.reload = (self.reload_t / weapon(v.weapon).reload.max(0.5)).clamp(0.001, 0.999);
        } else {
            self.reload_t = 0.0;
            self.reload = 0.0;
        }
        self.shot = (self.shot - dt * 7.0).max(0.0);
    }
}

/// Position des pièces mobiles d'une arme.
#[derive(Clone, Copy)]
pub struct WeaponPose {
    /// Décalage du chargeur par rapport à son logement.
    pub mag: Vec3,
    pub mag_on: bool,
    /// Recul de la culasse ou du levier d'armement, de 0 à 1.
    pub slide: f32,
}

impl Default for WeaponPose {
    fn default() -> Self {
        WeaponPose { mag: Vec3::ZERO, mag_on: true, slide: 0.0 }
    }
}

/// Étapes du rechargement pour une progression `r` de 0 à 1 :
/// inclinaison de l'arme, chargeur retiré, nouveau chargeur engagé, armement.
pub struct ReloadPose {
    /// Arme inclinée vers la main gauche (0 à 1).
    pub tilt: f32,
    pub mag: Vec3,
    pub mag_on: bool,
    pub slide: f32,
    /// La main gauche accompagne le chargeur (0 à 1).
    pub follow: f32,
    /// La main gauche est hors champ, partie chercher le chargeur neuf (0 à 1).
    pub away: f32,
    /// La main gauche manœuvre la culasse (0 à 1).
    pub rack: f32,
}

pub fn reload_pose(r: f32) -> ReloadPose {
    if r <= 0.0 || r >= 1.0 {
        return ReloadPose { tilt: 0.0, mag: Vec3::ZERO, mag_on: true, slide: 0.0, follow: 0.0, away: 0.0, rack: 0.0 };
    }
    let drop = if r < 0.43 { ss(0.14, 0.36, r) } else { 1.0 - ss(0.50, 0.68, r) };
    ReloadPose {
        tilt: ss(0.0, 0.14, r) * (1.0 - ss(0.90, 1.0, r)),
        mag: vec3(-0.03, -0.26, -0.02) * drop,
        mag_on: !(0.36..0.50).contains(&r),
        slide: if r > 0.78 && r < 0.92 { ((r - 0.78) / 0.14 * std::f32::consts::PI).sin() } else { 0.0 },
        follow: ss(0.05, 0.14, r) * (1.0 - ss(0.70, 0.78, r)),
        away: ss(0.34, 0.40, r) * (1.0 - ss(0.46, 0.54, r)),
        rack: ss(0.72, 0.79, r) * (1.0 - ss(0.91, 0.97, r)),
    }
}

/// Points de prise d'une arme, dans son repère.
pub struct Grip {
    /// Main droite, sur la poignée.
    pub hand: Vec3,
    /// Main gauche au repos (garde-main, ou sous la poignée pour un pistolet).
    pub fore: Vec3,
    /// Bas du chargeur.
    pub mag: Vec3,
    /// Levier d'armement ou arrière de la culasse.
    pub rack: Vec3,
    pub muzzle: f32,
}

pub fn grip(id: u8) -> Grip {
    match weapon(id).class {
        Class::Pistol => Grip {
            hand: vec3(0.0, -0.06, -0.028),
            fore: vec3(-0.03, -0.075, -0.02),
            mag: vec3(0.0, -0.13, -0.028),
            rack: vec3(0.0, 0.05, -0.05),
            muzzle: if id == 3 { 0.22 } else { 0.15 },
        },
        Class::Smg => Grip {
            hand: vec3(0.0, -0.075, -0.04),
            fore: vec3(0.0, -0.045, 0.17),
            mag: if id == 6 { vec3(0.0, 0.12, 0.07) } else { vec3(0.0, -0.21, 0.07) },
            rack: vec3(0.04, 0.045, 0.03),
            muzzle: 0.34,
        },
        Class::Rifle => Grip {
            hand: vec3(0.0, -0.08, -0.06),
            fore: vec3(0.0, -0.04, 0.32),
            mag: vec3(0.0, -0.23, 0.1),
            rack: vec3(0.045, 0.04, 0.0),
            muzzle: 0.68,
        },
        Class::Shotgun => Grip {
            hand: vec3(0.0, -0.08, -0.06),
            // Main gauche sur la pompe ; au rechargement, elle glisse les cartouches par-dessous.
            fore: vec3(0.0, -0.03, 0.33),
            mag: vec3(0.0, -0.07, 0.06),
            rack: vec3(0.0, -0.03, 0.26),
            muzzle: 0.72,
        },
        Class::Sniper => Grip {
            hand: vec3(0.0, -0.08, -0.08),
            fore: vec3(0.0, -0.045, 0.28),
            mag: vec3(0.0, -0.12, 0.1),
            rack: vec3(0.06, 0.05, -0.08),
            muzzle: 0.94,
        },
        _ => Grip {
            hand: vec3(0.0, -0.085, -0.08),
            fore: vec3(0.0, -0.055, 0.34),
            mag: vec3(0.0, -0.19, 0.09),
            rack: vec3(0.045, 0.05, 0.0),
            muzzle: 0.7,
        },
    }
}

fn team_colors(team: Team) -> (Color, Color, Color) {
    match team {
        Team::T => (rgb(196, 150, 78), rgb(92, 70, 48), rgb(58, 52, 48)),
        Team::CT => (rgb(58, 88, 140), rgb(40, 46, 62), rgb(46, 60, 84)),
    }
}

const SKIN: Color = Color::new(0.87, 0.68, 0.53, 1.0);
const SHOE: Color = Color::new(0.13, 0.12, 0.12, 1.0);

/// Membre : boîte entre deux points, orientée de façon stable grâce à `side`.
pub fn limb(b: &mut Batch, from: Vec3, to: Vec3, side: Vec3, w: f32, color: Color) {
    let d = to - from;
    let len = d.length();
    if len < 0.005 {
        return;
    }
    let f = d / len;
    let mut r = side - f * side.dot(f);
    if r.length_squared() < 1e-4 {
        r = if f.y.abs() > 0.9 { Vec3::X } else { f.cross(Vec3::Y) };
    }
    let r = r.normalize();
    let u = r.cross(f);
    let m = Mat4::from_cols(r.extend(0.0), u.extend(0.0), f.extend(0.0), from.extend(1.0));
    b.cube(&m, vec3(-w, -w, -w * 0.3), vec3(w, w, len + w * 0.3), color);
}

/// Cinématique inverse à deux segments dans un plan (z vers l'avant, y vers le haut).
/// Renvoie le genou (plié vers l'avant) et l'extrémité réellement atteinte.
fn ik2(l1: f32, l2: f32, tz: f32, ty: f32) -> ((f32, f32), (f32, f32)) {
    let d = (tz * tz + ty * ty).sqrt().clamp((l1 - l2).abs() + 0.02, l1 + l2 - 0.004);
    let base = tz.atan2(-ty);
    let a1 = ((l1 * l1 + d * d - l2 * l2) / (2.0 * l1 * d)).clamp(-1.0, 1.0).acos();
    let th = base + a1;
    ((l1 * th.sin(), -l1 * th.cos()), (d * base.sin(), -d * base.cos()))
}

/// Bras : épaule, coude (calculé) et main.
fn arm(b: &mut Batch, shoulder: Vec3, hand: Vec3, pole: Vec3, side: Vec3, sleeve: Color, w: f32) {
    let (l1, l2) = (0.30, 0.29);
    let d = hand - shoulder;
    let len = d.length().clamp(0.06, l1 + l2 - 0.01);
    let dir = d.normalize_or_zero();
    let hand = shoulder + dir * len;
    let a = (l1 * l1 - l2 * l2 + len * len) / (2.0 * len);
    let h = (l1 * l1 - a * a).max(0.0).sqrt();
    let p = (pole - dir * pole.dot(dir)).normalize_or_zero();
    let elbow = shoulder + dir * a + p * h;
    limb(b, shoulder, elbow, side, w * 1.12, sleeve);
    limb(b, elbow, hand, side, w, sleeve);
    limb(b, hand - dir * 0.02, hand + dir * 0.07, side, w * 0.95, SKIN);
}

fn muzzle_flash(b: &mut Batch, m: &Mat4, size: f32) {
    b.cube(m, vec3(-0.03, -0.03, 0.0) * size, vec3(0.03, 0.03, 0.14) * size, Color::new(1.0, 0.97, 0.75, 1.0));
    b.cube(m, vec3(-0.1, -0.012, 0.0) * size, vec3(0.1, 0.012, 0.07) * size, Color::new(1.0, 0.75, 0.25, 1.0));
    b.cube(m, vec3(-0.012, -0.1, 0.0) * size, vec3(0.012, 0.1, 0.07) * size, Color::new(1.0, 0.75, 0.25, 1.0));
}

/// Flamme de bouche à la première personne.
pub fn view_flash(b: &mut Batch, m: &Mat4, roll: f32, size: f32) {
    muzzle_flash(b, &(*m * Mat4::from_rotation_z(roll)), size);
}

pub fn draw_player(b: &mut Batch, v: &PlayerView, a: &Puppet) {
    use std::f32::consts::PI;
    let (shirt, pants, dark) = team_colors(v.team);
    let def = weapon(v.weapon);
    let c = a.crouch.max(a.kneel);
    let mv = a.moving * (1.0 - a.air);
    let m = basis(v.pos, v.yaw);
    let side = m.x_axis.truncate();
    let (ds, dc) = a.dir.sin_cos();

    // Bassin : descend en position accroupie, oscille à chaque pas.
    let hip_y = 0.84 - 0.40 * c - 0.03 * mv + (a.phase * 2.0).cos() * 0.03 * mv + 0.05 * a.air;
    let hip_z = -0.12 * c;
    let lean = 0.36 * c + 0.10 * mv * dc - v.pitch * 0.18;
    let tm = m * Mat4::from_translation(vec3(0.0, hip_y, hip_z)) * Mat4::from_rotation_z(-0.07 * mv * ds) * Mat4::from_rotation_x(lean);

    // Jambes : chaque pied suit sa trajectoire, le genou est déduit.
    let back = dc < -0.25;
    let leg_yaw = (if back { angle_diff(PI, a.dir) } else { a.dir }).clamp(-1.1, 1.1) * mv;
    let stride = 0.45 * mv * (1.0 - 0.4 * c) * if back { -1.0 } else { 1.0 };
    for (sx, off) in [(-1.0f32, 0.0f32), (1.0, PI)] {
        let p = a.phase + off;
        let mut fz = -stride * p.cos() - sx * 0.10 * c * (1.0 - mv) + 0.02;
        let mut fy = 0.17 * mv * p.sin().max(0.0);
        fz += (sx * 0.14 - fz) * a.air;
        fy += (0.2 - fy) * a.air;
        let lm = m * Mat4::from_translation(vec3(sx * 0.12, hip_y, hip_z)) * Mat4::from_rotation_y(leg_yaw);
        let (knee, foot) = ik2(0.44, 0.42, fz - hip_z, fy + 0.08 - hip_y);
        let pw = |z: f32, y: f32| lm.transform_point3(vec3(0.0, y, z));
        let lside = lm.x_axis.truncate();
        limb(b, pw(0.0, 0.0), pw(knee.0, knee.1), lside, 0.088, pants);
        limb(b, pw(knee.0, knee.1), pw(foot.0, foot.1), lside, 0.074, pants);
        b.cube(&(lm * Mat4::from_translation(vec3(0.0, knee.1, knee.0))), vec3(-0.08, -0.05, 0.02), vec3(0.08, 0.05, 0.1), dark);
        b.cube(&(lm * Mat4::from_translation(vec3(0.0, foot.1, foot.0))), vec3(-0.08, -0.08, -0.1), vec3(0.08, 0.015, 0.17), SHOE);
    }

    // Buste, gilet et ceinture.
    b.cube(&tm, vec3(-0.23, 0.0, -0.125), vec3(0.23, 0.58, 0.125), shirt);
    b.cube(&tm, vec3(-0.238, -0.06, -0.132), vec3(0.238, 0.07, 0.132), pants);
    b.cube(&tm, vec3(-0.19, 0.2, -0.15), vec3(0.19, 0.52, 0.155), dark);
    b.cube(&tm, vec3(-0.14, 0.24, 0.155), vec3(0.14, 0.34, 0.185), SHOE);

    // Tête : suit le regard, compense l'inclinaison du buste.
    let hm = tm * Mat4::from_translation(vec3(0.0, 0.6, 0.0)) * Mat4::from_rotation_x(-lean * 0.85 - v.pitch * 0.55);
    b.cube(&hm, vec3(-0.14, 0.0, -0.14), vec3(0.14, 0.3, 0.14), SKIN);
    b.cube(&hm, vec3(-0.155, 0.17, -0.155), vec3(0.155, 0.33, 0.155), dark);
    if v.team == Team::T {
        b.cube(&hm, vec3(-0.147, 0.0, -0.147), vec3(0.147, 0.11, 0.147), dark);
    } else {
        b.cube(&hm, vec3(-0.15, 0.13, 0.12), vec3(0.15, 0.2, 0.16), SHOE);
    }

    let (sh_r, sh_l) = (tm.transform_point3(vec3(0.27, 0.5, 0.0)), tm.transform_point3(vec3(-0.27, 0.5, 0.0)));
    let (pole_r, pole_l) = (side * 0.6 - Vec3::Y, -side * 0.6 - Vec3::Y);
    if a.kneel > 0.5 {
        // Mains au sol, sur la bombe.
        arm(b, sh_r, m.transform_point3(vec3(0.12, 0.2, 0.42)), pole_r, side, shirt, 0.052);
        arm(b, sh_l, m.transform_point3(vec3(-0.12, 0.2, 0.42)), pole_l, side, shirt, 0.052);
    } else {
        let rp = reload_pose(a.reload);
        let g = grip(v.weapon);
        let wb = (a.phase * 2.0).sin() * 0.014 * mv;
        let aim = tm * Mat4::from_translation(vec3(0.0, 0.47, 0.05)) * Mat4::from_rotation_x(-lean - v.pitch);
        let wm = match def.class {
            Class::Knife => {
                aim * Mat4::from_translation(vec3(0.26, -0.2 + wb, 0.3 + 0.2 * a.shot)) * Mat4::from_rotation_x(-0.9 + 1.3 * a.shot)
            }
            Class::Pistol => {
                aim * Mat4::from_translation(vec3(0.05, -0.02 + wb, 0.5 - 0.04 * a.shot)) * Mat4::from_rotation_x(-0.25 * a.shot - 0.5 * rp.tilt)
            }
            _ => {
                aim * Mat4::from_translation(vec3(0.14, -0.06 + wb, 0.3 - 0.05 * a.shot))
                    * Mat4::from_rotation_x(-0.07 * a.shot - 0.35 * rp.tilt)
                    * Mat4::from_rotation_z(-0.4 * rp.tilt)
            }
        };
        let slide = if def.class == Class::Pistol { rp.slide.max(a.shot) } else { rp.slide };
        weapon_model(b, &wm, v.weapon, &WeaponPose { mag: rp.mag, mag_on: rp.mag_on, slide });
        let right = wm.transform_point3(if def.class == Class::Knife { vec3(0.0, 0.0, -0.01) } else { g.hand });
        let left = if def.class == Class::Knife {
            m.transform_point3(vec3(-0.3, hip_y - 0.02 + wb, 0.12 + 0.1 * mv * (a.phase).sin()))
        } else {
            let mag = if v.weapon == 6 { vec3(rp.mag.x, -rp.mag.y, rp.mag.z) } else { rp.mag };
            let p = g.fore.lerp(g.mag + mag, rp.follow) + vec3(-0.1, -0.2, -0.1) * rp.away;
            wm.transform_point3(p.lerp(g.rack, rp.rack))
        };
        arm(b, sh_r, right, pole_r, side, shirt, 0.052);
        arm(b, sh_l, left, pole_l, side, shirt, 0.052);
        if a.shot > 0.55 && def.class != Class::Knife {
            muzzle_flash(b, &(wm * Mat4::from_translation(vec3(0.0, 0.03, g.muzzle))), 1.3);
        }
    }
    if v.has_bomb {
        bomb_model(b, &(tm * Mat4::from_translation(vec3(0.0, 0.2, -0.16)) * Mat4::from_rotation_x(-1.57)), false);
    }
}

/// Corps qui s'effondre ; `t` va de 0 (debout) à 1 (au sol), l'arme tombe à côté.
pub fn draw_corpse(b: &mut Batch, pos: Vec3, yaw: f32, team: Team, t: f32) {
    let k = t.clamp(0.0, 1.0);
    let e = 1.0 - (1.0 - k).powi(3);
    let bounce = if k > 0.7 && k < 1.0 { ((k - 0.7) / 0.3 * std::f32::consts::PI).sin() * 0.07 } else { 0.0 };
    let m = basis(pos + Vec3::Y * (0.13 * e), yaw) * Mat4::from_rotation_x(-e * 1.5708 + bounce);
    let (shirt, pants, dark) = team_colors(team);
    // Les genoux fléchissent pendant la chute.
    let bend = (k * std::f32::consts::PI).sin() * 0.5 + 0.12 * e;
    for (x, s) in [(-0.13f32, 1.0f32), (0.13, 0.6)] {
        let thigh = m * Mat4::from_translation(vec3(x, 0.84, 0.0)) * Mat4::from_rotation_x(-bend * s);
        b.cube(&thigh, vec3(-0.09, -0.44, -0.09), vec3(0.09, 0.0, 0.09), pants);
        let shin = thigh * Mat4::from_translation(vec3(0.0, -0.44, 0.0)) * Mat4::from_rotation_x(bend * s * 1.8);
        b.cube(&shin, vec3(-0.075, -0.42, -0.075), vec3(0.075, 0.0, 0.075), pants);
        b.cube(&shin, vec3(-0.08, -0.48, -0.1), vec3(0.08, -0.4, 0.17), SHOE);
    }
    b.cube(&m, vec3(-0.23, 0.84, -0.125), vec3(0.23, 1.42, 0.125), shirt);
    b.cube(&m, vec3(-0.19, 1.04, -0.15), vec3(0.19, 1.36, 0.155), dark);
    b.cube(&m, vec3(-0.14, 1.44, -0.14), vec3(0.14, 1.74, 0.14), SKIN);
    b.cube(&m, vec3(-0.155, 1.61, -0.155), vec3(0.155, 1.77, 0.155), dark);
    // Bras écartés.
    for s in [-1.0f32, 1.0] {
        let am = m * Mat4::from_translation(vec3(s * 0.27, 1.34, 0.0)) * Mat4::from_rotation_z(s * (0.3 + 0.9 * e));
        b.cube(&am, vec3(-0.055, -0.56, -0.055), vec3(0.055, 0.0, 0.055), shirt);
        b.cube(&am, vec3(-0.05, -0.64, -0.05), vec3(0.05, -0.56, 0.05), SKIN);
    }
}

/// Karambit : anneau au pommeau, manche légèrement cambré et lame en griffe
/// faite de segments qui tournent progressivement vers le bas (-Y).
fn karambit(b: &mut Batch, m: &Mat4) {
    let grip = rgb(34, 36, 42);
    let steel = rgb(150, 156, 170);
    let edge = rgb(232, 236, 244);
    let ring = rgb(96, 100, 112);
    // Anneau (quatre côtés autour du trou pour l'index).
    b.cube(m, vec3(-0.006, 0.014, -0.116), vec3(0.006, 0.028, -0.058), ring);
    b.cube(m, vec3(-0.006, -0.028, -0.116), vec3(0.006, -0.014, -0.058), ring);
    b.cube(m, vec3(-0.006, -0.014, -0.116), vec3(0.006, 0.014, -0.102), ring);
    b.cube(m, vec3(-0.006, -0.014, -0.072), vec3(0.006, 0.014, -0.058), ring);
    // Manche en deux parties, avec rivets.
    let back = *m * Mat4::from_translation(vec3(0.0, 0.0, -0.06)) * Mat4::from_rotation_x(-0.12);
    b.cube(&back, vec3(-0.011, -0.017, 0.0), vec3(0.011, 0.017, 0.062), grip);
    b.cube(&back, vec3(-0.0125, -0.005, 0.02), vec3(0.0125, 0.005, 0.03), ring);
    let mut seg = *m * Mat4::from_rotation_x(0.12);
    b.cube(&seg, vec3(-0.011, -0.017, -0.004), vec3(0.011, 0.017, 0.052), grip);
    b.cube(&seg, vec3(-0.0125, -0.005, 0.022), vec3(0.0125, 0.005, 0.032), ring);
    // Garde.
    seg *= Mat4::from_translation(vec3(0.0, 0.0, 0.05));
    b.cube(&seg, vec3(-0.008, -0.024, 0.0), vec3(0.008, 0.022, 0.012), ring);
    seg *= Mat4::from_translation(vec3(0.0, 0.0, 0.01));
    // Lame : chaque segment est plus fin et tourne un peu plus que le précédent.
    for (len, h, turn) in [(0.046, 0.042, 0.26), (0.044, 0.038, 0.32), (0.042, 0.032, 0.36), (0.04, 0.024, 0.4), (0.036, 0.014, 0.0)] {
        b.cube(&seg, vec3(-0.004, -h / 2.0, 0.0), vec3(0.004, h / 2.0, len), steel);
        // Tranchant, sur le bord intérieur de la courbe.
        b.cube(&seg, vec3(-0.0045, -h / 2.0, 0.0), vec3(0.0045, -h / 2.0 + 0.007, len), edge);
        seg = seg * Mat4::from_translation(vec3(0.0, 0.0, len * 0.86)) * Mat4::from_rotation_x(turn);
    }
}

/// Grenade : petit cylindre en boîtes, aux couleurs de son type.
pub fn grenade_model(b: &mut Batch, m: &Mat4, kind: u8) {
    let cap = rgb(96, 100, 112);
    let (body, band) = match kind {
        FRAG => (rgb(74, 92, 58), rgb(52, 64, 42)),
        SMOKE => (rgb(120, 124, 128), rgb(232, 232, 232)),
        FLASH => (rgb(176, 184, 196), rgb(60, 110, 190)),
        _ => (rgb(170, 48, 40), rgb(240, 150, 40)),
    };
    if kind == FRAG {
        b.cube(m, vec3(-0.042, -0.05, -0.042), vec3(0.042, 0.04, 0.042), body);
        b.cube(m, vec3(-0.046, -0.02, -0.046), vec3(0.046, 0.0, 0.046), band);
    } else {
        b.cube(m, vec3(-0.036, -0.07, -0.036), vec3(0.036, 0.05, 0.036), body);
        b.cube(m, vec3(-0.038, -0.03, -0.038), vec3(0.038, 0.0, 0.038), band);
    }
    b.cube(m, vec3(-0.02, 0.04, -0.02), vec3(0.02, 0.075, 0.02), cap);
    b.cube(m, vec3(0.02, -0.03, -0.012), vec3(0.034, 0.08, 0.012), cap);
}

fn hash(a: i32, b: i32, c: i32, seed: u32) -> f32 {
    let mut h = (a as u32).wrapping_mul(374_761_393) ^ (b as u32).wrapping_mul(668_265_263) ^ (c as u32).wrapping_mul(2_147_483_647) ^ seed.wrapping_mul(0x9E37_79B9);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    ((h ^ (h >> 16)) & 0xFFFF) as f32 / 65535.0
}

/// Nuage de fumée : cubes gris qui se chevauchent sur une grille, opaque de l'intérieur
/// comme de l'extérieur. Il gonfle à l'apparition puis s'amenuise en fin de vie.
pub fn draw_smoke(b: &mut Batch, id: u16, pos: Vec3, time_left: f32, t: f32) {
    let grow = ss(0.0, 1.4, SMOKE_TIME - time_left);
    let fade = ss(0.0, 2.5, time_left);
    let r = SMOKE_RADIUS * (0.25 + 0.75 * grow);
    for iy in 0..=3 {
        for iz in -3..=3 {
            for ix in -3..=3 {
                let (h1, h2, h3) = (hash(ix, iy, iz, id as u32), hash(iz, ix, iy, id as u32 + 7), hash(iy, iz, ix, id as u32 + 13));
                let p = vec3(ix as f32 + (h1 - 0.5) * 0.5, iy as f32 * 0.95 + 0.35 + (h2 - 0.5) * 0.4, iz as f32 + (h3 - 0.5) * 0.5);
                if vec3(p.x, p.y * 1.15, p.z).length() > r {
                    continue;
                }
                let drift = vec3((t * 0.45 + h1 * 6.28).sin(), (t * 0.33 + h2 * 6.28).sin() * 0.6, (t * 0.4 + h3 * 6.28).cos()) * 0.12;
                let s = (0.56 + 0.16 * h2) * (0.35 + 0.65 * grow) * (0.25 + 0.75 * fade);
                let g = 0.6 + 0.2 * h3;
                b.cube(&Mat4::from_translation(pos + p + drift), Vec3::splat(-s), Vec3::splat(s), Color::new(g, g, g * 1.02, 1.0));
            }
        }
    }
}

/// Flammes : colonnes vacillantes posées sur le sol dans le rayon de l'incendie.
pub fn draw_fire(b: &mut Batch, world: &World, id: u16, pos: Vec3, time_left: f32, t: f32) {
    let fade = ss(0.0, 1.2, time_left) * ss(0.0, 0.4, FIRE_TIME - time_left);
    for i in 0..46 {
        let (h1, h2, h3) = (hash(i, 1, 0, id as u32), hash(i, 2, 0, id as u32), hash(i, 3, 0, id as u32));
        let (a, rr) = (h1 * std::f32::consts::TAU, FIRE_RADIUS * h2.sqrt());
        let (x, z) = (pos.x + a.cos() * rr, pos.z + a.sin() * rr);
        let Some(hit) = world.raycast(vec3(x, pos.y + 1.2, z), Vec3::NEG_Y, 3.0) else { continue };
        let y = pos.y + 1.2 - hit.dist;
        let flick = (t * (7.0 + h3 * 6.0) + h1 * 20.0).sin() * 0.5 + 0.5;
        let hgt = (0.3 + 0.7 * flick) * (0.6 + 0.9 * h3) * fade * (1.3 - 0.6 * rr / FIRE_RADIUS);
        let w = 0.11 + 0.11 * h2;
        let m = Mat4::from_translation(vec3(x, y, z));
        b.cube(&m, vec3(-w, 0.0, -w), vec3(w, hgt * 0.62, w), Color::new(1.0, 0.42 + 0.2 * h3, 0.08, 1.0));
        b.cube(&m, vec3(-w * 0.55, hgt * 0.5, -w * 0.55), vec3(w * 0.55, hgt, w * 0.55), Color::new(1.0, 0.86, 0.3, 1.0));
        b.cube(&m, vec3(-w * 1.25, 0.0, -w * 1.25), vec3(w * 1.25, 0.01, w * 1.25), Color::new(0.16, 0.12, 0.1, 1.0));
    }
}

/// Arme en boîtes ; origine à la poignée, +Z vers le canon, unités en mètres.
/// `pose` déplace le chargeur et la culasse (rechargement, tir).
pub fn weapon_model(b: &mut Batch, m: &Mat4, id: u8, pose: &WeaponPose) {
    if is_grenade(id) {
        grenade_model(b, &(*m * Mat4::from_translation(vec3(0.0, 0.04, 0.0)) * Mat4::from_scale(Vec3::splat(1.2))), id - GRENADE_BASE);
        return;
    }
    let def = weapon(id);
    if def.class == Class::Knife {
        karambit(b, m);
        return;
    }
    let steel = rgb(48, 48, 54);
    let dark = rgb(30, 30, 34);
    let wood = rgb(128, 84, 46);
    let magc = rgb(40, 40, 46);
    let s = pose.slide;
    // Le chargeur de l'Essaim (id 6) est posé sur le dessus : il sort vers le haut.
    let mm = *m * Mat4::from_translation(if id == 6 { vec3(pose.mag.x, -pose.mag.y, pose.mag.z) } else { pose.mag });
    let mag_on = pose.mag_on;
    let mut c = |mat: &Mat4, x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32, col: Color| {
        b.cube(mat, vec3(x0, y0, z0), vec3(x1, y1, z1), col);
    };
    match def.class {
        Class::Knife => {}
        Class::Pistol => {
            let (body, len) = match id {
                2 => (rgb(84, 92, 66), 0.15),
                3 => (rgb(176, 178, 184), 0.21),
                _ => (steel, 0.14),
            };
            c(m, -0.016, -0.11, -0.05, 0.016, 0.0, -0.005, dark);
            c(m, -0.015, 0.0, -0.055, 0.015, 0.022, len - 0.012, dark);
            c(m, -0.012, -0.03, -0.005, 0.012, 0.0, 0.05, dark);
            c(m, -0.008, 0.022, len - 0.07, 0.008, 0.042, len + 0.004, rgb(70, 70, 76));
            // Culasse mobile : recule au tir et à l'armement.
            let z = -0.05 * s;
            c(m, -0.018, 0.022, -0.06 + z, 0.018, 0.052, len + z, body);
            c(m, -0.005, 0.052, len - 0.025 + z, 0.005, 0.062, len - 0.008 + z, dark);
            c(m, -0.012, 0.052, -0.055 + z, 0.012, 0.062, -0.04 + z, dark);
            if mag_on {
                c(&mm, -0.011, -0.122, -0.044, 0.011, -0.012, -0.012, magc);
            }
        }
        Class::Smg => {
            let body = match id {
                4 => rgb(70, 70, 76),
                5 => rgb(40, 42, 48),
                _ => rgb(74, 80, 70),
            };
            c(m, -0.025, -0.02, -0.12, 0.025, 0.07, 0.24, body);
            c(m, -0.014, -0.12, -0.06, 0.014, -0.02, -0.02, dark);
            c(m, -0.012, 0.01, 0.24, 0.012, 0.035, 0.34, dark);
            c(m, -0.012, 0.0, -0.32, 0.012, 0.03, -0.12, dark);
            c(m, -0.015, -0.03, -0.34, 0.015, 0.07, -0.31, dark);
            c(m, -0.004, 0.07, 0.18, 0.004, 0.09, 0.2, dark);
            c(m, 0.025, 0.035, 0.02 - 0.07 * s, 0.042, 0.055, 0.05 - 0.07 * s, rgb(96, 100, 112));
            if mag_on && id == 6 {
                c(&mm, -0.03, 0.07, -0.06, 0.03, 0.1, 0.2, rgb(60, 110, 120));
            } else if mag_on {
                c(&mm, -0.012, -0.2, 0.05, 0.012, -0.02, 0.09, magc);
            }
        }
        Class::Rifle => {
            let (body, guard) = match id {
                7 => (rgb(56, 52, 50), wood),
                8 => (rgb(44, 46, 52), rgb(44, 46, 52)),
                _ => (rgb(70, 82, 64), rgb(52, 60, 48)),
            };
            c(m, -0.025, -0.02, -0.14, 0.025, 0.07, 0.2, body);
            c(m, -0.028, -0.015, 0.2, 0.028, 0.06, 0.44, guard);
            c(m, -0.01, 0.015, 0.44, 0.01, 0.04, 0.68, dark);
            c(m, -0.014, -0.13, -0.08, 0.014, -0.02, -0.04, dark);
            c(m, -0.018, -0.04, -0.44, 0.018, 0.06, -0.14, if id == 7 { wood } else { body });
            c(m, -0.005, 0.07, 0.36, 0.005, 0.1, 0.38, dark);
            c(m, -0.005, 0.07, -0.08, 0.005, 0.1, -0.06, dark);
            c(m, 0.025, 0.03, -0.02 - 0.08 * s, 0.045, 0.05, 0.02 - 0.08 * s, rgb(96, 100, 112));
            if mag_on {
                c(&mm, -0.013, -0.2, 0.06, 0.013, -0.02, 0.11, magc);
                c(&mm, -0.013, -0.24, 0.085, 0.013, -0.18, 0.14, magc);
            }
        }
        Class::Sniper => {
            let body = if id == 11 { rgb(60, 78, 58) } else { rgb(72, 66, 58) };
            c(m, -0.026, -0.02, -0.18, 0.026, 0.07, 0.34, body);
            c(m, -0.011, 0.02, 0.34, 0.011, 0.045, 0.92, dark);
            c(m, -0.016, 0.01, 0.86, 0.016, 0.055, 0.94, dark);
            c(m, -0.02, -0.06, -0.52, 0.02, 0.06, -0.18, body);
            c(m, -0.014, -0.13, -0.1, 0.014, -0.02, -0.06, dark);
            c(m, -0.022, 0.09, -0.08, 0.022, 0.135, 0.26, dark);
            c(m, -0.028, 0.085, 0.22, 0.028, 0.14, 0.28, dark);
            c(m, -0.008, 0.07, 0.0, 0.008, 0.09, 0.03, dark);
            c(m, -0.008, 0.07, 0.16, 0.008, 0.09, 0.19, dark);
            // Verrou : levier sur le côté droit, tiré en arrière entre deux tirs.
            let z = -0.1 * s;
            c(m, 0.026, 0.04, -0.1 + z, 0.06, 0.058, -0.075 + z, rgb(96, 100, 112));
            c(m, 0.05, 0.03, -0.105 + z, 0.072, 0.068, -0.07 + z, rgb(20, 20, 22));
            if mag_on {
                c(&mm, -0.013, -0.1, 0.06, 0.013, -0.02, 0.14, magc);
            }
        }
        Class::Shotgun => {
            // Sanglier : bois et acier, pompe mobile. Ouragan : noir, semi-automatique.
            let pump = id == 13;
            let (body, furn) = if pump { (rgb(58, 58, 64), wood) } else { (rgb(34, 36, 40), rgb(42, 44, 48)) };
            c(m, -0.026, -0.02, -0.12, 0.026, 0.07, 0.16, body);
            c(m, -0.013, 0.03, 0.16, 0.013, 0.058, 0.72, dark);
            c(m, -0.012, -0.004, 0.16, 0.012, 0.024, 0.62, steel);
            c(m, -0.004, 0.058, 0.69, 0.004, 0.072, 0.71, rgb(200, 200, 190));
            c(m, -0.014, -0.13, -0.08, 0.014, -0.02, -0.04, if pump { wood } else { dark });
            c(m, -0.019, -0.05, -0.44, 0.019, 0.06, -0.12, furn);
            c(m, -0.022, -0.06, -0.47, 0.022, 0.07, -0.44, dark);
            if pump {
                // Pompe : tirée vers l'arrière entre deux tirs.
                let z = -0.09 * s;
                c(m, -0.027, -0.03, 0.24 + z, 0.027, 0.034, 0.42 + z, furn);
                for k in 0..4 {
                    let zz = 0.27 + k as f32 * 0.04 + z;
                    c(m, -0.028, -0.025, zz, 0.028, 0.03, zz + 0.012, rgb(96, 64, 34));
                }
            } else {
                c(m, -0.027, -0.03, 0.2, 0.027, 0.034, 0.44, furn);
                c(m, 0.026, 0.035, 0.0 - 0.08 * s, 0.044, 0.052, 0.035 - 0.08 * s, rgb(96, 100, 112));
                c(m, -0.006, 0.07, -0.06, 0.006, 0.085, 0.1, dark);
            }
            // Cartouche rouge tenue par la main gauche pendant le rechargement.
            if pose.mag.length() > 0.005 {
                c(&mm, -0.009, -0.075, 0.03, 0.009, -0.057, 0.09, rgb(170, 40, 34));
                c(&mm, -0.0095, -0.0755, 0.03, 0.0095, -0.0565, 0.042, rgb(190, 160, 70));
            }
        }
        Class::Lmg => {
            c(m, -0.032, -0.03, -0.16, 0.032, 0.08, 0.3, rgb(52, 54, 58));
            c(m, -0.012, 0.015, 0.3, 0.012, 0.045, 0.7, dark);
            c(m, -0.02, 0.0, 0.3, 0.02, 0.06, 0.46, rgb(40, 40, 44));
            c(m, -0.014, -0.13, -0.1, 0.014, -0.03, -0.06, dark);
            c(m, -0.02, -0.05, -0.46, 0.02, 0.06, -0.16, rgb(40, 40, 44));
            c(m, -0.03, 0.08, -0.04, 0.03, 0.1, 0.2, dark);
            c(m, -0.05, -0.16, 0.58, -0.04, 0.02, 0.6, dark);
            c(m, 0.04, -0.16, 0.58, 0.05, 0.02, 0.6, dark);
            c(m, 0.032, 0.03, -0.02 - 0.08 * s, 0.05, 0.05, 0.02 - 0.08 * s, rgb(96, 100, 112));
            if mag_on {
                c(&mm, -0.06, -0.17, 0.02, 0.06, -0.03, 0.16, rgb(66, 80, 62));
            }
        }
    }
}

/// Bombe : boîtier, pains d'explosif et voyant.
pub fn bomb_model(b: &mut Batch, m: &Mat4, light: bool) {
    b.cube(m, vec3(-0.16, 0.0, -0.11), vec3(0.16, 0.09, 0.11), rgb(44, 46, 48));
    for x in [-0.1, 0.0, 0.1] {
        b.cube(m, vec3(x - 0.04, 0.09, -0.09), vec3(x + 0.04, 0.14, 0.03), rgb(150, 96, 60));
    }
    b.cube(m, vec3(-0.08, 0.09, 0.04), vec3(0.08, 0.12, 0.1), rgb(26, 30, 28));
    let led = if light { rgb(255, 60, 40) } else { rgb(90, 24, 20) };
    b.cube(m, vec3(0.10, 0.09, 0.05), vec3(0.14, 0.13, 0.09), led);
}

/// Boîte fine entre deux points (traçante).
pub fn beam(b: &mut Batch, from: Vec3, to: Vec3, w: f32, color: Color) {
    limb(b, from, to, Vec3::Y, w, color);
}
