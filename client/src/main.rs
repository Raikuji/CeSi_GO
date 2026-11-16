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

use audio::Audio;
use cesi_common::map;
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

/// Vue aérienne tournante de la carte derrière les menus.
fn backdrop(wr: &WorldRender, t: f32) {
    clear_background(render::SKY);
    let centre = vec3(32.0, 3.0, 34.0);
    let eye = centre + vec3((t * 0.06).cos() * 40.0, 24.0, (t * 0.06).sin() * 40.0);
    let fovy = 0.9;
    set_camera(&Camera3D { position: eye, target: centre, up: Vec3::Y, fovy, z_near: 0.1, z_far: 400.0, ..Default::default() });
    wr.begin(eye);
    wr.draw(eye, (centre - eye).normalize(), fovy);
    set_default_camera();
    gl_use_default_material();
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
    let mut ui = Ui::new();
    clear_background(Color::new(0.06, 0.07, 0.09, 1.0));
    ui.begin();
    ui.text_c("Chargement…", 640.0, 370.0, 30.0, ui::TEXT);
    next_frame().await;

    let (world, info) = map::build();
    let (world, info) = (Rc::new(world), Rc::new(info));
    let wr = WorldRender::new(&world);
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
                            settings.save();
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
            backdrop(&wr, get_time() as f32);
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
                "jeu" | "achats" | "scores" | "tir" | "couteau" | "coup" | "coup2" | "coup3" if game.is_none() && start.is_none() => {
                    start = Some(true);
                    shot_at = Some(get_time() + 9.0);
                }
                _ => {}
            }
        }
        if let (Some((mode, _)), Some(g)) = (&shot, game.as_mut()) {
            if mode == "achats" && g.can_buy() {
                g.overlay = Overlay::Buy;
            }
            g.debug = match mode.as_str() {
                "tir" => 1,
                "couteau" => 4,
                "coup" => 4 | 16,
                "coup2" => 4 | 16 | 32,
                "coup3" => 4 | 8,
                "scores" => 2,
                _ => 0,
            };
        }

        if let Some(local) = start {
            settings.save();
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
    settings.save();
}
