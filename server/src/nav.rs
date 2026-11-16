//! Graphe de navigation des bots, calculé automatiquement à partir des voxels.

use cesi_common::glam::{vec3, IVec3, Vec3};
use cesi_common::voxel::{World, VS};
use cesi_common::Rng;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const NONE: u32 = u32::MAX;
const DIRS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];

pub struct Nav {
    sx: i32,
    sy: i32,
    sz: i32,
    index: Vec<u32>,
    nodes: Vec<IVec3>,
    edges: Vec<Vec<(u32, u32)>>,
    reachable: Vec<bool>,
}

/// Vrai si un corps fin tient debout dans la cellule (sans regarder le sol).
fn free(w: &World, x: i32, y: i32, z: i32) -> bool {
    let (cx, cz) = ((x as f32 + 0.5) * VS, (z as f32 + 0.5) * VS);
    let y0 = y as f32 * VS + 0.01;
    // Bas du corps étroit (pour tenir sur une marche), haut du corps à la
    // largeur réelle du joueur (pour ne pas frôler murs et montants de porte).
    !w.aabb_hits(vec3(cx - 0.2, y0, cz - 0.2), vec3(cx + 0.2, y0 + 0.6, cz + 0.2))
        && !w.aabb_hits(vec3(cx - 0.3, y0 + 0.55, cz - 0.3), vec3(cx + 0.3, y0 + 1.75, cz + 0.3))
}

impl Nav {
    pub fn build(w: &World, start: Vec3) -> Nav {
        let mut nav = Nav {
            sx: w.sx,
            sy: w.sy,
            sz: w.sz,
            index: vec![NONE; (w.sx * w.sy * w.sz) as usize],
            nodes: Vec::new(),
            edges: Vec::new(),
            reachable: Vec::new(),
        };
        for y in 1..w.sy - 4 {
            for z in 0..w.sz {
                for x in 0..w.sx {
                    if !w.solid(x, y, z) && w.solid(x, y - 1, z) && free(w, x, y, z) {
                        let i = nav.idx(x, y, z);
                        nav.index[i] = nav.nodes.len() as u32;
                        nav.nodes.push(IVec3::new(x, y, z));
                    }
                }
            }
        }
        nav.edges = vec![Vec::new(); nav.nodes.len()];
        for i in 0..nav.nodes.len() {
            let c = nav.nodes[i];
            let mut list = Vec::new();
            for (dx, dz) in DIRS {
                let (nx, nz) = (c.x + dx, c.z + dz);
                if dx != 0 && dz != 0 {
                    // Diagonale : seulement à plat, sans couper un coin.
                    if let Some(t) = nav.node(nx, c.y, nz) {
                        if nav.node(nx, c.y, c.z).is_some() && nav.node(c.x, c.y, nz).is_some() {
                            list.push((t, 1414));
                        }
                    }
                    continue;
                }
                if let Some(t) = nav.node(nx, c.y, nz) {
                    list.push((t, 1000));
                } else if let Some(t) = nav.node(nx, c.y + 1, nz) {
                    if free(w, c.x, c.y + 1, c.z) {
                        list.push((t, 1300)); // marche à monter
                    }
                } else if free(w, nx, c.y, nz) {
                    for d in 1..=8 {
                        if let Some(t) = nav.node(nx, c.y - d, nz) {
                            list.push((t, 1000 + 250 * d as u32)); // chute
                            break;
                        }
                        if w.solid(nx, c.y - d, nz) {
                            break;
                        }
                    }
                }
            }
            nav.edges[i] = list;
        }
        // Accessibilité depuis le point de départ (parcours en largeur).
        nav.reachable = vec![false; nav.nodes.len()];
        if let Some(s) = nav.nearest_any(start) {
            let mut stack = vec![s];
            nav.reachable[s as usize] = true;
            while let Some(n) = stack.pop() {
                for &(t, _) in &nav.edges[n as usize] {
                    if !nav.reachable[t as usize] {
                        nav.reachable[t as usize] = true;
                        stack.push(t);
                    }
                }
            }
        }
        nav
    }

    fn idx(&self, x: i32, y: i32, z: i32) -> usize {
        ((y * self.sz + z) * self.sx + x) as usize
    }

    fn node(&self, x: i32, y: i32, z: i32) -> Option<u32> {
        if x < 0 || y < 0 || z < 0 || x >= self.sx || y >= self.sy || z >= self.sz {
            return None;
        }
        let n = self.index[self.idx(x, y, z)];
        (n != NONE).then_some(n)
    }

    pub fn pos(&self, n: u32) -> Vec3 {
        let c = self.nodes[n as usize];
        vec3((c.x as f32 + 0.5) * VS, c.y as f32 * VS, (c.z as f32 + 0.5) * VS)
    }

    fn nearest_filtered(&self, p: Vec3, need_reachable: bool) -> Option<u32> {
        let cx = (p.x / VS).floor() as i32;
        let cy = ((p.y + 0.1) / VS).floor() as i32;
        let cz = (p.z / VS).floor() as i32;
        for r in 0..=3i32 {
            for dy in [0, -1, 1, -2, 2, -3, 3] {
                for dz in -r..=r {
                    for dx in -r..=r {
                        if dx.abs().max(dz.abs()) != r {
                            continue;
                        }
                        if let Some(n) = self.node(cx + dx, cy + dy, cz + dz) {
                            if !need_reachable || self.reachable[n as usize] {
                                return Some(n);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn nearest_any(&self, p: Vec3) -> Option<u32> {
        self.nearest_filtered(p, false)
    }

    pub fn nearest(&self, p: Vec3) -> Option<u32> {
        self.nearest_filtered(p, true).or_else(|| self.nearest_any(p))
    }

    /// Point accessible au hasard dans une boîte (en mètres).
    pub fn random_in(&self, min: Vec3, max: Vec3, rng: &mut Rng) -> Option<Vec3> {
        for _ in 0..80 {
            let x = (rng.range(min.x, max.x) / VS).floor() as i32;
            let z = (rng.range(min.z, max.z) / VS).floor() as i32;
            let y0 = (min.y / VS).floor() as i32;
            let y1 = (max.y / VS).ceil() as i32;
            for y in y0..=y1 {
                if let Some(n) = self.node(x, y, z) {
                    let p = self.pos(n);
                    if self.reachable[n as usize] && p.y >= min.y && p.y <= max.y {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    /// Plus court chemin (A*) ; renvoie les positions des pieds à suivre.
    pub fn path(&self, from: Vec3, to: Vec3) -> Option<Vec<Vec3>> {
        let s = self.nearest(from)?;
        let g = self.nearest(to)?;
        let gp = self.nodes[g as usize];
        let h = |n: u32| -> u32 {
            let c = self.nodes[n as usize];
            let (dx, dz) = ((c.x - gp.x) as f32, (c.z - gp.z) as f32);
            ((dx * dx + dz * dz).sqrt() * 1000.0) as u32
        };
        let mut cost = vec![u32::MAX; self.nodes.len()];
        let mut from_node = vec![NONE; self.nodes.len()];
        let mut heap = BinaryHeap::new();
        cost[s as usize] = 0;
        heap.push(Reverse((h(s), s)));
        while let Some(Reverse((f, n))) = heap.pop() {
            if n == g {
                break;
            }
            let c = cost[n as usize];
            if f > c.saturating_add(h(n)) {
                continue;
            }
            for &(t, w) in &self.edges[n as usize] {
                let nc = c + w;
                if nc < cost[t as usize] {
                    cost[t as usize] = nc;
                    from_node[t as usize] = n;
                    heap.push(Reverse((nc + h(t), t)));
                }
            }
        }
        if cost[g as usize] == u32::MAX {
            return None;
        }
        let mut out = vec![self.pos(g)];
        let mut n = g;
        while n != s {
            n = from_node[n as usize];
            out.push(self.pos(n));
        }
        out.reverse();
        Some(out)
    }
}
