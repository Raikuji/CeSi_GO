//! Exécutable d'administration : lance un serveur dédié CeSi GO et le pilote
//! depuis une console de commandes.

use cesi_server::{Server, ServerConfig};
use std::io::BufRead;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};

/// Adresse IP locale utilisée pour sortir sur le réseau (aucun paquet n'est envoyé).
fn lan_ip() -> Option<String> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    Some(s.local_addr().ok()?.ip().to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg_path = PathBuf::from("server.json");
    let mut port: Option<u16> = None;
    let mut run_empty = false;
    let mut fast = false;
    let mut map_name: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" if i + 1 < args.len() => {
                cfg_path = PathBuf::from(&args[i + 1]);
                i += 1;
            }
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().ok();
                i += 1;
            }
            "--map" if i + 1 < args.len() => {
                map_name = Some(args[i + 1].clone());
                i += 1;
            }
            "--run-empty" => run_empty = true,
            "--fast" => fast = true,
            "--help" | "-h" => {
                println!("Usage : cesi_go_server [--config server.json] [--port 27500] [--map <nom|numéro>] [--run-empty] [--fast]");
                return;
            }
            other => eprintln!("Argument ignoré : {}", other),
        }
        i += 1;
    }

    let mut cfg = match std::fs::read_to_string(&cfg_path) {
        Ok(text) => match serde_json::from_str::<ServerConfig>(&text) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{} est invalide ({}), configuration par défaut utilisée", cfg_path.display(), e);
                ServerConfig::default()
            }
        },
        Err(_) => {
            let c = ServerConfig::default();
            if let Ok(s) = serde_json::to_string_pretty(&c) {
                if std::fs::write(&cfg_path, s).is_ok() {
                    println!("Configuration par défaut créée : {}", cfg_path.display());
                }
            }
            c
        }
    };
    if let Some(p) = port {
        cfg.port = p;
    }
    if let Some(m) = map_name {
        cfg.map = m;
    }
    cfg.run_empty |= run_empty || fast;
    cfg.fast = fast;

    println!("==============================================");
    println!("  CeSi GO — serveur dédié");
    println!("==============================================");
    let mut server = match Server::bind(cfg.clone(), Some(cfg_path), Box::new(|s| println!("{}", s))) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    println!("Nom      : {}", cfg.name);
    println!("Carte    : {}  (commande « map » pour en changer)", server.map_name());
    println!("Port     : {} (UDP)", server.port());
    if let Some(ip) = lan_ip() {
        println!("Adresse  : {}:{}  (à saisir dans « Rejoindre un serveur »)", ip, server.port());
    }
    println!("Équipes  : {} contre {}, bots {}", cfg.team_size, cfg.team_size, if cfg.fill_bots { "activés" } else { "désactivés" });
    println!("Tapez « help » pour la liste des commandes.");
    println!();

    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
        // Entrée fermée : le serveur continue de tourner sans console.
        std::mem::forget(tx);
    });
    server.run(rx, Arc::new(AtomicBool::new(false)));
    println!("Serveur arrêté.");
}
