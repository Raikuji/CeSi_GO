//! CeSi GO — client du jeu.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod game;
mod hud;
mod keys;
mod menu;
mod netclient;
mod render;
mod settings;
mod ui;
mod viewmodel;

use audio::Audio;
use cesi_common::map::{self, MAPS};
use cesi_common::voxel::{World, VS};
use cesi_common::protocol::ClientMsg;
use cesi_server::{Server, ServerConfig};
use game::{Game, Overlay, Status};
use macroquad::prelude::*;
use menu::{MenuAction, MenuState, PauseAction, Screen};
use render::{Batch, WorldRender};
use settings::Settings;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use ui::Ui;

fn window_conf() -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: "CeSi GO".to_owned(),
            window_width: 1280,
            window_height: 720,
            high_dpi: true,
            sample_count: 4,
            ..Default::default()
        },
        draw_call_vertex_capacity: 20000,
        draw_call_index_capacity: 30000,
        ..Default::default()
    }
}

/// Serveur lancé dans un fil d'exécution pour les parties locales.
struct LocalServer {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    _cmds: mpsc::Sender<String>,
}

impl Drop for LocalServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn start_local(s: &Settings) -> Result<(LocalServer, String), String> {
    let cfg = ServerConfig {
        name: "Partie locale".into(),
        port: 0,
        local_only: true,
        max_players: 1,
        team_size: s.bot_team_size.round().clamp(1.0, 5.0) as u8,
        bot_skill: (s.bot_skill / 100.0).clamp(0.0, 1.0),
        max_rounds: 16,
        map: MAPS[s.map as usize % MAPS.len()].name.into(),
        ..Default::default()
    };
    let mut server = Server::bind(cfg, None, Box::new(|_| {}))?;
    let addr = format!("127.0.0.1:{}", server.port());
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<String>();
    let flag = stop.clone();
    let thread = std::thread::spawn(move || server.run(rx, flag));
    Ok((LocalServer { stop, thread: Some(thread), _cmds: tx }, addr))
}

/// Dessine la carte seule depuis un point de vue donné.
fn view(wr: &WorldRender, eye: Vec3, target: Vec3) {
    clear_background(render::SKY);
    let fovy = 0.9;
    let dir = (target - eye).normalize();
    let up = if dir.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
    set_camera(&Camera3D { position: eye, target, up, fovy, z_near: 0.1, z_far: 400.0, ..Default::default() });
    wr.begin(eye);
    wr.draw(eye, dir, fovy);
    set_default_camera();
    gl_use_default_material();
}

/// Vue aérienne tournante de la carte derrière les menus.
fn backdrop(wr: &WorldRender, world: &World, t: f32) {
    let centre = vec3(world.sx as f32 * VS / 2.0, 3.0, world.sz as f32 * VS / 2.0 + 2.0);
    view(wr, centre + vec3((t * 0.06).cos() * 40.0, 24.0, (t * 0.06).sin() * 40.0), centre);
}

#[macroquad::main(window_conf)]
async fn main() {
    // Option de diagnostic : --shot <écran> <fichier.png> capture un écran puis quitte.
    let args: Vec<String> = std::env::args().collect();
    let shot = args
        .iter()
        .position(|a| a == "--shot")
        .filter(|i| i + 2 < args.len())
        .map(|i| (args[i + 1].clone(), args[i + 2].clone()));

    let mut settings = Settings::load();
    // Diagnostic : --map <n> force la carte, --cam x,y,z,cx,cy,cz fixe la vue de « --shot vue ».
    if let Some(m) = args.iter().position(|a| a == "--map").and_then(|i| args.get(i + 1)).and_then(|a| map::find(a)) {
        settings.map = m;
    }
    let cam: Option<Vec<f32>> = args
        .iter()
        .position(|a| a == "--cam")
        .and_then(|i| args.get(i + 1))
        .map(|a| a.split(',').filter_map(|v| v.parse().ok()).collect())
        .filter(|v: &Vec<f32>| v.len() == 6);
    // Les captures ne doivent pas modifier les réglages du joueur.
    let persist = shot.is_none();
    let mut ui = Ui::new();
    clear_background(Color::new(0.06, 0.07, 0.09, 1.0));
    ui.begin();
    ui.text_c("Chargement…", 640.0, 370.0, 30.0, ui::TEXT);
    next_frame().await;

    let (world, info) = map::build(settings.map);
    let (mut world, mut info) = (Rc::new(world), Rc::new(info));
    let mut wr = WorldRender::new(&world);
    let mut batch = Batch::new();
    let mut audio = Audio::new().await;
    if settings.fullscreen {
        set_fullscreen(true);
    }

    let mut menu = MenuState::new();
    let mut game: Option<Game> = None;
    let mut server: Option<LocalServer> = None;
    let mut grabbed = false;
    let mut shot_at: Option<f64> = shot.as_ref().map(|_| get_time() + 1.0);
    // --connect <adresse> : rejoint directement un serveur au lancement.
    let mut autojoin = args.iter().position(|a| a == "--connect").and_then(|i| args.get(i + 1).cloned());

    loop {
        let dt = get_frame_time().min(0.1);
        audio.volume = settings.volume;
        ui.begin();
        let esc = is_key_pressed(KeyCode::Escape);
        let mut start: Option<bool> = None; // Some(true) = partie locale
        let mut leave: Option<Option<String>> = None;

        if let Some(g) = game.as_mut() {
            if esc {
                if menu.rebinding.is_some() {
                    menu.rebinding = None;
                } else {
                    g.overlay = match g.overlay {
                        Overlay::None => Overlay::Pause,
                        Overlay::Settings => {
                            if persist {
                                settings.save();
                            }
                            Overlay::Pause
                        }
                        _ => Overlay::None,
                    };
                    ui.focus = None;
                }
            }
            let want = g.want_grab() && shot.is_none();
            if want != grabbed {
                set_cursor_grab(want);
                show_mouse(!want);
                grabbed = want;
                g.grab_frames = 0;
            }
            if grabbed {
                g.grab_frames += 1;
            }
            g.update(dt, &settings, &audio);
            if g.map_dirty {
                // Le serveur joue une autre carte que celle en mémoire.
                g.map_dirty = false;
                world = g.world.clone();
                info = g.info.clone();
                wr.set_world(&world);
            }
            g.draw_world(&wr, &mut batch);
            g.draw_hud(&settings, &ui);
            match g.overlay {
                Overlay::None => {}
                Overlay::Pause => match menu::pause_menu(&mut ui, true) {
                    PauseAction::Resume => g.overlay = Overlay::None,
                    PauseAction::Settings => g.overlay = Overlay::Settings,
                    PauseAction::SwitchTeam => {
                        g.net.reliable(ClientMsg::SwitchTeam);
                        g.overlay = Overlay::None;
                    }
                    PauseAction::Leave => leave = Some(None),
                    PauseAction::None => {}
                },
                Overlay::Settings => {
                    ui.dim(0.55);
                    if menu::settings_panel(&mut ui, &mut menu, &mut settings) {
                        g.overlay = Overlay::Pause;
                    }
                }
                Overlay::Buy => g.draw_buy(&mut ui, &settings, &audio),
                Overlay::Chat => g.draw_chat_input(&mut ui),
            }
            if let Status::Failed(e) = &g.status {
                leave = Some(Some(e.clone()));
            }
        } else {
            if settings.map != info.id {
                // Carte choisie dans le menu (ou retour d'un serveur qui en jouait une autre).
                let (w, i) = map::build(settings.map);
                world = Rc::new(w);
                info = Rc::new(i);
                wr.set_world(&world);
            }
            if let (Some(c), Some(("vue", _))) = (&cam, shot.as_ref().map(|(m, f)| (m.as_str(), f))) {
                view(&wr, vec3(c[0], c[1], c[2]), vec3(c[3], c[4], c[5]));
            } else {
                backdrop(&wr, &world, get_time() as f32);
                ui.dim(0.6);
                if esc {
                    if menu.rebinding.is_some() {
                        menu.rebinding = None;
                    } else if menu.screen != Screen::Main {
                        menu.screen = Screen::Main;
                        settings.save();
                    }
                }
                match menu::main_menu(&mut ui, &mut menu, &mut settings) {
                    MenuAction::PlayLocal => start = Some(true),
                    MenuAction::Join => start = Some(false),
                    MenuAction::Quit => break,
                    MenuAction::None => {}
                }
            }
        }

        if let Some(addr) = autojoin.take() {
            settings.last_server = addr;
            start = Some(false);
            if shot.is_some() {
                shot_at = Some(get_time() + 14.0);
            }
        }
        // Écrans imposés par l'option de capture.
        if let (Some((mode, _)), true) = (&shot, get_time() < 0.5 + shot_at.unwrap_or(0.0) - 1.0) {
            match mode.as_str() {
                "play" => menu.screen = Screen::Play,
                "options" | "touches" | "viseur" => {
                    menu.screen = Screen::Settings;
                    menu.tab = ["options", "touches", "viseur"].iter().position(|m| m == mode).unwrap_or(0);
                }
                "jeu" | "achats" | "scores" | "tir" | "couteau" | "coup" | "coup2" | "coup3" | "poses" | "vm" | "nade" | "main" | "sol" | "echange" | "jeter"
                    if game.is_none() && start.is_none() =>
                {
                    start = Some(true);
                    shot_at = Some(get_time() + if mode == "nade" { 11.5 } else { 9.0 });
                }
                _ => {}
            }
        }
        if let (Some((mode, _)), Some(g)) = (&shot, game.as_mut()) {
            // « vm » : --shot vm <fichier> --vm <arme> <progression du rechargement>
            if let Some(i) = args.iter().position(|a| a == "--vm") {
                g.debug_weapon = args.get(i + 1).and_then(|a| a.parse().ok()).unwrap_or(0);
                g.debug_f = args.get(i + 2).and_then(|a| a.parse().ok()).unwrap_or(0.0);
            }
            // « nade » : --shot nade <fichier> --nade <type> achète et lance la grenade.
            if let Some(i) = args.iter().position(|a| a == "--nade") {
                g.debug_nade = args.get(i + 1).and_then(|a| a.parse().ok());
            }
            // « sol » regarde par terre ; « echange » ramasse en plus les armes visées (deux fois).
            // « jeter » jette l'arme en main puis regarde où elle est tombée.
            if (mode == "sol" || mode == "echange" || mode == "jeter") && !g.debug_look {
                g.debug_look = true;
                g.debug_pick = if mode == "echange" { 2 } else { 0 };
                g.debug_drop = mode == "jeter";
            }
            if mode == "achats" && g.can_buy() {
                g.overlay = Overlay::Buy;
            }
            g.debug = match mode.as_str() {
                "tir" => 1,
                "poses" => 64,
                "main" => 128,
                "couteau" => 4,
                "coup" => 4 | 16,
                "coup2" => 4 | 16 | 32,
                "coup3" => 4 | 8,
                "scores" => 2,
                _ => 0,
            };
        }

        if let Some(local) = start {
            if persist {
                settings.save();
            }
            let result = if local {
                start_local(&settings).map(|(srv, addr)| {
                    server = Some(srv);
                    addr
                })
            } else {
                Ok(settings.last_server.clone())
            };
            match result.and_then(|addr| Game::connect(&addr, &settings.name, world.clone(), info.clone())) {
                Ok(g) => {
                    game = Some(g);
                    menu.error = None;
                }
                Err(e) => {
                    server = None;
                    menu.error = Some(e);
                }
            }
        }
        if let Some(err) = leave {
            if let Some(mut g) = game.take() {
                g.leave();
            }
            server = None;
            menu.error = err;
            menu.screen = if menu.error.is_some() { Screen::Play } else { Screen::Main };
            set_cursor_grab(false);
            show_mouse(true);
            grabbed = false;
        }

        if let (Some((_, path)), Some(t)) = (&shot, shot_at) {
            if get_time() >= t {
                get_screen_data().export_png(path);
                break;
            }
        }
        next_frame().await;
    }
    if let Some(mut g) = game.take() {
        g.leave();
    }
    drop(server);
    if persist {
        settings.save();
    }
}
