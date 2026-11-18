//! Serveur de jeu : connexions, rounds, économie, bombe, tirs et bots.

use crate::bot::{Bot, BotCtx};
use crate::drops::Drop;
use crate::nades::{Area, Nade};
use crate::nav::Nav;
use cesi_common::glam::{vec3, Vec3};
use cesi_common::grenades::*;
use cesi_common::map::{self, MapInfo};
use cesi_common::net::Channel;
use cesi_common::physics::eye_height;
use cesi_common::protocol::*;
use cesi_common::voxel::World;
use cesi_common::weapons::*;
use cesi_common::{hdist, Rng};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PLANT_TIME: f32 = 3.2;
const ROUND_END_TIME: f32 = 5.0;
const MATCH_END_TIME: f32 = 12.0;
const MONEY_MAX: u32 = 16000;
const TIMEOUT: f64 = 8.0;
const BOT_NAMES: [&str; 16] = [
    "Gaston", "Lucien", "Marcel", "Odile", "Fernand", "Yvette", "Raymond", "Ginette", "Albert", "Suzanne", "Léon",
    "Paulette", "Honoré", "Colette", "Firmin", "Josette",
];

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct ServerConfig {
    /// Nom affiché aux joueurs.
    pub name: String,
    pub port: u16,
    /// Carte jouée : nom ou numéro (voir la commande `map`).
    pub map: String,
    /// Nombre maximal de joueurs humains.
    pub max_players: u8,
    /// Joueurs par équipe (les bots complètent).
    pub team_size: u8,
    pub fill_bots: bool,
    /// Niveau des bots, de 0 (facile) à 1 (expert).
    pub bot_skill: f32,
    /// Nombre de rounds du match (victoire à la moitié + 1).
    pub max_rounds: u16,
    pub freeze_time: f32,
    pub buy_time: f32,
    pub round_time: f32,
    pub bomb_time: f32,
    pub start_money: u32,
    /// Fait tourner les bots même sans joueur connecté.
    pub run_empty: bool,
    /// Écoute uniquement sur 127.0.0.1 (partie locale).
    #[serde(skip)]
    pub local_only: bool,
    /// Simulation sans attente (tests d'équilibrage bots contre bots).
    #[serde(skip)]
    pub fast: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            name: "Serveur CeSi GO".into(),
            port: DEFAULT_PORT,
            map: map::MAPS[0].name.into(),
            max_players: 10,
            team_size: 5,
            fill_bots: true,
            bot_skill: 0.5,
            max_rounds: 24,
            freeze_time: 6.0,
            buy_time: 20.0,
            round_time: 115.0,
            bomb_time: 40.0,
            start_money: 800,
            run_empty: false,
            local_only: false,
            fast: false,
        }
    }
}

pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub team: Team,
    pub is_bot: bool,
    pub bot: Option<Box<Bot>>,
    pub alive: bool,
    /// Équipement à réinitialiser au prochain round.
    fresh: bool,
    pub health: i32,
    pub armor: i32,
    pub helmet: bool,
    pub kit: bool,
    pub money: u32,
    pub kills: u16,
    pub assists: u16,
    pub deaths: u16,
    /// Dégâts reçus des adversaires depuis l'apparition, par joueur (pour les assistances).
    pub(crate) damage_from: Vec<(PlayerId, i32)>,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub crouch: bool,
    pub weapon: u8,
    pub primary: u8,
    pub secondary: u8,
    pub using: bool,
    pub reloading: bool,
    pub air: bool,
    /// Grenades possédées, par type.
    pub nades: [u8; 4],
    pub action: u8,
    pub progress: f32,
    action_pos: Vec3,
    history: VecDeque<(f64, Vec3, bool)>,
    fire_budget: f32,
    switch_team: bool,
    last_state: u32,
    /// Achats du round, revendables tant que les achats sont ouverts.
    pub(crate) bought: Vec<crate::shop::Purchase>,
    /// Dernière liste de reventes envoyée au joueur.
    pub(crate) refunds_sent: Vec<(BuyItem, u32)>,
}

impl Player {
    pub(crate) fn new(id: PlayerId, name: String, team: Team, money: u32, bot: Option<Box<Bot>>) -> Player {
        Player {
            id,
            name,
            team,
            is_bot: bot.is_some(),
            bot,
            alive: false,
            fresh: true,
            health: 0,
            armor: 0,
            helmet: false,
            kit: false,
            money,
            kills: 0,
            assists: 0,
            deaths: 0,
            damage_from: Vec::new(),
            pos: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            crouch: false,
            weapon: DEFAULT_PISTOL,
            primary: 0,
            secondary: DEFAULT_PISTOL,
            using: false,
            reloading: false,
            air: false,
            nades: [0; 4],
            action: 0,
            progress: 0.0,
            action_pos: Vec3::ZERO,
            history: VecDeque::new(),
            fire_budget: 2.0,
            switch_team: false,
            bought: Vec::new(),
            refunds_sent: Vec::new(),
            last_state: 0,
        }
    }

    /// Position (et posture) du joueur à un instant passé, pour la compensation de latence.
    fn pos_at(&self, t: f64) -> (Vec3, bool) {
        let mut prev: Option<&(f64, Vec3, bool)> = None;
        for h in &self.history {
            if h.0 >= t {
                return match prev {
                    Some(p) if h.0 > p.0 => {
                        let k = ((t - p.0) / (h.0 - p.0)) as f32;
                        (p.1.lerp(h.1, k.clamp(0.0, 1.0)), h.2)
                    }
                    _ => (h.1, h.2),
                };
            }
            prev = Some(h);
        }
        (self.pos, self.crouch)
    }
}

pub(crate) struct Conn {
    pid: PlayerId,
    chan: Channel<ServerMsg>,
    last_recv: f64,
}

pub struct Server {
    pub(crate) cfg: ServerConfig,
    pub(crate) cfg_path: Option<PathBuf>,
    pub(crate) socket: UdpSocket,
    pub(crate) world: World,
    pub(crate) info: MapInfo,
    pub(crate) nav: Nav,
    pub(crate) rng: Rng,
    pub(crate) log: Box<dyn Fn(&str) + Send>,
    pub(crate) players: Vec<Player>,
    pub(crate) conns: HashMap<SocketAddr, Conn>,
    pub(crate) time: f64,
    pub(crate) tick: u64,
    pub(crate) phase: Phase,
    pub(crate) timer: f32,
    pub(crate) round: u16,
    pub(crate) round_elapsed: f32,
    pub(crate) score_t: u16,
    pub(crate) score_ct: u16,
    pub(crate) loss_t: u32,
    pub(crate) loss_ct: u32,
    pub(crate) half_swapped: bool,
    pub(crate) bomb: BombState,
    pub(crate) t_site: u8,
    pub(crate) quit: bool,
    pub(crate) nades: Vec<Nade>,
    pub(crate) areas: Vec<Area>,
    pub(crate) next_nade: u16,
    pub(crate) drops: Vec<Drop>,
    pub(crate) next_drop: u16,
    /// Celui qui vient de lâcher la bombe ne la reprend pas aussitôt : (joueur, à partir de quand).
    pub(crate) bomb_block: Option<(PlayerId, f64)>,
}

/// Intersection rayon / boîte ; renvoie la distance d'entrée.
fn ray_aabb(o: Vec3, d: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let (mut t0, mut t1) = (0.0f32, f32::INFINITY);
    for i in 0..3 {
        if d[i].abs() < 1e-8 {
            if o[i] < min[i] || o[i] > max[i] {
                return None;
            }
        } else {
            let (a, b) = ((min[i] - o[i]) / d[i], (max[i] - o[i]) / d[i]);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
            if t0 > t1 {
                return None;
            }
        }
    }
    Some(t0)
}

/// Teste les zones de touche d'un joueur : 0 tête, 1 corps, 2 jambes.
fn ray_player(o: Vec3, d: Vec3, feet: Vec3, crouch: bool) -> Option<(f32, u8)> {
    let boxes: [(f32, f32, f32, u8); 3] = if crouch {
        [(0.95, 1.30, 0.16, 0), (0.45, 0.95, 0.27, 1), (0.0, 0.45, 0.24, 2)]
    } else {
        [(1.42, 1.76, 0.16, 0), (0.78, 1.42, 0.27, 1), (0.0, 0.78, 0.22, 2)]
    };
    let mut best: Option<(f32, u8)> = None;
    for (y0, y1, hw, group) in boxes {
        let min = feet + vec3(-hw, y0, -hw);
        let max = feet + vec3(hw, y1, hw);
        if let Some(t) = ray_aabb(o, d, min, max) {
            if best.map_or(true, |b| t < b.0) {
                best = Some((t, group));
            }
        }
    }
    best
}

fn clock(t: f64) -> String {
    let s = t as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

impl Server {
    pub fn bind(cfg: ServerConfig, cfg_path: Option<PathBuf>, log: Box<dyn Fn(&str) + Send>) -> Result<Server, String> {
        let host = if cfg.local_only { "127.0.0.1" } else { "0.0.0.0" };
        let socket = UdpSocket::bind((host, cfg.port)).map_err(|e| format!("Impossible d'ouvrir le port {} : {}", cfg.port, e))?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        let id = map::find(&cfg.map).unwrap_or_else(|| {
            log(&format!("Carte « {} » inconnue : {} utilisée à la place", cfg.map, map::MAPS[0].name));
            0
        });
        let (world, info) = map::build(id);
        let nav = Nav::build(&world, info.t_spawns[0].0);
        let mut cfg = cfg;
        cfg.map = info.name.into();
        Ok(Server {
            cfg,
            cfg_path,
            socket,
            world,
            info,
            nav,
            rng: Rng::from_time(),
            log,
            players: Vec::new(),
            conns: HashMap::new(),
            time: 0.0,
            tick: 0,
            phase: Phase::Warmup,
            timer: 0.0,
            round: 0,
            round_elapsed: 0.0,
            score_t: 0,
            score_ct: 0,
            loss_t: 0,
            loss_ct: 0,
            half_swapped: false,
            bomb: BombState::None,
            t_site: 0,
            quit: false,
            nades: Vec::new(),
            areas: Vec::new(),
            next_nade: 0,
            drops: Vec::new(),
            next_drop: 0,
            bomb_block: None,
        })
    }

    pub fn map_name(&self) -> &'static str {
        self.info.name
    }

    pub fn port(&self) -> u16 {
        self.socket.local_addr().map(|a| a.port()).unwrap_or(self.cfg.port)
    }

    pub(crate) fn say(&self, s: &str) {
        (self.log)(&format!("[{}] {}", clock(self.time), s));
    }

    /// Boucle principale ; rend la main quand `stop` passe à vrai ou sur la commande `quit`.
    pub fn run(&mut self, cmds: Receiver<String>, stop: Arc<AtomicBool>) {
        let dt = 1.0 / TICK_RATE as f32;
        let start = Instant::now();
        let mut next = 0.0f64;
        if self.cfg.run_empty {
            self.start_match();
        }
        while !stop.load(Ordering::Relaxed) && !self.quit {
            let now = if self.cfg.fast { next } else { start.elapsed().as_secs_f64() };
            if now < next {
                let idle = self.phase == Phase::Warmup && self.conns.is_empty();
                std::thread::sleep(Duration::from_millis(if idle { 5 } else { 1 }));
                continue;
            }
            if now - next > 0.25 {
                next = now;
            }
            next += dt as f64;
            self.time = next;
            self.tick += 1;
            self.receive();
            while let Ok(line) = cmds.try_recv() {
                self.command(&line);
            }
            self.update(dt);
            if self.tick % 2 == 0 {
                self.send_snapshots();
            }
        }
        // Dernier paquet pour prévenir les clients.
        self.broadcast(ServerMsg::Reject("Le serveur s'est arrêté".into()));
        self.send_snapshots();
    }

    // ------------------------------------------------------------------ réseau

    fn receive(&mut self) {
        let mut buf = [0u8; 2048];
        for _ in 0..2000 {
            match self.socket.recv_from(&mut buf) {
                Ok((n, addr)) => self.handle_packet(addr, &buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => continue,
            }
        }
        let dead: Vec<SocketAddr> = self
            .conns
            .iter()
            .filter(|(_, c)| self.time - c.last_recv > TIMEOUT || c.chan.backlog() > 4000)
            .map(|(a, _)| *a)
            .collect();
        for a in dead {
            self.drop_conn(a, "connexion perdue");
        }
    }

    fn handle_packet(&mut self, addr: SocketAddr, data: &[u8]) {
        if let Some(conn) = self.conns.get_mut(&addr) {
            let Some((msgs, state)) = conn.chan.receive::<ClientMsg, ClientState>(data) else { return };
            conn.last_recv = self.time;
            let pid = conn.pid;
            for m in msgs {
                self.handle_msg(pid, addr, m);
            }
            if let Some(st) = state {
                self.handle_state(pid, st);
            }
            return;
        }
        let mut chan = Channel::<ServerMsg>::new();
        let Some((msgs, _)) = chan.receive::<ClientMsg, ClientState>(data) else { return };
        let Some(ClientMsg::Join { version, name }) = msgs.into_iter().next() else { return };
        let humans = self.players.iter().filter(|p| !p.is_bot).count();
        let reject = if version != PROTOCOL {
            Some("Version du jeu incompatible avec le serveur")
        } else if humans >= self.cfg.max_players as usize {
            Some("Le serveur est plein")
        } else {
            None
        };
        if let Some(r) = reject {
            chan.send_reliable(ServerMsg::Reject(r.into()));
            let _ = self.socket.send_to(&chan.build::<Snapshot>(None), addr);
            return;
        }
        // Nom : nettoyé et rendu unique.
        let mut name: String = name.chars().filter(|c| !c.is_control()).take(16).collect::<String>().trim().to_string();
        if name.is_empty() {
            name = "Joueur".into();
        }
        let base = name.clone();
        let mut k = 2;
        while self.players.iter().any(|p| p.name == name) {
            name = format!("{} ({})", base, k);
            k += 1;
        }
        let count = |s: &Server, t: Team, humans: bool| s.players.iter().filter(|p| p.team == t && (!humans || !p.is_bot)).count();
        let team = match count(self, Team::T, true).cmp(&count(self, Team::CT, true)) {
            std::cmp::Ordering::Less => Team::T,
            std::cmp::Ordering::Greater => Team::CT,
            _ => {
                if self.rng.chance(0.5) {
                    Team::T
                } else {
                    Team::CT
                }
            }
        };
        let pid = self.free_id();
        chan.send_reliable(ServerMsg::Welcome {
            id: pid,
            cfg: MatchCfg {
                server_name: self.cfg.name.clone(),
                map: self.info.id,
                freeze_time: self.cfg.freeze_time,
                buy_time: self.cfg.buy_time,
                round_time: self.cfg.round_time,
                bomb_time: self.cfg.bomb_time,
                max_rounds: self.cfg.max_rounds,
            },
        });
        self.conns.insert(addr, Conn { pid, chan, last_recv: self.time });
        self.players.push(Player::new(pid, name.clone(), team, self.cfg.start_money, None));
        self.say(&format!("{} a rejoint la partie ({}) depuis {}", name, team.label(), addr));
        self.chat_all(None, format!("{} a rejoint les {}", name, team.label()));
        if self.phase == Phase::Warmup {
            self.start_match();
        } else {
            self.balance_bots();
            if self.phase == Phase::Freeze {
                if let Some(i) = self.idx(pid) {
                    self.equip_fresh(i);
                    self.spawn(i);
                }
            }
        }
    }

    fn free_id(&self) -> PlayerId {
        (0..=250u8).find(|i| self.players.iter().all(|p| p.id != *i)).unwrap_or(250)
    }

    pub(crate) fn idx(&self, id: PlayerId) -> Option<usize> {
        self.players.iter().position(|p| p.id == id)
    }

    pub(crate) fn send_to(&mut self, pid: PlayerId, msg: ServerMsg) {
        if let Some(c) = self.conns.values_mut().find(|c| c.pid == pid) {
            c.chan.send_reliable(msg);
        }
    }

    pub(crate) fn broadcast(&mut self, msg: ServerMsg) {
        for c in self.conns.values_mut() {
            c.chan.send_reliable(msg.clone());
        }
    }

    fn broadcast_except(&mut self, pid: PlayerId, msg: ServerMsg) {
        for c in self.conns.values_mut().filter(|c| c.pid != pid) {
            c.chan.send_reliable(msg.clone());
        }
    }

    fn chat_all(&mut self, from: Option<(String, Team)>, text: String) {
        let (name, team) = match from {
            Some((n, t)) => (n, Some(t)),
            None => (String::new(), None),
        };
        self.broadcast(ServerMsg::Chat { from: name, team, text });
    }

    fn drop_conn(&mut self, addr: SocketAddr, why: &str) {
        let Some(conn) = self.conns.remove(&addr) else { return };
        if let Some(i) = self.idx(conn.pid) {
            let p = self.players.remove(i);
            if self.bomb == BombState::Carried(p.id) {
                self.bomb = if p.alive { BombState::Dropped(p.pos) } else { BombState::None };
            }
            self.say(&format!("{} a quitté la partie ({})", p.name, why));
            self.chat_all(None, format!("{} a quitté la partie", p.name));
        }
        if self.conns.is_empty() && !self.cfg.run_empty {
            self.phase = Phase::Warmup;
            self.players.clear();
            self.drops.clear();
            self.bomb = BombState::None;
            self.say("Plus aucun joueur : serveur en attente");
        } else {
            self.balance_bots();
        }
    }

    fn handle_state(&mut self, pid: PlayerId, st: ClientState) {
        let frozen = self.phase == Phase::Freeze;
        let Some(i) = self.idx(pid) else { return };
        let p = &mut self.players[i];
        if st.n <= p.last_state && p.last_state - st.n < 100_000 {
            return;
        }
        if !(st.pos.is_finite() && st.yaw.is_finite() && st.pitch.is_finite()) {
            return;
        }
        p.last_state = st.n;
        p.yaw = st.yaw;
        p.pitch = st.pitch.clamp(-1.57, 1.57);
        p.crouch = st.crouch;
        p.using = st.using;
        p.reloading = st.reloading;
        p.air = st.air;
        let holds_nade = is_grenade(st.weapon) && p.nades[(st.weapon - GRENADE_BASE) as usize] > 0;
        p.weapon = if st.weapon != 0 && (st.weapon == p.primary || st.weapon == p.secondary || holds_nade) { st.weapon } else { KNIFE };
        if p.alive && !frozen {
            p.pos = st.pos;
        }
    }

    fn handle_msg(&mut self, pid: PlayerId, addr: SocketAddr, msg: ClientMsg) {
        let Some(i) = self.idx(pid) else { return };
        match msg {
            ClientMsg::Join { .. } => {}
            ClientMsg::Leave => self.drop_conn(addr, "déconnexion"),
            ClientMsg::Fire { weapon: wid, origin, dir, render_time, heavy } => {
                if !matches!(self.phase, Phase::Live | Phase::RoundEnd) {
                    return;
                }
                let time = self.time;
                let p = &mut self.players[i];
                if !p.alive || !dir.is_finite() || !origin.is_finite() || dir.length_squared() < 0.5 {
                    return;
                }
                if wid != KNIFE && wid != p.primary && wid != p.secondary {
                    return;
                }
                // Le coup de couteau puissant coûte plus cher : son délai est imposé par le serveur.
                let heavy = heavy && wid == KNIFE;
                let cost = if heavy { KNIFE_HEAVY_COOLDOWN / KNIFE_COOLDOWN * 1.05 } else { 1.0 };
                if p.fire_budget < cost {
                    return;
                }
                p.fire_budget -= cost;
                let eye = p.pos + Vec3::Y * eye_height(if p.crouch { 1.0 } else { 0.0 });
                let origin = if origin.distance(eye) > 1.5 { eye } else { origin };
                let rt = if render_time.is_finite() { render_time.clamp(time - 0.4, time) } else { time };
                self.apply_shot(i, origin, dir.normalize(), wid, rt, if heavy { KNIFE_HEAVY_MULT } else { 1.0 });
            }
            ClientMsg::Throw { kind, origin, dir, strong } => self.throw_nade(i, kind, origin, dir, strong),
            ClientMsg::Pickup { id, mag, reserve } => self.pickup(i, id, mag, reserve, false),
            ClientMsg::Drop { slot, mag, reserve } => self.throw_weapon(i, slot, mag, reserve),
            ClientMsg::DropBomb => self.drop_bomb(i),
            ClientMsg::Buy(item) => {
                self.try_buy(i, item);
            }
            ClientMsg::Sell(item) => {
                self.sell(i, item);
            }
            ClientMsg::SwitchTeam => {
                let p = &mut self.players[i];
                p.switch_team = !p.switch_team;
                let text = if p.switch_team {
                    "Vous changerez d'équipe au prochain round".to_string()
                } else {
                    "Changement d'équipe annulé".to_string()
                };
                self.send_to(pid, ServerMsg::Chat { from: String::new(), team: None, text });
            }
            ClientMsg::Chat(text) => {
                let text: String = text.chars().filter(|c| !c.is_control()).take(120).collect();
                if text.trim().is_empty() {
                    return;
                }
                let (name, team) = (self.players[i].name.clone(), self.players[i].team);
                self.say(&format!("<{}> {}", name, text));
                self.chat_all(Some((name, team)), text);
            }
        }
    }

    fn send_snapshots(&mut self) {
        if self.conns.is_empty() {
            return;
        }
        self.sync_refunds();
        let time_left = match (self.phase, self.bomb) {
            (Phase::Live, BombState::Planted { time_left, .. }) => time_left,
            _ => self.timer,
        };
        let snap = Snapshot {
            time: self.time,
            phase: self.phase,
            time_left,
            round: self.round,
            score_t: self.score_t,
            score_ct: self.score_ct,
            bomb: self.bomb,
            players: self
                .players
                .iter()
                .map(|p| PlayerSnap {
                    id: p.id,
                    name: p.name.clone(),
                    bot: p.is_bot,
                    team: p.team,
                    alive: p.alive,
                    pos: p.pos,
                    yaw: p.yaw,
                    pitch: p.pitch,
                    crouch: p.crouch,
                    weapon: p.weapon,
                    primary: p.primary,
                    secondary: p.secondary,
                    health: p.health.clamp(0, 255) as u8,
                    armor: p.armor.clamp(0, 255) as u8,
                    helmet: p.helmet,
                    kit: p.kit,
                    money: p.money,
                    kills: p.kills,
                    assists: p.assists,
                    deaths: p.deaths,
                    action: p.action,
                    progress: p.progress,
                    reloading: p.reloading,
                    air: p.air,
                    nades: p.nades,
                })
                .collect(),
            nades: self.nades.iter().map(|n| NadeSnap { id: n.id, kind: n.kind, pos: n.pos }).collect(),
            areas: self.areas.iter().map(|a| AreaSnap { id: a.id, kind: a.kind, pos: a.pos, time_left: a.time_left }).collect(),
            drops: self.drops.iter().map(|d| DropSnap { id: d.id, weapon: d.weapon, pos: d.pos, yaw: d.yaw }).collect(),
        };
        for (addr, c) in self.conns.iter() {
            let _ = self.socket.send_to(&c.chan.build(Some(&snap)), addr);
        }
    }

    // ------------------------------------------------------------------ partie

    fn count(&self, team: Team) -> usize {
        self.players.iter().filter(|p| p.team == team).count()
    }

    fn alive(&self, team: Team) -> usize {
        self.players.iter().filter(|p| p.team == team && p.alive).count()
    }

    /// Ajoute ou retire des bots pour que chaque équipe ait `team_size` joueurs.
    fn balance_bots(&mut self) {
        for team in [Team::T, Team::CT] {
            loop {
                let total = self.count(team);
                let bots = self.players.iter().filter(|p| p.team == team && p.is_bot).count();
                let want = self.cfg.team_size as usize;
                if self.cfg.fill_bots && total < want {
                    let id = self.free_id();
                    let name = BOT_NAMES
                        .iter()
                        .map(|n| format!("Bot {}", n))
                        .find(|n| self.players.iter().all(|p| &p.name != n))
                        .unwrap_or_else(|| format!("Bot {}", id));
                    let skill = self.cfg.bot_skill + self.rng.range(-0.1, 0.1);
                    self.players.push(Player::new(id, name, team, self.cfg.start_money, Some(Box::new(Bot::new(skill)))));
                } else if bots > 0 && (total > want || !self.cfg.fill_bots) {
                    // Retire de préférence un bot déjà mort.
                    let pick = self
                        .players
                        .iter()
                        .rposition(|p| p.team == team && p.is_bot && !p.alive)
                        .or_else(|| self.players.iter().rposition(|p| p.team == team && p.is_bot));
                    if let Some(i) = pick {
                        let p = self.players.remove(i);
                        if self.bomb == BombState::Carried(p.id) {
                            self.bomb = BombState::Dropped(p.pos);
                        }
                    }
                } else {
                    break;
                }
            }
        }
    }

    fn start_match(&mut self) {
        self.round = 0;
        self.score_t = 0;
        self.score_ct = 0;
        self.loss_t = 0;
        self.loss_ct = 0;
        self.half_swapped = false;
        let money = self.cfg.start_money;
        for p in &mut self.players {
            p.money = money;
            p.kills = 0;
            p.assists = 0;
            p.deaths = 0;
            p.fresh = true;
            p.alive = false;
        }
        self.say("Début du match");
        self.start_round();
    }

    /// Charge une autre carte et relance le match dessus.
    fn change_map(&mut self, id: u8) {
        let (world, info) = map::build(id);
        self.nav = Nav::build(&world, info.t_spawns[0].0);
        self.world = world;
        self.info = info;
        self.cfg.map = self.info.name.into();
        self.bomb = BombState::None;
        self.nades.clear();
        self.areas.clear();
        self.drops.clear();
        for p in &mut self.players {
            p.history.clear();
            if let Some(b) = p.bot.as_mut() {
                b.forget();
            }
        }
        self.broadcast(ServerMsg::MapChange { map: id });
        self.chat_all(None, format!("[Serveur] Changement de carte : {}", self.info.name));
        self.say(&format!("Carte : {}", self.info.name));
        if self.conns.is_empty() && !self.cfg.run_empty {
            self.phase = Phase::Warmup;
        } else {
            self.start_match();
        }
    }

    fn equip_fresh(&mut self, i: usize) {
        let p = &mut self.players[i];
        p.primary = 0;
        p.secondary = DEFAULT_PISTOL;
        p.armor = 0;
        p.helmet = false;
        p.kit = false;
        p.nades = [0; 4];
        p.fresh = false;
    }

    /// Fait apparaître le joueur `i` à un point de départ de son équipe.
    fn spawn(&mut self, i: usize) {
        let team = self.players[i].team;
        let rank = self.players[..i].iter().filter(|p| p.team == team).count();
        let list = if team == Team::T { &self.info.t_spawns } else { &self.info.ct_spawns };
        let offset = (self.round as usize * 3) % list.len();
        let (pos, yaw) = list[(rank + offset) % list.len()];
        let site = (rank % 2) as u8;
        let p = &mut self.players[i];
        p.alive = true;
        p.health = 100;
        p.damage_from.clear();
        p.pos = pos;
        p.yaw = yaw;
        p.pitch = 0.0;
        p.crouch = false;
        p.using = false;
        p.action = 0;
        p.progress = 0.0;
        p.history.clear();
        p.fire_budget = 2.0;
        p.weapon = if p.primary != 0 { p.primary } else { p.secondary };
        if let Some(b) = p.bot.as_mut() {
            b.reset(pos, yaw, site);
        }
        let pid = p.id;
        if !p.is_bot {
            self.send_to(pid, ServerMsg::Spawn { pos, yaw });
        }
    }

    fn start_round(&mut self) {
        self.round += 1;
        for p in &mut self.players {
            if p.switch_team {
                p.switch_team = false;
                p.team = p.team.other();
                p.fresh = true;
                p.alive = false;
            }
        }
        self.balance_bots();
        self.bomb = BombState::None;
        self.nades.clear();
        self.areas.clear();
        self.drops.clear();
        self.t_site = self.rng.below(2) as u8;
        self.bomb_block = None;
        for p in &mut self.players {
            p.bought.clear();
        }
        for i in 0..self.players.len() {
            if !self.players[i].alive || self.players[i].fresh {
                self.equip_fresh(i);
            }
            self.spawn(i);
        }
        let ts: Vec<PlayerId> = self.players.iter().filter(|p| p.team == Team::T).map(|p| p.id).collect();
        if !ts.is_empty() {
            self.bomb = BombState::Carried(ts[self.rng.below(ts.len())]);
        }
        self.phase = Phase::Freeze;
        self.timer = self.cfg.freeze_time;
        self.round_elapsed = 0.0;
        for i in 0..self.players.len() {
            if self.players[i].is_bot {
                self.bot_buy(i);
            }
        }
        self.broadcast(ServerMsg::RoundStart { round: self.round });
        self.say(&format!(
            "Round {} — Attaquants {} : {} Défenseurs",
            self.round, self.score_t, self.score_ct
        ));
        if self.cfg.fast {
            self.say(&format!("  site visé {}", if self.t_site == 0 { "A" } else { "B" }));
        }
    }

    fn end_round(&mut self, winner: Team, reason: EndReason) {
        if self.phase != Phase::Live {
            return;
        }
        let planted = matches!(self.bomb, BombState::Planted { .. } | BombState::Exploded(_) | BombState::Defused(_));
        if winner == Team::T {
            self.score_t += 1;
            self.loss_t = 0;
            self.loss_ct += 1;
        } else {
            self.score_ct += 1;
            self.loss_ct = 0;
            self.loss_t += 1;
        }
        let win_bonus = if matches!(reason, EndReason::Exploded | EndReason::Defused) { 3500 } else { 3250 };
        let (loss_t, loss_ct) = (self.loss_t, self.loss_ct);
        for p in &mut self.players {
            let bonus = if p.team == winner {
                win_bonus
            } else {
                let streak = if p.team == Team::T { loss_t } else { loss_ct };
                let mut b = 1400 + 500 * (streak.saturating_sub(1)).min(4);
                if p.team == Team::T && planted {
                    b += 800;
                }
                b
            };
            p.money = (p.money + bonus).min(MONEY_MAX);
            p.action = 0;
            p.progress = 0.0;
        }
        self.phase = Phase::RoundEnd;
        self.timer = ROUND_END_TIME;
        self.broadcast(ServerMsg::RoundEnd { winner, reason });
        let why = match reason {
            EndReason::Elimination => "équipe adverse éliminée",
            EndReason::Time => "temps écoulé",
            EndReason::Exploded => "la bombe a explosé",
            EndReason::Defused => "bombe désamorcée",
        };
        self.say(&format!(
            "Victoire des {} ({}) — {} : {}",
            winner.label(),
            why,
            self.score_t,
            self.score_ct
        ));
    }

    fn after_round(&mut self) {
        let half = (self.cfg.max_rounds / 2).max(1);
        if self.score_t > half || self.score_ct > half || self.round >= self.cfg.max_rounds {
            let winner = match self.score_t.cmp(&self.score_ct) {
                std::cmp::Ordering::Greater => Some(Team::T),
                std::cmp::Ordering::Less => Some(Team::CT),
                _ => None,
            };
            self.phase = Phase::MatchEnd;
            self.timer = MATCH_END_TIME;
            self.broadcast(ServerMsg::MatchEnd { winner });
            self.say(&format!("Fin du match : {} à {}", self.score_t, self.score_ct));
        } else if self.round == half && !self.half_swapped {
            self.half_swapped = true;
            let money = self.cfg.start_money;
            for p in &mut self.players {
                p.team = p.team.other();
                p.money = money;
                p.fresh = true;
                p.alive = false;
            }
            std::mem::swap(&mut self.score_t, &mut self.score_ct);
            self.loss_t = 0;
            self.loss_ct = 0;
            self.chat_all(None, "Mi-temps : les équipes changent de côté".into());
            self.say("Mi-temps : changement de côté");
            self.start_round();
        } else {
            self.start_round();
        }
    }

    fn update(&mut self, dt: f32) {
        match self.phase {
            Phase::Warmup => return,
            Phase::Freeze => {
                self.timer -= dt;
                if self.timer <= 0.0 {
                    self.phase = Phase::Live;
                    self.timer = self.cfg.round_time;
                }
            }
            Phase::Live => {
                self.round_elapsed += dt;
                if !matches!(self.bomb, BombState::Planted { .. }) {
                    self.timer -= dt;
                }
                self.update_bomb(dt);
                self.update_nades(dt);
                self.check_win();
            }
            Phase::RoundEnd => {
                self.update_nades(dt);
                self.timer -= dt;
                if self.timer <= 0.0 {
                    self.after_round();
                }
            }
            Phase::MatchEnd => {
                self.timer -= dt;
                if self.timer <= 0.0 {
                    self.start_match();
                }
            }
        }
        self.update_bots(dt);
        let time = self.time;
        for p in &mut self.players {
            let rate = weapon(p.weapon).rpm / 60.0;
            p.fire_budget = (p.fire_budget + dt * rate * 1.15).min(3.0);
            if p.alive {
                p.history.push_back((time, p.pos, p.crouch));
                while p.history.front().is_some_and(|h| h.0 < time - 0.6) {
                    p.history.pop_front();
                }
            }
        }
    }

    fn check_win(&mut self) {
        if self.phase != Phase::Live {
            return;
        }
        let planted = matches!(self.bomb, BombState::Planted { .. });
        let (nt, nct) = (self.count(Team::T), self.count(Team::CT));
        if nct > 0 && nt > 0 && self.alive(Team::CT) == 0 {
            self.end_round(Team::T, EndReason::Elimination);
        } else if nt > 0 && nct > 0 && self.alive(Team::T) == 0 && !planted {
            self.end_round(Team::CT, EndReason::Elimination);
        } else if self.timer <= 0.0 && !planted {
            self.end_round(Team::CT, EndReason::Time);
        }
    }

    pub(crate) fn update_bomb(&mut self, dt: f32) {
        match self.bomb {
            BombState::Carried(id) => {
                let Some(i) = self.idx(id) else {
                    self.bomb = BombState::None;
                    return;
                };
                let site = self.info.site_of(self.players[i].pos);
                let p = &mut self.players[i];
                match site {
                    Some(site) if p.alive && p.using => {
                        if p.action != 1 || p.pos.distance(p.action_pos) > 0.5 {
                            p.action = 1;
                            p.progress = 0.0;
                            p.action_pos = p.pos;
                        }
                        p.progress += dt / PLANT_TIME;
                        if p.progress >= 1.0 {
                            p.action = 0;
                            p.progress = 0.0;
                            p.money = (p.money + 300).min(MONEY_MAX);
                            let (pos, name) = (p.pos, p.name.clone());
                            self.bomb = BombState::Planted { pos, time_left: self.cfg.bomb_time, site };
                            self.broadcast(ServerMsg::BombPlanted { site });
                            self.say(&format!("{} a posé la bombe sur le site {}", name, if site == 0 { "A" } else { "B" }));
                        }
                    }
                    _ => {
                        p.action = 0;
                        p.progress = 0.0;
                    }
                }
            }
            BombState::Dropped(pos) => {
                // Celui qui l'a lâchée peut la reprendre après 1 s, une fois éloigné.
                if let Some((owner, at)) = self.bomb_block {
                    let near = self.players.iter().any(|p| p.id == owner && p.alive && hdist(p.pos, pos) < 1.3);
                    if self.time >= at && !near {
                        self.bomb_block = None;
                    }
                }
                let block = self.bomb_block.map(|(o, _)| o);
                if let Some(p) = self.players.iter().find(|p| {
                    p.alive && p.team == Team::T && Some(p.id) != block && hdist(p.pos, pos) < 1.0 && (p.pos.y - pos.y).abs() < 1.2
                }) {
                    self.bomb = BombState::Carried(p.id);
                    self.bomb_block = None;
                }
            }
            BombState::Planted { pos, time_left, site } => {
                let time_left = time_left - dt;
                self.bomb = BombState::Planted { pos, time_left, site };
                let near = |p: &Player| p.alive && p.team == Team::CT && p.using && p.pos.distance(pos) < 1.8;
                let defuser = self
                    .players
                    .iter()
                    .position(|p| near(p) && p.action == 2)
                    .or_else(|| self.players.iter().position(near));
                for (i, p) in self.players.iter_mut().enumerate() {
                    if Some(i) != defuser {
                        p.action = 0;
                        p.progress = 0.0;
                    }
                }
                if let Some(i) = defuser {
                    let p = &mut self.players[i];
                    p.action = 2;
                    p.progress += dt / if p.kit { 5.0 } else { 10.0 };
                    if p.progress >= 1.0 {
                        p.money = (p.money + 300).min(MONEY_MAX);
                        let name = p.name.clone();
                        self.bomb = BombState::Defused(pos);
                        self.broadcast(ServerMsg::BombDefused);
                        self.say(&format!("{} a désamorcé la bombe", name));
                        self.end_round(Team::CT, EndReason::Defused);
                        return;
                    }
                }
                if time_left <= 0.0 {
                    self.bomb = BombState::Exploded(pos);
                    self.broadcast(ServerMsg::BombExploded { pos });
                    for i in 0..self.players.len() {
                        if !self.players[i].alive {
                            continue;
                        }
                        let d = (self.players[i].pos + Vec3::Y * 0.9).distance(pos);
                        if d < 18.0 {
                            let dmg = 500.0 * (1.0 - d / 18.0).powf(1.5);
                            self.damage(i, None, dmg, 1.0, 1, KNIFE, pos);
                        }
                    }
                    self.end_round(Team::T, EndReason::Exploded);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn can_buy(&self) -> bool {
        self.phase == Phase::Freeze || (self.phase == Phase::Live && self.round_elapsed < self.cfg.buy_time)
    }

    pub(crate) fn try_buy(&mut self, i: usize, item: BuyItem) -> bool {
        let (before, money) = (self.inventory(i), self.players[i].money);
        if !self.buy_item(i, item) {
            return false;
        }
        let paid = money - self.players[i].money;
        self.record_purchase(i, item, before, paid);
        true
    }

    fn buy_item(&mut self, i: usize, item: BuyItem) -> bool {
        if !self.can_buy() {
            return false;
        }
        // Achats réservés à la zone de départ de l'équipe.
        if !self.info.in_buy_zone(self.players[i].team, self.players[i].pos) {
            return false;
        }
        let p = &mut self.players[i];
        if !p.alive {
            return false;
        }
        match item {
            BuyItem::Weapon(id) => {
                if id == KNIFE || id as usize >= WEAPONS.len() {
                    return false;
                }
                let d = weapon(id);
                let slot = if d.class == Class::Pistol { &mut p.secondary } else { &mut p.primary };
                if *slot == id || p.money < d.price {
                    return false;
                }
                *slot = id;
                p.money -= d.price;
            }
            BuyItem::Kevlar => {
                if p.armor >= 100 || p.money < PRICE_KEVLAR {
                    return false;
                }
                p.armor = 100;
                p.money -= PRICE_KEVLAR;
            }
            BuyItem::KevlarHelmet => {
                if p.armor >= 100 && p.helmet {
                    return false;
                }
                let price = if p.armor >= 100 { PRICE_HELMET_ONLY } else { PRICE_KEVLAR_HELMET };
                if p.money < price {
                    return false;
                }
                p.armor = 100;
                p.helmet = true;
                p.money -= price;
            }
            BuyItem::Grenade(k) => {
                let Some(g) = GRENADES.get(k as usize) else { return false };
                let total: u8 = p.nades.iter().sum();
                if p.nades[k as usize] >= g.max || total >= MAX_GRENADES || p.money < g.price {
                    return false;
                }
                p.nades[k as usize] += 1;
                p.money -= g.price;
            }
            BuyItem::Kit => {
                if p.team != Team::CT || p.kit || p.money < PRICE_KIT {
                    return false;
                }
                p.kit = true;
                p.money -= PRICE_KIT;
            }
        }
        true
    }

    fn bot_buy(&mut self, i: usize) {
        let money = self.players[i].money;
        if self.players[i].primary == 0 {
            let special: Option<u8> = if money >= 6500 && self.rng.chance(0.15) {
                Some(11)
            } else if money >= 7500 && self.rng.chance(0.08) {
                Some(12)
            } else {
                None
            };
            let tiers: [&[u8]; 3] = [&[7, 8], &[9, 6], &[5, 4, 10]];
            let pick = special.or_else(|| {
                tiers.iter().find_map(|tier| {
                    let opts: Vec<u8> = tier.iter().copied().filter(|w| weapon(*w).price <= money).collect();
                    (!opts.is_empty()).then(|| opts[self.rng.below(opts.len())])
                })
            });
            if let Some(w) = pick {
                self.try_buy(i, BuyItem::Weapon(w));
            }
        }
        if self.players[i].armor < 60 {
            if self.players[i].money >= PRICE_KEVLAR_HELMET {
                self.try_buy(i, BuyItem::KevlarHelmet);
            } else if self.players[i].money >= PRICE_KEVLAR + 200 {
                self.try_buy(i, BuyItem::Kevlar);
            }
        }
        if self.players[i].primary == 0 && self.players[i].secondary == DEFAULT_PISTOL && self.players[i].money >= 900 && self.rng.chance(0.5) {
            let w = if self.rng.chance(0.5) { 3 } else { 2 };
            self.try_buy(i, BuyItem::Weapon(w));
        }
        if self.players[i].team == Team::CT && self.players[i].money >= 1200 {
            self.try_buy(i, BuyItem::Kit);
        }
        // Quelques grenades quand il reste de l'argent.
        for (kind, chance) in [(FRAG, 0.4), (FLASH, 0.3), (FIRE, 0.15)] {
            if self.players[i].money >= 1000 && self.rng.chance(chance) {
                self.try_buy(i, BuyItem::Grenade(kind));
            }
        }
    }

    fn update_bots(&mut self, dt: f32) {
        let mut shots = Vec::new();
        // Ennemis repérés, partagés au sein de chaque équipe.
        let alert = |team: Team, players: &[Player]| {
            players.iter().filter(|p| p.alive && p.team == team).find_map(|p| p.bot.as_ref().and_then(|b| b.spotted()))
        };
        let (alert_t, alert_ct) = (alert(Team::T, &self.players), alert(Team::CT, &self.players));
        let smokes: Vec<Vec3> = self.areas.iter().filter(|a| a.kind == SMOKE).map(|a| a.pos).collect();
        let fires: Vec<Vec3> = self.areas.iter().filter(|a| a.kind == FIRE).map(|a| a.pos).collect();
        let mut throws = Vec::new();
        for i in 0..self.players.len() {
            if !self.players[i].alive {
                continue;
            }
            let Some(mut bot) = self.players[i].bot.take() else { continue };
            let out = {
                let ctx = BotCtx {
                    world: &self.world,
                    nav: &self.nav,
                    info: &self.info,
                    players: &self.players,
                    me: i,
                    phase: self.phase,
                    bomb: self.bomb,
                    t_site: self.t_site,
                    alert: if self.players[i].team == Team::T { alert_t } else { alert_ct },
                    smokes: &smokes,
                    fires: &fires,
                    dt,
                };
                bot.update(&ctx, &mut self.rng)
            };
            let p = &mut self.players[i];
            p.pos = bot.body.pos;
            p.yaw = bot.yaw;
            p.pitch = bot.pitch;
            p.crouch = bot.body.crouched;
            p.weapon = out.weapon;
            p.using = out.using;
            p.reloading = bot.reloading();
            p.air = !bot.body.on_ground;
            p.bot = Some(bot);
            if let Some(s) = out.shot {
                shots.push((i, s));
            }
            if let Some(t) = out.throw {
                throws.push((i, t));
            }
        }
        self.auto_pickup();
        for (i, (kind, origin, dir)) in throws {
            self.throw_nade(i, kind, origin, dir, true);
        }
        let time = self.time;
        for (i, (origin, dir, wid)) in shots {
            if self.players[i].alive {
                self.apply_shot(i, origin, dir, wid, time, 1.0);
            }
        }
    }

    // ------------------------------------------------------------------ combat

    fn apply_shot(&mut self, shooter: usize, origin: Vec3, dir: Vec3, wid: u8, rewind: f64, power: f32) {
        let def = weapon(wid);
        let range = if wid == KNIFE { KNIFE_RANGE } else { 250.0 };
        let wall = self.world.raycast(origin, dir, range).map_or(range, |h| h.dist);
        let team = self.players[shooter].team;
        let mut best: Option<(usize, f32, u8)> = None;
        for (i, p) in self.players.iter().enumerate() {
            if i == shooter || !p.alive || p.team == team {
                continue;
            }
            let (pos, crouch) = p.pos_at(rewind);
            if let Some((t, group)) = ray_player(origin, dir, pos, crouch) {
                if t < wall && best.map_or(true, |b| t < b.1) {
                    best = Some((i, t, group));
                }
            }
        }
        let end = origin + dir * best.map_or(wall, |b| b.1);
        let (sid, bot) = (self.players[shooter].id, self.players[shooter].is_bot);
        let msg = ServerMsg::Shot { shooter: sid, weapon: wid, origin, end, hit_player: best.is_some() };
        if bot {
            self.broadcast(msg);
        } else {
            self.broadcast_except(sid, msg);
        }
        if let Some((victim, dist, group)) = best {
            let mult = match group {
                0 => def.head_mult,
                1 => 1.0,
                _ => 0.75,
            };
            let dmg = def.damage * def.range_mod.powf(dist / 12.0) * mult * power;
            self.damage(victim, Some(shooter), dmg, def.armor_pen, group, wid, end);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn damage(&mut self, victim: usize, attacker: Option<usize>, dmg: f32, pen: f32, group: u8, wid: u8, hit_pos: Vec3) {
        let from = attacker.map_or(hit_pos, |a| self.players[a].pos);
        let v = &mut self.players[victim];
        let head = group == 0;
        let armored = v.armor > 0 && match group {
            0 => v.helmet,
            1 => true,
            _ => false,
        };
        let mut hp = dmg;
        if armored && pen < 1.0 {
            hp = dmg * pen;
            v.armor = (v.armor - ((dmg - hp) * 0.5).ceil() as i32).max(0);
        }
        let d = hp.round().max(1.0) as i32;
        let dealt = d.min(v.health.max(0));
        v.health -= d;
        let killed = v.health <= 0;
        let (vid, vbot, vteam) = (v.id, v.is_bot, v.team);
        // Dégâts d'un adversaire : comptés pour une éventuelle assistance.
        if let Some(a) = attacker.filter(|a| *a != victim && self.players[*a].team != vteam) {
            let aid = self.players[a].id;
            let v = &mut self.players[victim];
            match v.damage_from.iter_mut().find(|(id, _)| *id == aid) {
                Some((_, total)) => *total += dealt,
                None => v.damage_from.push((aid, dealt)),
            }
        }
        if !vbot {
            self.send_to(vid, ServerMsg::Hurt { from, damage: d.min(999) as u16 });
        }
        if let Some(a) = attacker {
            let (aid, abot) = (self.players[a].id, self.players[a].is_bot);
            if !abot {
                self.send_to(aid, ServerMsg::Hit { victim: vid, damage: dealt.min(999) as u16, headshot: head, killed, pos: hit_pos });
            }
        }
        if killed {
            self.kill(victim, attacker, wid, head);
        }
    }

    fn kill(&mut self, victim: usize, attacker: Option<usize>, wid: u8, headshot: bool) {
        let v = &mut self.players[victim];
        v.alive = false;
        v.health = 0;
        v.deaths += 1;
        v.action = 0;
        v.progress = 0.0;
        let (vid, vname, vpos) = (v.id, v.name.clone(), v.pos);
        if self.bomb == BombState::Carried(vid) {
            // Elle tombe au sol, même si le porteur meurt en plein saut.
            self.bomb = BombState::Dropped(self.ground_below(vpos));
        }
        // Son arme reste au sol : la principale, sinon le pistolet.
        let v = &mut self.players[victim];
        let dropped = if v.primary != 0 { std::mem::take(&mut v.primary) } else { std::mem::take(&mut v.secondary) };
        self.drop_weapon(dropped, vpos, None);
        let mut killer = None;
        if let Some(a) = attacker {
            let p = &mut self.players[a];
            p.kills += 1;
            let reward = if is_grenade(wid) { 300 } else { weapon(wid).kill_reward };
            p.money = (p.money + reward).min(MONEY_MAX);
            killer = Some(p.id);
            let msg = format!(
                "{} a éliminé {} ({}{})",
                p.name,
                vname,
                item_name(wid),
                if headshot { ", tir à la tête" } else { "" }
            );
            self.say(&msg);
            if self.cfg.fast {
                let (team, kp) = (self.players[victim].team, self.players[a].pos);
                self.say(&format!(
                    "  mort {:?} {:.1} {:.1} tueur {:.1} {:.1} t={:.1}",
                    team, vpos.x, vpos.z, kp.x, kp.z, self.round_elapsed
                ));
            }
        } else {
            self.say(&format!("{} est mort dans l'explosion", vname));
        }
        // Assistances : tout adversaire autre que le tueur qui a infligé assez de dégâts.
        let helpers: Vec<(PlayerId, i32)> = std::mem::take(&mut self.players[victim].damage_from)
            .into_iter()
            .filter(|(id, d)| *d >= ASSIST_DAMAGE && Some(*id) != killer)
            .collect();
        for (id, _) in &helpers {
            if let Some(j) = self.idx(*id) {
                self.players[j].assists += 1;
            }
        }
        let assist = helpers.iter().max_by_key(|(_, d)| *d).map(|(id, _)| *id);
        self.broadcast(ServerMsg::Kill { killer, assist, victim: vid, weapon: wid, headshot });
    }

    // ------------------------------------------------------------------ administration

    fn find_player(&self, key: &str) -> Option<usize> {
        if let Ok(id) = key.parse::<u8>() {
            if let Some(i) = self.idx(id) {
                return Some(i);
            }
        }
        let k = key.to_lowercase();
        self.players.iter().position(|p| p.name.to_lowercase().contains(&k))
    }

    /// Exécute une commande de la console d'administration.
    pub fn command(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let (cmd, rest) = line.split_once(' ').map_or((line, ""), |(a, b)| (a, b.trim()));
        macro_rules! out {
            ($s:expr) => {
                (self.log)($s)
            };
        }
        match cmd.to_lowercase().as_str() {
            "help" | "aide" | "?" => {
                out!("Commandes disponibles :");
                out!("  status                 état du serveur et liste des joueurs");
                out!("  say <message>          envoie un message à tous les joueurs");
                out!("  kick <id|nom>          expulse un joueur");
                out!("  bots <on|off>          active ou désactive les bots");
                out!("  map [nom|numéro]       liste les cartes ou change de carte (relance le match)");
                out!("  teamsize <n>           joueurs par équipe (bots compris)");
                out!("  difficulty <0-100>     niveau des bots");
                out!("  money <id|nom|all> <n> donne de l'argent");
                out!("  set <clé> <valeur>     round_time, freeze_time, buy_time, bomb_time,");
                out!("                         max_rounds, start_money, max_players, name");
                out!("  config                 affiche la configuration");
                out!("  save                   enregistre la configuration dans server.json");
                out!("  restart                relance le match");
                out!("  quit                   arrête le serveur");
            }
            "status" => {
                let phase = match self.phase {
                    Phase::Warmup => "en attente de joueurs",
                    Phase::Freeze => "préparation",
                    Phase::Live => "round en cours",
                    Phase::RoundEnd => "fin de round",
                    Phase::MatchEnd => "fin de match",
                };
                out!(&format!(
                    "{} — port {} — carte {} — {}",
                    self.cfg.name,
                    self.port(),
                    self.info.name,
                    phase
                ));
                out!(&format!(
                    "Round {}/{} — Attaquants {} : {} Défenseurs",
                    self.round, self.cfg.max_rounds, self.score_t, self.score_ct
                ));
                out!("  id  équipe      nom                   K/M     argent  adresse");
                for p in &self.players {
                    let addr = self
                        .conns
                        .iter()
                        .find(|(_, c)| c.pid == p.id)
                        .map_or("bot".to_string(), |(a, _)| a.to_string());
                    out!(&format!(
                        "  {:<3} {:<11} {:<21} {:>2}/{:<4} {:>6}  {}{}",
                        p.id,
                        p.team.label(),
                        p.name,
                        p.kills,
                        p.deaths,
                        p.money,
                        addr,
                        if p.alive { "" } else { "  (mort)" }
                    ));
                }
                if self.players.is_empty() {
                    out!("  (aucun joueur)");
                }
            }
            "say" => {
                if !rest.is_empty() {
                    self.chat_all(None, format!("[Serveur] {}", rest));
                    self.say(&format!("[Serveur] {}", rest));
                }
            }
            "kick" => match self.find_player(rest) {
                Some(i) if !self.players[i].is_bot => {
                    let pid = self.players[i].id;
                    if let Some((addr, _)) = self.conns.iter().find(|(_, c)| c.pid == pid).map(|(a, c)| (*a, c.pid)) {
                        if let Some(c) = self.conns.get_mut(&addr) {
                            c.chan.send_reliable(ServerMsg::Reject("Vous avez été expulsé du serveur".into()));
                            let _ = self.socket.send_to(&c.chan.build::<Snapshot>(None), addr);
                        }
                        self.drop_conn(addr, "expulsé");
                    }
                }
                Some(_) => out!("Impossible d'expulser un bot : utilisez « bots off » ou « teamsize »"),
                None => out!("Joueur introuvable"),
            },
            "bots" => {
                match rest {
                    "on" | "1" => self.cfg.fill_bots = true,
                    "off" | "0" => self.cfg.fill_bots = false,
                    _ => {
                        out!("Usage : bots <on|off>");
                        return;
                    }
                }
                if self.phase != Phase::Warmup {
                    self.balance_bots();
                }
                out!(if self.cfg.fill_bots { "Bots activés" } else { "Bots désactivés" });
            }
            "map" | "carte" => {
                if rest.is_empty() {
                    out!("Cartes disponibles (« map <nom ou numéro> » pour en changer) :");
                    for (i, m) in map::MAPS.iter().enumerate() {
                        let mark = if i as u8 == self.info.id { "*" } else { " " };
                        out!(&format!(" {} {}  {:<10} {}", mark, i + 1, m.name, m.desc));
                    }
                } else {
                    match map::find(rest) {
                        Some(id) if id == self.info.id => out!(&format!("La carte {} est déjà en cours", self.info.name)),
                        Some(id) => self.change_map(id),
                        None => out!("Carte inconnue — tapez « map » pour la liste"),
                    }
                }
            }
            "teamsize" => match rest.parse::<u8>() {
                Ok(n) if (1..=8).contains(&n) => {
                    self.cfg.team_size = n;
                    if self.phase != Phase::Warmup {
                        self.balance_bots();
                    }
                    out!(&format!("{} joueurs par équipe", n));
                }
                _ => out!("Usage : teamsize <1-8>"),
            },
            "difficulty" | "difficulte" => match rest.parse::<f32>() {
                Ok(n) if (0.0..=100.0).contains(&n) => {
                    self.cfg.bot_skill = n / 100.0;
                    for p in &mut self.players {
                        if let Some(b) = p.bot.as_mut() {
                            b.skill = n / 100.0;
                        }
                    }
                    out!(&format!("Niveau des bots : {}", n));
                }
                _ => out!("Usage : difficulty <0-100>"),
            },
            "money" => {
                let mut it = rest.split_whitespace();
                let (who, amount) = (it.next().unwrap_or(""), it.next().and_then(|a| a.parse::<u32>().ok()));
                let Some(amount) = amount else {
                    out!("Usage : money <id|nom|all> <montant>");
                    return;
                };
                if who == "all" {
                    for p in &mut self.players {
                        p.money = (p.money + amount).min(MONEY_MAX);
                    }
                    out!("Argent distribué à tous les joueurs");
                } else if let Some(i) = self.find_player(who) {
                    let p = &mut self.players[i];
                    p.money = (p.money + amount).min(MONEY_MAX);
                    (self.log)(&format!("{} a maintenant {} $", p.name, p.money));
                } else {
                    out!("Joueur introuvable");
                }
            }
            "set" => {
                let (key, val) = rest.split_once(' ').map_or((rest, ""), |(a, b)| (a, b.trim()));
                let f = val.parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0);
                let ok = match (key, f) {
                    ("round_time", Some(v)) => {
                        self.cfg.round_time = v.max(10.0);
                        true
                    }
                    ("freeze_time", Some(v)) => {
                        self.cfg.freeze_time = v;
                        true
                    }
                    ("buy_time", Some(v)) => {
                        self.cfg.buy_time = v;
                        true
                    }
                    ("bomb_time", Some(v)) => {
                        self.cfg.bomb_time = v.max(5.0);
                        true
                    }
                    ("max_rounds", Some(v)) => {
                        self.cfg.max_rounds = (v as u16).max(2);
                        true
                    }
                    ("start_money", Some(v)) => {
                        self.cfg.start_money = (v as u32).min(MONEY_MAX);
                        true
                    }
                    ("max_players", Some(v)) => {
                        self.cfg.max_players = (v as u8).max(1);
                        true
                    }
                    ("name", _) if !val.is_empty() => {
                        self.cfg.name = val.to_string();
                        true
                    }
                    _ => false,
                };
                if ok {
                    (self.log)(&format!("{} = {} (les durées s'appliquent au prochain round)", key, val));
                } else {
                    (self.log)("Usage : set <round_time|freeze_time|buy_time|bomb_time|max_rounds|start_money|max_players|name> <valeur>");
                }
            }
            "bomb" => {
                // Diagnostic : « bomb <joueur> » donne la bombe à un attaquant vivant.
                match rest.split_whitespace().next().and_then(|w| self.find_player(w)) {
                    Some(i) if self.players[i].alive && self.players[i].team == Team::T => {
                        self.bomb = BombState::Carried(self.players[i].id);
                    }
                    _ => out!("Usage : bomb <id|nom> (attaquant vivant)"),
                }
            }
            "drop" => {
                // Diagnostic : « drop <joueur> <n° d'arme> » pose une arme devant le joueur.
                let mut it = rest.split_whitespace();
                let who = it.next().and_then(|w| self.find_player(w));
                let wid = it.next().and_then(|a| a.parse::<u8>().ok()).filter(|w| (1..WEAPONS.len() as u8).contains(w));
                match (who, wid) {
                    (Some(i), Some(wid)) => {
                        let p = &self.players[i];
                        let pos = p.pos + cesi_common::dir_from_angles(p.yaw, 0.0) * 1.1;
                        self.drop_weapon(wid, pos, None);
                    }
                    _ => out!("Usage : drop <id|nom> <1-12>"),
                }
            }
            "debug" => {
                out!(&format!("bombe {:?} — site visé {} — reste {:.0}s", self.bomb, self.t_site, self.timer));
                for p in &self.players {
                    out!(&format!(
                        "  {:<14} {:?} pv {:>3} arme {:<10} pos ({:5.1} {:4.1} {:5.1}) site {:?}",
                        p.name,
                        p.team,
                        p.health,
                        weapon(p.weapon).name,
                        p.pos.x,
                        p.pos.y,
                        p.pos.z,
                        self.info.site_of(p.pos)
                    ));
                }
            }
            "config" => {
                if let Ok(s) = serde_json::to_string_pretty(&self.cfg) {
                    for l in s.lines() {
                        out!(l);
                    }
                }
            }
            "save" => match &self.cfg_path {
                Some(path) => match serde_json::to_string_pretty(&self.cfg).map(|s| std::fs::write(path, s)) {
                    Ok(Ok(())) => out!(&format!("Configuration enregistrée dans {}", path.display())),
                    _ => out!("Échec de l'enregistrement"),
                },
                None => out!("Aucun fichier de configuration"),
            },
            "restart" => {
                if self.conns.is_empty() && !self.cfg.run_empty {
                    out!("Aucun joueur connecté");
                } else {
                    self.chat_all(None, "[Serveur] Le match recommence".into());
                    self.start_match();
                }
            }
            "quit" | "exit" | "stop" => {
                out!("Arrêt du serveur…");
                self.quit = true;
            }
            _ => out!("Commande inconnue — tapez « help »"),
        }
    }
}
