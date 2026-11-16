//! Connexion UDP du client au serveur.

use cesi_common::net::Channel;
use cesi_common::protocol::{ClientMsg, ClientState, ServerMsg, Snapshot, DEFAULT_PORT, PROTOCOL};
use std::net::{ToSocketAddrs, UdpSocket};

pub struct NetClient {
    sock: UdpSocket,
    chan: Channel<ClientMsg>,
}

impl NetClient {
    /// `addr` : « ip », « ip:port » ou nom d'hôte.
    pub fn connect(addr: &str, name: &str) -> Result<NetClient, String> {
        let addr = addr.trim();
        let full = if addr.contains(':') { addr.to_string() } else { format!("{}:{}", addr, DEFAULT_PORT) };
        let target = full
            .to_socket_addrs()
            .map_err(|_| format!("Adresse invalide : {}", addr))?
            .find(|a| a.is_ipv4())
            .ok_or_else(|| format!("Adresse introuvable : {}", addr))?;
        let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        sock.connect(target).map_err(|e| e.to_string())?;
        sock.set_nonblocking(true).map_err(|e| e.to_string())?;
        let mut chan = Channel::new();
        chan.send_reliable(ClientMsg::Join { version: PROTOCOL, name: name.to_string() });
        Ok(NetClient { sock, chan })
    }

    pub fn reliable(&mut self, m: ClientMsg) {
        self.chan.send_reliable(m);
    }

    pub fn send(&mut self, state: Option<ClientState>) {
        let _ = self.sock.send(&self.chan.build(state));
    }

    /// Lit tous les paquets en attente.
    pub fn poll(&mut self) -> (Vec<ServerMsg>, Vec<Snapshot>) {
        let (mut msgs, mut snaps) = (Vec::new(), Vec::new());
        let mut buf = [0u8; 8192];
        for _ in 0..500 {
            match self.sock.recv(&mut buf) {
                Ok(n) => {
                    if let Some((m, s)) = self.chan.receive::<ServerMsg, Snapshot>(&buf[..n]) {
                        msgs.extend(m);
                        snaps.extend(s);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => continue,
            }
        }
        (msgs, snaps)
    }

    /// Prévient le serveur du départ (envoyé plusieurs fois, sans attendre d'accusé).
    pub fn leave(&mut self) {
        self.chan.send_reliable(ClientMsg::Leave);
        for _ in 0..3 {
            self.send(None);
        }
    }
}
