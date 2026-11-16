//! Canal UDP avec une couche fiable minimale : chaque paquet transporte
//! l'accusé de réception, les messages fiables non encore acquittés (renvoyés
//! jusqu'à confirmation) et une charge non fiable facultative.

use bincode::Options;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const MAGIC: u16 = 0xC5E1;
const MAX_RELIABLE_PER_PACKET: usize = 32;

fn opts() -> impl Options {
    bincode::DefaultOptions::new().with_limit(1 << 16)
}

#[derive(Serialize, Deserialize)]
struct Packet<R, U> {
    magic: u16,
    ack: u32,
    rel: Vec<(u32, R)>,
    unrel: Option<U>,
}

pub struct Channel<S> {
    next_seq: u32,
    pending: VecDeque<(u32, S)>,
    recv_seq: u32,
}

impl<S: Serialize + Clone> Default for Channel<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Serialize + Clone> Channel<S> {
    pub fn new() -> Self {
        Channel { next_seq: 0, pending: VecDeque::new(), recv_seq: 0 }
    }

    pub fn send_reliable(&mut self, m: S) {
        self.next_seq += 1;
        self.pending.push_back((self.next_seq, m));
    }

    /// Nombre de messages fiables en attente d'accusé de réception.
    pub fn backlog(&self) -> usize {
        self.pending.len()
    }

    /// Construit le prochain paquet à envoyer.
    pub fn build<U: Serialize>(&self, unrel: Option<U>) -> Vec<u8> {
        let rel: Vec<(u32, S)> = self.pending.iter().take(MAX_RELIABLE_PER_PACKET).cloned().collect();
        opts()
            .serialize(&Packet { magic: MAGIC, ack: self.recv_seq, rel, unrel })
            .unwrap_or_default()
    }

    /// Traite un paquet reçu : renvoie les nouveaux messages fiables (dans
    /// l'ordre) et la charge non fiable. `None` si le paquet est invalide.
    pub fn receive<R: DeserializeOwned, U: DeserializeOwned>(&mut self, data: &[u8]) -> Option<(Vec<R>, Option<U>)> {
        let p: Packet<R, U> = opts().deserialize(data).ok()?;
        if p.magic != MAGIC {
            return None;
        }
        while self.pending.front().is_some_and(|(s, _)| *s <= p.ack) {
            self.pending.pop_front();
        }
        let mut out = Vec::new();
        for (seq, m) in p.rel {
            if seq == self.recv_seq + 1 {
                self.recv_seq = seq;
                out.push(m);
            }
        }
        Some((out, p.unrel))
    }
}
