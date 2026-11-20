//! Sons synthétisés au lancement (aucun fichier audio à distribuer).

use cesi_common::weapons::{Class, WeaponDef};
use macroquad::audio::{load_sound_from_bytes, play_sound, PlaySoundParams, Sound};
use macroquad::prelude::Vec3;
use std::collections::HashMap;
use std::f32::consts::TAU;

const RATE: f32 = 44100.0;

struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
    }
}

/// Encode des échantillons mono en WAV 16 bits.
fn wav(samples: &[f32]) -> Vec<u8> {
    let n = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + n as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + n).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(RATE as u32).to_le_bytes());
    out.extend_from_slice(&(RATE as u32 * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&n.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32000.0) as i16).to_le_bytes());
    }
    out
}

/// Détonation : bruit filtré + coup grave, avec décroissance exponentielle.
fn shot(dur: f32, decay: f32, lowpass: f32, thump: f32, seed: u32) -> Vec<f32> {
    let mut nz = Noise(seed);
    let mut lp = 0.0;
    (0..(dur * RATE) as usize)
        .map(|i| {
            let t = i as f32 / RATE;
            lp += (nz.next() - lp) * lowpass;
            let body = lp * (-t * decay).exp() * 1.6;
            let low = (t * thump * TAU).sin() * (-t * decay * 1.5).exp() * 0.9;
            let crack = nz.next() * (-t * 400.0).exp() * 0.8;
            (body + low + crack).tanh() * 0.9
        })
        .collect()
}

fn tone(dur: f32, freqs: &[f32], decay: f32) -> Vec<f32> {
    (0..(dur * RATE) as usize)
        .map(|i| {
            let t = i as f32 / RATE;
            let attack = (t * 400.0).min(1.0);
            freqs.iter().map(|f| (t * f * TAU).sin()).sum::<f32>() / freqs.len() as f32 * (-t * decay).exp() * attack * 0.7
        })
        .collect()
}

/// Suite de notes (fréquence, durée).
fn melody(notes: &[(f32, f32)]) -> Vec<f32> {
    notes.iter().flat_map(|(f, d)| tone(*d, &[*f, *f * 2.0], 5.0)).collect()
}

/// Clics secs aux instants donnés (rechargement, percuteur…).
fn clicks(dur: f32, times: &[f32], seed: u32) -> Vec<f32> {
    let mut nz = Noise(seed);
    (0..(dur * RATE) as usize)
        .map(|i| {
            let t = i as f32 / RATE;
            let n = nz.next();
            times.iter().filter(|c| t >= **c).map(|c| n * (-(t - c) * 180.0).exp()).sum::<f32>() * 0.5
        })
        .collect()
}

fn thud(dur: f32, lowpass: f32, decay: f32, gain: f32, seed: u32) -> Vec<f32> {
    let mut nz = Noise(seed);
    let mut lp = 0.0;
    (0..(dur * RATE) as usize)
        .map(|i| {
            let t = i as f32 / RATE;
            lp += (nz.next() - lp) * lowpass;
            (lp * (-t * decay).exp() * gain).tanh()
        })
        .collect()
}

/// Superpose deux sons (le plus long donne la durée).
fn mix(a: Vec<f32>, b: Vec<f32>) -> Vec<f32> {
    let n = a.len().max(b.len());
    (0..n).map(|i| a.get(i).unwrap_or(&0.0) + b.get(i).unwrap_or(&0.0)).collect()
}

/// Touche du clavier de la bombe : petit bip électronique sur un clic de bouton.
fn key(freq: f32, seed: u32) -> Vec<f32> {
    mix(tone(0.075, &[freq, freq * 1.5], 38.0), clicks(0.03, &[0.0], seed))
}

fn explosion() -> Vec<f32> {
    let mut nz = Noise(77);
    let mut lp = 0.0;
    (0..(2.2 * RATE) as usize)
        .map(|i| {
            let t = i as f32 / RATE;
            lp += (nz.next() - lp) * 0.06;
            let rumble = (t * 48.0 * TAU).sin() * (-t * 1.8).exp();
            ((lp * 6.0 * (-t * 2.2).exp()) + rumble).tanh() * 0.95
        })
        .collect()
}

pub struct Audio {
    sounds: HashMap<&'static str, Sound>,
    pub volume: f32,
}

impl Audio {
    pub async fn new() -> Audio {
        let defs: Vec<(&'static str, Vec<f32>)> = vec![
            ("pistol", shot(0.28, 20.0, 0.55, 170.0, 1)),
            ("smg", shot(0.20, 28.0, 0.65, 210.0, 2)),
            ("rifle", shot(0.38, 15.0, 0.42, 130.0, 3)),
            ("sniper", shot(1.0, 5.5, 0.26, 75.0, 4)),
            ("lmg", shot(0.32, 17.0, 0.36, 105.0, 5)),
            ("shotgun", shot(0.5, 10.0, 0.3, 80.0, 6)),
            // Fusil à pompe : la détonation, puis la pompe tirée et repoussée.
            ("pump", mix(shot(0.62, 8.5, 0.28, 70.0, 7), clicks(0.62, &[0.3, 0.33, 0.5, 0.52], 8))),
            ("knife", thud(0.16, 0.5, 26.0, 1.2, 6)),
            ("mag_out", clicks(0.16, &[0.0, 0.05], 7)),
            ("mag_in", clicks(0.14, &[0.0, 0.015, 0.03], 12)),
            ("rack", clicks(0.26, &[0.0, 0.11, 0.125], 13)),
            ("click", clicks(0.08, &[0.0], 8)),
            ("step", thud(0.09, 0.12, 45.0, 2.4, 9)),
            ("hit", tone(0.06, &[1500.0, 2250.0], 50.0)),
            ("headshot", tone(0.22, &[2100.0, 3150.0, 4200.0], 16.0)),
            ("hurt", thud(0.2, 0.08, 18.0, 5.0, 10)),
            ("beep", tone(0.09, &[1050.0], 12.0)),
            ("planted", melody(&[(880.0, 0.1), (660.0, 0.1), (880.0, 0.2)])),
            ("defused", melody(&[(660.0, 0.1), (880.0, 0.1), (1320.0, 0.3)])),
            ("win", melody(&[(523.0, 0.12), (659.0, 0.12), (784.0, 0.12), (1046.0, 0.35)])),
            ("lose", melody(&[(440.0, 0.16), (370.0, 0.16), (294.0, 0.4)])),
            ("buy", clicks(0.2, &[0.0, 0.07], 11)),
            ("explosion", explosion()),
            ("boom", shot(1.1, 5.0, 0.18, 55.0, 21)),
            ("bang", shot(0.5, 11.0, 0.75, 320.0, 22)),
            ("ring", tone(2.6, &[3150.0, 3170.0], 1.5)),
            ("smoke", thud(1.3, 0.55, 2.6, 0.8, 23)),
            ("ignite", thud(0.9, 0.16, 4.0, 3.2, 24)),
            ("fire", thud(0.55, 0.3, 4.5, 1.1, 25)),
            ("throw", thud(0.2, 0.45, 18.0, 1.0, 26)),
            ("ping", melody(&[(1320.0, 0.07), (1760.0, 0.16)])),
            // Pose de la bombe : on l'ouvre, puis on tape le code.
            ("bomb_open", mix(clicks(0.3, &[0.0, 0.09, 0.17], 31), thud(0.25, 0.2, 14.0, 1.4, 32))),
            ("bomb_key0", key(1180.0, 33)),
            ("bomb_key1", key(1320.0, 34)),
            ("bomb_key2", key(1480.0, 35)),
            ("bomb_key3", key(1570.0, 36)),
            // Désamorçage : on ouvre le boîtier, puis cliquetis d'outil et fils coupés.
            ("defuse_open", clicks(0.34, &[0.0, 0.11, 0.13, 0.25], 37)),
            ("defuse_tick", mix(clicks(0.16, &[0.0, 0.035, 0.07, 0.105], 38), tone(0.05, &[2600.0, 3900.0], 70.0))),
            ("defuse_snip", mix(clicks(0.06, &[0.0, 0.006], 39), tone(0.12, &[5200.0, 5240.0], 40.0))),
        ];
        let mut sounds = HashMap::new();
        for (name, samples) in defs {
            if let Ok(s) = load_sound_from_bytes(&wav(&samples)).await {
                sounds.insert(name, s);
            }
        }
        Audio { sounds, volume: 0.7 }
    }

    pub fn play(&self, name: &str, vol: f32) {
        let v = (vol * self.volume).clamp(0.0, 1.0);
        if v < 0.01 {
            return;
        }
        if let Some(s) = self.sounds.get(name) {
            play_sound(s, PlaySoundParams { looped: false, volume: v });
        }
    }

    /// Son localisé : le volume baisse avec la distance jusqu'à `range`.
    pub fn play_at(&self, name: &str, listener: Vec3, pos: Vec3, range: f32, vol: f32) {
        let k = (1.0 - listener.distance(pos) / range).clamp(0.0, 1.0);
        self.play(name, vol * k * k);
    }

    pub fn shot_name(def: &WeaponDef) -> &'static str {
        match def.class {
            Class::Knife => "knife",
            Class::Pistol => "pistol",
            Class::Smg => "smg",
            Class::Rifle => "rifle",
            Class::Sniper => "sniper",
            Class::Lmg => "lmg",
            Class::Shotgun if def.auto => "shotgun",
            Class::Shotgun => "pump",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les sons de la bombe sont audibles sans saturer. Avec `CESI_WAV_DIR`, ils sont
    /// aussi enregistrés en WAV (pose puis désamorçage enchaînés) pour les écouter.
    #[test]
    fn bomb_sounds_are_audible_and_not_clipped() {
        let open = mix(clicks(0.3, &[0.0, 0.09, 0.17], 31), thud(0.25, 0.2, 14.0, 1.4, 32));
        let keys: Vec<Vec<f32>> = [1180.0, 1320.0, 1480.0, 1570.0].iter().enumerate().map(|(i, f)| key(*f, 33 + i as u32)).collect();
        let dopen = clicks(0.34, &[0.0, 0.11, 0.13, 0.25], 37);
        let tick = mix(clicks(0.16, &[0.0, 0.035, 0.07, 0.105], 38), tone(0.05, &[2600.0, 3900.0], 70.0));
        let snip = mix(clicks(0.06, &[0.0, 0.006], 39), tone(0.12, &[5200.0, 5240.0], 40.0));
        for s in keys.iter().chain([&open, &dopen, &tick, &snip]) {
            let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.15 && peak <= 1.0, "crête {}", peak);
        }
        if let Ok(dir) = std::env::var("CESI_WAV_DIR") {
            // Enchaînement : ouverture, 12 touches, pause, puis désamorçage.
            let at = |out: &mut Vec<f32>, t: f32, s: &[f32]| {
                let o = (t * RATE) as usize;
                out.resize(out.len().max(o + s.len()), 0.0);
                for (i, v) in s.iter().enumerate() {
                    out[o + i] += v;
                }
            };
            let mut plant = Vec::new();
            at(&mut plant, 0.0, &open);
            for i in 0..12 {
                at(&mut plant, 0.4 + i as f32 * 0.23, &keys[(i * 7 + i / 3) % 4]);
            }
            at(&mut plant, 3.5, &melody(&[(880.0, 0.1), (660.0, 0.1), (880.0, 0.2)]));
            let mut defuse = Vec::new();
            at(&mut defuse, 0.0, &dopen);
            for i in 0..12 {
                at(&mut defuse, 0.4 + i as f32 * 0.4, if i % 4 == 3 { &snip } else { &tick });
            }
            at(&mut defuse, 5.3, &melody(&[(660.0, 0.1), (880.0, 0.1), (1320.0, 0.3)]));
            std::fs::write(format!("{}/pose_bombe.wav", dir), wav(&plant)).unwrap();
            std::fs::write(format!("{}/desamorcage.wav", dir), wav(&defuse)).unwrap();
        }
    }
}
