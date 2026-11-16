//! Serveur de jeu CeSi GO. Utilisé par l'exécutable `cesi_go_server` et par le
//! client pour les parties locales contre des bots.

mod bot;
mod game;
mod nav;

pub use game::{Server, ServerConfig};
