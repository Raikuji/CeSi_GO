//! Rendu 3D : maillage du monde voxel (fusion des faces + occlusion ambiante),
//! shader, et modèles en boîtes des joueurs, des armes et de la bombe.

use cesi_common::dir_from_angles;
use cesi_common::protocol::Team;
use cesi_common::voxel::{block_color, World, VOID, VS};
use cesi_common::weapons::{weapon, Class};
use macroquad::prelude::*;

pub const SKY: Color = Color::new(0.56, 0.74, 0.92, 1.0);

const VERT: &str = r#"#version 100
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
const FRAG: &str = r#"#version 100
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
            ShaderSource::Glsl { vertex: VERT, fragment: FRAG },
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
        WorldRender { regions, material }
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
    pub yaw: f32,
    pub pitch: f32,
    pub crouch: bool,
    pub weapon: u8,
    pub has_bomb: bool,
    pub speed: f32,
}

fn team_colors(team: Team) -> (Color, Color, Color) {
    match team {
        Team::T => (rgb(196, 150, 78), rgb(92, 70, 48), rgb(58, 52, 48)),
        Team::CT => (rgb(58, 88, 140), rgb(40, 46, 62), rgb(46, 60, 84)),
    }
}

const SKIN: Color = Color::new(0.87, 0.68, 0.53, 1.0);

pub fn draw_player(b: &mut Batch, v: &PlayerView, walk_phase: f32) {
    let m = basis(v.pos, v.yaw);
    let (shirt, pants, head) = team_colors(v.team);
    let (hip, top) = if v.crouch { (0.45, 0.95) } else { (0.8, 1.4) };
    let swing = walk_phase.sin() * (v.speed / 4.5).min(1.0) * 0.75;
    for (x, a) in [(-0.13, swing), (0.13, -swing)] {
        let lm = m * Mat4::from_translation(vec3(x, hip, 0.0)) * Mat4::from_rotation_x(a);
        b.cube(&lm, vec3(-0.1, -hip, -0.11), vec3(0.1, 0.0, 0.11), pants);
        b.cube(&lm, vec3(-0.105, -hip, -0.115), vec3(0.105, -hip + 0.12, 0.16), rgb(34, 32, 32));
    }
    b.cube(&m, vec3(-0.25, hip, -0.14), vec3(0.25, top, 0.14), shirt);
    b.cube(&m, vec3(-0.2, hip + 0.18, -0.16), vec3(0.2, top - 0.06, 0.16), head);
    let hm = m * Mat4::from_translation(vec3(0.0, top + 0.02, 0.0)) * Mat4::from_rotation_x(-v.pitch * 0.6);
    b.cube(&hm, vec3(-0.15, 0.0, -0.15), vec3(0.15, 0.32, 0.15), head);
    b.cube(&hm, vec3(-0.12, 0.10, 0.15), vec3(0.12, 0.22, 0.17), SKIN);
    // Bras tendus vers l'avant et arme, inclinés selon la visée.
    let am = m * Mat4::from_translation(vec3(0.0, top - 0.12, 0.06)) * Mat4::from_rotation_x(-v.pitch);
    b.cube(&am, vec3(0.19, -0.07, -0.02), vec3(0.31, 0.07, 0.40), shirt);
    b.cube(&am, vec3(-0.31, -0.07, -0.02), vec3(-0.19, 0.07, 0.30), shirt);
    b.cube(&am, vec3(-0.28, -0.06, 0.28), vec3(0.10, 0.05, 0.40), shirt);
    b.cube(&am, vec3(0.10, -0.05, 0.36), vec3(0.22, 0.05, 0.46), SKIN);
    weapon_model(b, &(am * Mat4::from_translation(vec3(0.15, 0.0, 0.42))), v.weapon);
    if v.has_bomb {
        bomb_model(b, &(m * Mat4::from_translation(vec3(0.0, hip + 0.28, -0.2)) * Mat4::from_rotation_x(1.57)), false);
    }
}

/// Corps au sol ; `t` va de 0 (debout) à 1 (allongé).
pub fn draw_corpse(b: &mut Batch, pos: Vec3, yaw: f32, team: Team, t: f32) {
    let k = t.clamp(0.0, 1.0);
    let m = basis(pos + Vec3::Y * (0.14 * k), yaw) * Mat4::from_rotation_x(-k * k * 1.5708);
    let (shirt, pants, head) = team_colors(team);
    for x in [-0.13, 0.13] {
        b.cube(&m, vec3(x - 0.1, 0.0, -0.11), vec3(x + 0.1, 0.8, 0.11), pants);
    }
    b.cube(&m, vec3(-0.25, 0.8, -0.14), vec3(0.25, 1.4, 0.14), shirt);
    b.cube(&m, vec3(-0.15, 1.42, -0.15), vec3(0.15, 1.74, 0.15), head);
    b.cube(&m, vec3(-0.42, 1.0, -0.08), vec3(-0.25, 1.38, 0.08), shirt);
    b.cube(&m, vec3(0.25, 1.0, -0.08), vec3(0.42, 1.38, 0.08), shirt);
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

/// Arme en boîtes ; origine à la poignée, +Z vers le canon, unités en mètres.
pub fn weapon_model(b: &mut Batch, m: &Mat4, id: u8) {
    if weapon(id).class == Class::Knife {
        karambit(b, m);
        return;
    }
    let steel = rgb(48, 48, 54);
    let dark = rgb(30, 30, 34);
    let wood = rgb(128, 84, 46);
    let mut c = |x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32, col: Color| {
        b.cube(m, vec3(x0, y0, z0), vec3(x1, y1, z1), col);
    };
    match weapon(id).class {
        Class::Knife => {}
        Class::Pistol => {
            let (body, len) = match id {
                2 => (rgb(84, 92, 66), 0.15),
                3 => (rgb(176, 178, 184), 0.21),
                _ => (steel, 0.14),
            };
            c(-0.016, -0.11, -0.05, 0.016, 0.0, -0.005, dark);
            c(-0.018, 0.0, -0.06, 0.018, 0.05, len, body);
            c(-0.006, 0.05, len - 0.02, 0.006, 0.062, len - 0.005, dark);
            c(-0.012, -0.03, -0.005, 0.012, 0.0, 0.05, dark);
        }
        Class::Smg => {
            let body = match id {
                4 => rgb(70, 70, 76),
                5 => rgb(40, 42, 48),
                _ => rgb(74, 80, 70),
            };
            c(-0.025, -0.02, -0.12, 0.025, 0.07, 0.24, body);
            c(-0.014, -0.12, -0.06, 0.014, -0.02, -0.02, dark);
            c(-0.012, 0.01, 0.24, 0.012, 0.035, 0.34, dark);
            c(-0.012, 0.0, -0.32, 0.012, 0.03, -0.12, dark);
            c(-0.015, -0.03, -0.34, 0.015, 0.07, -0.31, dark);
            if id == 6 {
                c(-0.03, 0.07, -0.06, 0.03, 0.10, 0.2, rgb(60, 110, 120)); // chargeur horizontal
            } else {
                c(-0.012, -0.2, 0.05, 0.012, -0.02, 0.09, dark);
            }
        }
        Class::Rifle => {
            let (body, guard) = match id {
                7 => (rgb(56, 52, 50), wood),
                8 => (rgb(44, 46, 52), rgb(44, 46, 52)),
                _ => (rgb(70, 82, 64), rgb(52, 60, 48)),
            };
            c(-0.025, -0.02, -0.14, 0.025, 0.07, 0.20, body);
            c(-0.028, -0.015, 0.20, 0.028, 0.06, 0.44, guard);
            c(-0.01, 0.015, 0.44, 0.01, 0.04, 0.68, dark);
            c(-0.014, -0.13, -0.08, 0.014, -0.02, -0.04, dark);
            c(-0.013, -0.2, 0.06, 0.013, -0.02, 0.11, dark);
            c(-0.013, -0.24, 0.085, 0.013, -0.18, 0.14, dark);
            c(-0.018, -0.04, -0.44, 0.018, 0.06, -0.14, if id == 7 { wood } else { body });
            c(-0.005, 0.07, 0.36, 0.005, 0.10, 0.38, dark);
            c(-0.005, 0.07, -0.08, 0.005, 0.10, -0.06, dark);
        }
        Class::Sniper => {
            let body = if id == 11 { rgb(60, 78, 58) } else { rgb(72, 66, 58) };
            c(-0.026, -0.02, -0.18, 0.026, 0.07, 0.34, body);
            c(-0.011, 0.02, 0.34, 0.011, 0.045, 0.92, dark);
            c(-0.016, 0.01, 0.86, 0.016, 0.055, 0.94, dark);
            c(-0.02, -0.06, -0.52, 0.02, 0.06, -0.18, body);
            c(-0.014, -0.13, -0.1, 0.014, -0.02, -0.06, dark);
            c(-0.013, -0.1, 0.06, 0.013, -0.02, 0.14, dark);
            c(-0.022, 0.09, -0.08, 0.022, 0.135, 0.26, dark);
            c(-0.028, 0.085, 0.22, 0.028, 0.14, 0.28, dark);
            c(-0.008, 0.07, 0.0, 0.008, 0.09, 0.03, dark);
            c(-0.008, 0.07, 0.16, 0.008, 0.09, 0.19, dark);
        }
        Class::Lmg => {
            c(-0.032, -0.03, -0.16, 0.032, 0.08, 0.30, rgb(52, 54, 58));
            c(-0.012, 0.015, 0.30, 0.012, 0.045, 0.70, dark);
            c(-0.02, 0.0, 0.30, 0.02, 0.06, 0.46, rgb(40, 40, 44));
            c(-0.06, -0.17, 0.02, 0.06, -0.03, 0.16, rgb(66, 80, 62));
            c(-0.014, -0.13, -0.1, 0.014, -0.03, -0.06, dark);
            c(-0.02, -0.05, -0.46, 0.02, 0.06, -0.16, rgb(40, 40, 44));
            c(-0.03, 0.08, -0.04, 0.03, 0.10, 0.2, dark);
            c(-0.05, -0.16, 0.58, -0.04, 0.02, 0.6, dark);
            c(0.04, -0.16, 0.58, 0.05, 0.02, 0.6, dark);
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
    let d = to - from;
    let len = d.length();
    if len < 0.01 {
        return;
    }
    let f = d / len;
    let r = if f.y.abs() > 0.99 { Vec3::X } else { f.cross(Vec3::Y).normalize() };
    let u = r.cross(f);
    let m = Mat4::from_cols(r.extend(0.0), u.extend(0.0), f.extend(0.0), from.extend(1.0));
    b.cube(&m, vec3(-w, -w, 0.0), vec3(w, w, len), color);
}
