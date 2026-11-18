//! Affichage tête haute : viseur, vie, munitions, scores, messages,
//! tableau des scores, menu d'achat et discussion.

use crate::audio::Audio;
use crate::game::{Game, Overlay, Status};
use crate::settings::{Action, Crosshair, Settings, NADE_ACTIONS};
use crate::ui::{Ui, ACCENT, CT_COLOR, DIM, PANEL, TEXT, T_COLOR, WIDGET, WIDGET_HOT};
use crate::radar;
use crate::render::ss;
use cesi_common::grenades::*;
use cesi_common::protocol::*;
use cesi_common::weapons::*;
use macroquad::prelude::*;

fn team_color(t: Team) -> Color {
    if t == Team::T {
        T_COLOR
    } else {
        CT_COLOR
    }
}

/// Dessine le viseur au point donné ; `extra` écarte les branches (pixels).
pub fn draw_crosshair(cx: f32, cy: f32, c: &Crosshair, extra: f32) {
    let k = (screen_height() / 1080.0).max(0.5);
    let th = (c.thickness * k).round().max(1.0);
    let len = (c.size * k).round();
    let gap = (c.gap * k).round() + extra.round();
    let col = Color::from_rgba(c.color[0], c.color[1], c.color[2], (c.alpha.clamp(0.0, 1.0) * 255.0) as u8);
    let (cx, cy) = (cx.round(), cy.round());
    let h = (th / 2.0).floor();
    let mut rects: Vec<(f32, f32, f32, f32)> = Vec::new();
    if len > 0.0 {
        rects.push((cx + gap, cy - h, len, th));
        rects.push((cx - gap - len, cy - h, len, th));
        rects.push((cx - h, cy + gap, th, len));
        if !c.t_style {
            rects.push((cx - h, cy - gap - len, th, len));
        }
    }
    if c.dot {
        rects.push((cx - h, cy - h, th, th));
    }
    if c.outline {
        for (x, y, w, hh) in &rects {
            draw_rectangle(x - 1.0, y - 1.0, w + 2.0, hh + 2.0, Color::new(0.0, 0.0, 0.0, col.a));
        }
    }
    for (x, y, w, hh) in rects {
        draw_rectangle(x, y, w, hh, col);
    }
}

fn clock(t: f32) -> String {
    let s = t.max(0.0).ceil() as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

impl Game {
    pub fn draw_hud(&self, st: &Settings, ui: &Ui) {
        let (w, h) = (screen_width(), screen_height());
        let k = h / 720.0;
        if !matches!(self.status, Status::Connected) || self.snap().is_none() {
            ui.dim(0.5);
            ui.shadow_px_c("Connexion au serveur…", w / 2.0, h / 2.0, 30.0 * k, TEXT);
            return;
        }
        let snap = self.snap().unwrap();
        let Some(me) = self.me() else { return };
        let def = weapon(self.weapon_id());

        // --- Lunette ou viseur ---
        if me.alive && self.zoom > 0 {
            let (cx, cy, r) = (w / 2.0, h / 2.0, h * 0.47);
            draw_rectangle(0.0, 0.0, cx - r, h, BLACK);
            draw_rectangle(cx + r, 0.0, w - cx - r, h, BLACK);
            let big = r * 1.55;
            for i in 0..72 {
                let (a0, a1) = (i as f32 / 72.0 * std::f32::consts::TAU, (i + 1) as f32 / 72.0 * std::f32::consts::TAU);
                let (p0, p1) = (vec2(a0.cos(), a0.sin()), vec2(a1.cos(), a1.sin()));
                let c = vec2(cx, cy);
                draw_triangle(c + p0 * r, c + p1 * r, c + p1 * big, BLACK);
                draw_triangle(c + p0 * r, c + p1 * big, c + p0 * big, BLACK);
            }
            draw_rectangle(cx - r, cy - k, r * 2.0, 2.0 * k, BLACK);
            draw_rectangle(cx - k, cy - r, 2.0 * k, r * 2.0, BLACK);
        } else if me.alive && !def.scope && self.overlay != Overlay::Buy {
            let extra = if st.crosshair.dynamic {
                self.spread(def).to_radians().tan() / (self.cam_fovy / 2.0).tan() * h / 2.0
            } else {
                0.0
            };
            draw_crosshair(w / 2.0, h / 2.0, &st.crosshair, extra);
        }

        // --- Touche confirmée ---
        if self.hitmarker_t > 0.0 {
            let col = if self.hit_kill { Color::new(1.0, 0.25, 0.2, 1.0) } else { WHITE };
            let (cx, cy) = (w / 2.0, h / 2.0);
            for (sx, sy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                draw_line(cx + sx * 7.0 * k, cy + sy * 7.0 * k, cx + sx * 15.0 * k, cy + sy * 15.0 * k, 2.0 * k, col);
            }
        }

        // --- Fumée : écran gris quand la caméra est dans le nuage ---
        for a in snap.areas.iter().filter(|a| a.kind == SMOKE) {
            let d = self.cam_pos.distance(a.pos + Vec3::Y * 1.2);
            let dense = ss(0.0, 1.4, SMOKE_TIME - a.time_left) * ss(0.0, 2.5, a.time_left);
            let alpha = (1.0 - ss(1.9, 2.9, d)) * dense * 0.96;
            if alpha > 0.01 {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.66, 0.66, 0.68, alpha));
            }
        }
        // --- Minimap (sous l'aveuglement : une grenade aveuglante la masque aussi) ---
        let (rx, ry, rs) = radar::rect(w, h);
        self.draw_radar(ui, rx, ry, rs);

        // --- Aveuglement : écran blanc qui se dissipe ---
        if self.blind_t > 0.0 {
            draw_rectangle(0.0, 0.0, w, h, Color::new(1.0, 1.0, 1.0, (self.blind_t / 1.3).min(1.0)));
        }

        // --- Dégâts reçus et explosion ---
        if self.hurt_t > 0.0 {
            let a = (self.hurt_t / 0.6).min(1.0) * 0.45;
            let t = 70.0 * k;
            let red = Color::new(0.8, 0.0, 0.0, a);
            draw_rectangle(0.0, 0.0, w, t, red);
            draw_rectangle(0.0, h - t, w, t, red);
            draw_rectangle(0.0, t, t, h - 2.0 * t, red);
            draw_rectangle(w - t, t, t, h - 2.0 * t, red);
        }
        if self.flash_t > 0.0 {
            draw_rectangle(0.0, 0.0, w, h, Color::new(1.0, 0.95, 0.85, self.flash_t.min(1.0)));
        }

        // --- Bandeau du haut : scores et chrono ---
        let planted = matches!(snap.bomb, BombState::Planted { .. });
        draw_rectangle(w / 2.0 - 150.0 * k, 0.0, 300.0 * k, 52.0 * k, Color::new(0.0, 0.0, 0.0, 0.45));
        let (timer, tcol) = match snap.phase {
            Phase::Live if planted => ("BOMBE".to_string(), if (self.now * 3.0) as i64 % 2 == 0 { Color::new(1.0, 0.3, 0.2, 1.0) } else { TEXT }),
            Phase::Freeze => (clock(snap.time_left), Color::new(1.0, 0.85, 0.3, 1.0)),
            Phase::Live => (clock(snap.time_left), if snap.time_left < 10.0 { Color::new(1.0, 0.3, 0.2, 1.0) } else { TEXT }),
            _ => ("—".to_string(), DIM),
        };
        ui.shadow_px_c(&timer, w / 2.0, 34.0 * k, 28.0 * k, tcol);
        ui.shadow_px_c(&snap.score_t.to_string(), w / 2.0 - 100.0 * k, 32.0 * k, 30.0 * k, T_COLOR);
        ui.shadow_px_c(&snap.score_ct.to_string(), w / 2.0 + 100.0 * k, 32.0 * k, 30.0 * k, CT_COLOR);
        let alive = |t: Team| snap.players.iter().filter(|p| p.team == t && p.alive).count();
        ui.shadow_px_c(&format!("{} en vie", alive(Team::T)), w / 2.0 - 100.0 * k, 47.0 * k, 12.0 * k, T_COLOR);
        ui.shadow_px_c(&format!("{} en vie", alive(Team::CT)), w / 2.0 + 100.0 * k, 47.0 * k, 12.0 * k, CT_COLOR);

        // --- Éliminations, sous la minimap ---
        let mut y = ry + rs + 30.0 * k;
        for l in self.killfeed.iter().filter(|l| self.now - l.time < 7.0) {
            let mid = format!(" [{}{}] ", item_name(l.weapon), if l.headshot { " · tête" } else { "" });
            let size = 16.0 * k;
            let (wk, wm, wv) = (ui.width_px(&l.killer, size), ui.width_px(&mid, size), ui.width_px(&l.victim, size));
            let x = w - 16.0 * k - (wk + wm + wv);
            draw_rectangle(x - 8.0 * k, y - 17.0 * k, wk + wm + wv + 16.0 * k, 24.0 * k, Color::new(0.0, 0.0, 0.0, if l.mine { 0.7 } else { 0.4 }));
            if l.mine {
                draw_rectangle_lines(x - 8.0 * k, y - 17.0 * k, wk + wm + wv + 16.0 * k, 24.0 * k, 2.0, Color::new(0.9, 0.2, 0.2, 0.9));
            }
            ui.text_px(&l.killer, x, y, size, l.kteam.map_or(TEXT, team_color));
            ui.text_px(&mid, x + wk, y, size, TEXT);
            ui.text_px(&l.victim, x + wk + wm, y, size, team_color(l.vteam));
            y += 28.0 * k;
        }

        // --- Discussion ---
        let mut y = h - 150.0 * k;
        let chatting = self.overlay == Overlay::Chat;
        for (from, team, text, t) in self.chat.iter().rev() {
            if !chatting && self.now - t > 9.0 {
                continue;
            }
            if from.is_empty() {
                ui.shadow_px(text, 22.0 * k, y, 16.0 * k, Color::new(1.0, 0.9, 0.5, 1.0));
            } else {
                let name = format!("{} : ", from);
                ui.shadow_px(&name, 22.0 * k, y, 16.0 * k, team.map_or(TEXT, team_color));
                ui.shadow_px(text, 22.0 * k + ui.width_px(&name, 16.0 * k), y, 16.0 * k, TEXT);
            }
            y -= 22.0 * k;
        }

        // --- Message central ---
        if let Some((text, until, color)) = &self.center {
            if self.now < *until {
                let wt = ui.width_px(text, 30.0 * k);
                draw_rectangle(w / 2.0 - wt / 2.0 - 20.0 * k, h * 0.2 - 34.0 * k, wt + 40.0 * k, 50.0 * k, Color::new(0.0, 0.0, 0.0, 0.5));
                ui.shadow_px_c(text, w / 2.0, h * 0.2, 30.0 * k, *color);
            }
        }

        if me.alive {
            // --- Vie, gilet, argent ---
            let hp_col = if me.health <= 25 { Color::new(1.0, 0.3, 0.25, 1.0) } else { TEXT };
            ui.shadow_px("VIE", 24.0 * k, h - 62.0 * k, 13.0 * k, DIM);
            ui.shadow_px(&me.health.to_string(), 24.0 * k, h - 22.0 * k, 42.0 * k, hp_col);
            ui.shadow_px(if me.helmet { "GILET + CASQUE" } else { "GILET" }, 130.0 * k, h - 62.0 * k, 13.0 * k, DIM);
            ui.shadow_px(&me.armor.to_string(), 130.0 * k, h - 22.0 * k, 42.0 * k, if me.armor > 0 { TEXT } else { DIM });
            ui.shadow_px(&format!("{} $", me.money), 24.0 * k, h - 92.0 * k, 24.0 * k, Color::new(0.55, 0.9, 0.45, 1.0));
            let mut x = 150.0 * k;
            if self.has_bomb() {
                let t = format!("BOMBE [{}]", st.bind(Action::DropBomb).label());
                ui.shadow_px(&t, x, h - 92.0 * k, 18.0 * k, Color::new(1.0, 0.5, 0.25, 1.0));
                x += ui.width_px(&t, 18.0 * k) + 20.0 * k;
            }
            if me.kit {
                ui.shadow_px("KIT", x, h - 92.0 * k, 18.0 * k, CT_COLOR);
            }

            // --- Arme et munitions ---
            let right = w - 24.0 * k;
            if self.slot < 2 {
                let a = self.ammo[self.slot];
                let res = format!(" / {}", a.reserve);
                let wr = ui.width_px(&res, 22.0 * k);
                ui.shadow_px(&res, right - wr, h - 22.0 * k, 22.0 * k, DIM);
                let col = if a.mag as f32 <= def.mag as f32 * 0.25 { Color::new(1.0, 0.3, 0.25, 1.0) } else { TEXT };
                ui.shadow_px_r(&a.mag.to_string(), right - wr, h - 22.0 * k, 42.0 * k, col);
            }
            if self.reload_t > 0.0 {
                ui.shadow_px_r("Rechargement…", right, h - 66.0 * k, 16.0 * k, ACCENT);
            }
            if self.slot == 3 {
                ui.shadow_px_r(GRENADES[self.nade].name, right, h - 24.0 * k, 26.0 * k, TEXT);
            }
            let slots = [(0usize, st.bind(Action::Primary)), (1, st.bind(Action::Secondary)), (2, st.bind(Action::Knife))];
            let mut y = h - 96.0 * k;
            // Grenades possédées, chacune avec sa touche ; celle en main est mise en avant.
            for i in (0..4).rev().filter(|i| me.nades[*i] > 0) {
                let n = if me.nades[i] > 1 { format!("{} ×{}", GRENADES[i].name, me.nades[i]) } else { GRENADES[i].name.to_string() };
                let text = format!("{}   [{}]", n, st.bind(NADE_ACTIONS[i]).label());
                ui.shadow_px_r(&text, right, y, 16.0 * k, if self.slot == 3 && self.nade == i { TEXT } else { DIM });
                y -= 22.0 * k;
            }
            for (s, bind) in slots.iter().rev() {
                let id = if *s < 2 { self.ammo[*s].id } else { KNIFE };
                if *s < 2 && id == 0 {
                    continue;
                }
                let text = format!("{}   [{}]", weapon(id).name, bind.label());
                ui.shadow_px_r(&text, right, y, 16.0 * k, if *s == self.slot { TEXT } else { DIM });
                y -= 22.0 * k;
            }

            // --- Indications contextuelles ---
            let use_key = st.bind(Action::Use).label();
            let mut hint: Option<String> = None;
            if me.action == 1 || me.action == 2 {
                let label = if me.action == 1 { "Pose de la bombe…" } else { "Désamorçage…" };
                let (bw, bh) = (280.0 * k, 12.0 * k);
                ui.shadow_px_c(label, w / 2.0, h * 0.62 - 10.0 * k, 18.0 * k, TEXT);
                draw_rectangle(w / 2.0 - bw / 2.0, h * 0.62, bw, bh, Color::new(0.0, 0.0, 0.0, 0.6));
                draw_rectangle(w / 2.0 - bw / 2.0, h * 0.62, bw * me.progress.clamp(0.0, 1.0), bh, ACCENT);
            } else if let Some((_, wid)) = self.aim_drop {
                let mine = self.ammo[if weapon(wid).class == Class::Pistol { 1 } else { 0 }].id;
                hint = Some(if mine != 0 {
                    format!("[{}] Échanger {} contre {}", use_key, weapon(mine).name, weapon(wid).name)
                } else {
                    format!("[{}] Ramasser {}", use_key, weapon(wid).name)
                });
            } else if self.has_bomb() && self.info.site_of(self.body.pos).is_some() && snap.phase == Phase::Live {
                hint = Some(format!("Maintenez [{}] pour poser la bombe", use_key));
            } else if me.team == Team::CT && matches!(snap.bomb, BombState::Planted { pos, .. } if pos.distance(self.body.pos) < 1.8) {
                hint = Some(format!("Maintenez [{}] pour désamorcer", use_key));
            } else if let (Some(left), true, true) = (self.buy_time_left(), self.in_buy_zone(), self.overlay == Overlay::None) {
                hint = Some(format!("[{}] Acheter des armes — encore {} s", st.bind(Action::Buy).label(), left.ceil()));
            } else if self.slot == 3 {
                hint = Some(format!(
                    "[{}] lancer long · [{}] lancer court · [{}] type suivant",
                    st.bind(Action::Fire).label(),
                    st.bind(Action::Aim).label(),
                    st.bind(Action::Grenade).label()
                ));
            }
            if let Some(t) = hint {
                ui.shadow_px_c(&t, w / 2.0, h * 0.66, 18.0 * k, Color::new(1.0, 0.9, 0.5, 1.0));
            }
        } else {
            let who = self.spectate.and_then(|s| self.player(s)).map(|p| p.name.clone());
            let text = match who {
                Some(n) => format!("Vous êtes éliminé — caméra : {} (clic pour changer)", n),
                None => "Vous êtes éliminé".to_string(),
            };
            ui.shadow_px_c(&text, w / 2.0, h * 0.86, 20.0 * k, TEXT);
        }

        // --- Bilan des dégâts infligés, une fois mort ---
        if (!me.alive && matches!(snap.phase, Phase::Live | Phase::RoundEnd)) || self.debug_dmg {
            self.draw_damage(ui, snap.round);
        }

        if st.down(Action::Scoreboard) || snap.phase == Phase::MatchEnd || self.debug & 2 != 0 {
            self.draw_scoreboard(ui);
        }
    }

    /// Dégâts infligés pendant le round, par joueur touché (coin haut gauche).
    fn draw_damage(&self, ui: &Ui, round: u16) {
        let k = screen_height() / 720.0;
        let (x, y0, wd) = (16.0 * k, 16.0 * k, 360.0 * k);
        let rows = self.round_dmg.len().max(1) as f32;
        let hgt = (58.0 + rows * 24.0 + if self.round_dmg.len() > 1 { 28.0 } else { 0.0 }) * k;
        draw_rectangle(x, y0, wd, hgt, Color::new(0.0, 0.0, 0.0, 0.65));
        draw_rectangle(x, y0, 4.0 * k, hgt, ACCENT);
        ui.shadow_px(&format!("DÉGÂTS INFLIGÉS — ROUND {}", round), x + 16.0 * k, y0 + 26.0 * k, 15.0 * k, ACCENT);
        let right = x + wd - 14.0 * k;
        let mut y = y0 + 54.0 * k;
        if self.round_dmg.is_empty() {
            ui.shadow_px("Aucun joueur touché", x + 16.0 * k, y, 16.0 * k, DIM);
            return;
        }
        let mut total = 0;
        for l in &self.round_dmg {
            let (name, team) = self.player(l.victim).map_or(("?".to_string(), Team::T), |p| (p.name.clone(), p.team));
            ui.shadow_px(&name, x + 16.0 * k, y, 16.0 * k, team_color(team));
            let touches = if l.hits > 1 { format!("{} touches", l.hits) } else { "1 touche".to_string() };
            ui.shadow_px_r(&touches, right - 110.0 * k, y, 14.0 * k, DIM);
            if l.killed {
                ui.shadow_px_r("tué", right - 56.0 * k, y, 14.0 * k, Color::new(1.0, 0.35, 0.3, 1.0));
            }
            ui.shadow_px_r(&l.damage.to_string(), right, y, 18.0 * k, TEXT);
            total += l.damage;
            y += 24.0 * k;
        }
        if self.round_dmg.len() > 1 {
            draw_rectangle(x + 16.0 * k, y - 12.0 * k, wd - 30.0 * k, 1.0_f32.max(k), Color::new(1.0, 1.0, 1.0, 0.25));
            ui.shadow_px("Total", x + 16.0 * k, y + 10.0 * k, 16.0 * k, TEXT);
            ui.shadow_px_r(&total.to_string(), right, y + 10.0 * k, 18.0 * k, ACCENT);
        }
    }

    fn draw_scoreboard(&self, ui: &Ui) {
        let Some(snap) = self.snap() else { return };
        let my_team = self.me().map(|m| m.team);
        let panel = Rect::new(240.0, 90.0, 800.0, 540.0);
        ui.rect(panel, PANEL);
        ui.text_c(&format!("{} — {}", self.cfg.server_name, self.info.name), 640.0, 128.0, 22.0, DIM);
        ui.text_c(
            &format!("Round {} / {}", snap.round, self.cfg.max_rounds),
            640.0,
            154.0,
            16.0,
            DIM,
        );
        let mut y = 190.0;
        for team in [Team::CT, Team::T] {
            let col = team_color(team);
            let score = if team == Team::T { snap.score_t } else { snap.score_ct };
            ui.rect(Rect::new(panel.x + 20.0, y - 22.0, panel.w - 40.0, 30.0), Color::new(col.r, col.g, col.b, 0.18));
            ui.text(&format!("{} — {}", team.label(), score), panel.x + 32.0, y, 20.0, col);
            ui.text_r("Élim.", 800.0, y, 14.0, DIM);
            ui.text_r("Morts", 880.0, y, 14.0, DIM);
            ui.text_r("Argent", 1000.0, y, 14.0, DIM);
            y += 30.0;
            let mut list: Vec<&PlayerSnap> = snap.players.iter().filter(|p| p.team == team).collect();
            list.sort_by(|a, b| b.kills.cmp(&a.kills).then(a.deaths.cmp(&b.deaths)));
            for p in list {
                let c = if !p.alive {
                    DIM
                } else if Some(p.id) == self.my_id {
                    ACCENT
                } else {
                    TEXT
                };
                ui.text(&p.name, panel.x + 32.0, y, 18.0, c);
                if !p.alive {
                    ui.text("éliminé", panel.x + 300.0, y, 14.0, DIM);
                }
                ui.text_r(&p.kills.to_string(), 800.0, y, 18.0, c);
                ui.text_r(&p.deaths.to_string(), 880.0, y, 18.0, c);
                if Some(team) == my_team {
                    ui.text_r(&format!("{} $", p.money), 1000.0, y, 18.0, c);
                }
                y += 26.0;
            }
            y += 26.0;
        }
    }

    /// Menu d'achat ; les achats sont validés par le serveur.
    pub fn draw_buy(&mut self, ui: &mut Ui, st: &Settings, audio: &Audio) {
        let Some(me) = self.me().cloned() else { return };
        const W: f32 = 280.0;
        let green = Color::new(0.55, 0.9, 0.45, 1.0);
        ui.dim(0.35);
        ui.rect(Rect::new(30.0, 60.0, 1220.0, 600.0), PANEL);
        ui.text("ACHATS", 56.0, 108.0, 32.0, TEXT);
        ui.text_r(&format!("{} $", me.money), 1224.0, 108.0, 30.0, green);
        let left = self.buy_time_left().unwrap_or(0.0);
        ui.text_c(&format!("Fermeture des achats dans {} s — zone d'achat uniquement", left.ceil()), 640.0, 106.0, 16.0, DIM);

        let cols = [56.0, 352.0, 648.0, 944.0];
        let mut info: Option<String> = None;
        let mut order: Option<BuyItem> = None;
        let refunds = self.refunds.clone();
        let refund = |item: BuyItem| refunds.iter().find(|(i, _)| *i == item).map(|(_, v)| *v);
        let mut sold: Option<BuyItem> = None;
        // Ligne d'article : (achat cliqué, survolée, revente cliquée, revente survolée).
        // `state` remplace le prix quand l'article n'est plus achetable ; `refund` affiche
        // à droite un bouton de revente (achat de ce round).
        let row = |ui: &mut Ui, x: f32, y: f32, name: &str, price: u32, state: Option<&str>, refund: Option<u32>| -> (bool, bool, bool, bool) {
            let r = Rect::new(x, y, W, 38.0);
            let sell_r = Rect::new(x + W - 94.0, y + 5.0, 88.0, 28.0);
            let sell_hot = refund.is_some() && ui.hover(sell_r);
            let afford = me.money >= price && state.is_none();
            let hot = ui.hover(r);
            let buy_hot = hot && afford && !sell_hot;
            ui.rect(r, if buy_hot { WIDGET_HOT } else { WIDGET });
            if buy_hot {
                ui.rect(Rect::new(r.x, r.y, 4.0, r.h), ACCENT);
            }
            ui.text(name, x + 14.0, y + 26.0, 19.0, if afford || refund.is_some() { TEXT } else { DIM });
            match (refund, state) {
                (Some(v), _) => {
                    ui.rect(sell_r, if sell_hot { ACCENT } else { Color::new(0.96, 0.60, 0.14, 0.22) });
                    ui.text_c(&format!("+{} $", v), sell_r.x + sell_r.w / 2.0, y + 25.0, 16.0, if sell_hot { BLACK } else { ACCENT });
                }
                (None, Some(t)) => ui.text_r(t, x + W - 12.0, y + 26.0, 15.0, DIM),
                (None, None) => ui.text_r(&format!("{} $", price), x + W - 12.0, y + 26.0, 18.0, if afford { green } else { Color::new(0.8, 0.35, 0.3, 1.0) }),
            }
            (buy_hot && ui.click, hot, sell_hot && ui.click, sell_hot)
        };
        let sell_info = |name: &str, v: u32| format!("Revendre {} : {} $ rendus (acheté pendant ce round)", name, v);

        let groups = [
            (Class::Pistol, cols[0], 150.0),
            (Class::Smg, cols[1], 150.0),
            (Class::Rifle, cols[2], 150.0),
            (Class::Lmg, cols[1], 360.0),
            (Class::Sniper, cols[2], 360.0),
        ];
        for (class, x, y0) in groups {
            ui.text(class.label(), x, y0 + 16.0, 17.0, ACCENT);
            let mut y = y0 + 28.0;
            for d in WEAPONS.iter().filter(|d| d.class == class) {
                let owned = me.primary == d.id || me.secondary == d.id;
                let back = refund(BuyItem::Weapon(d.id));
                let (click, hot, sell, sell_hot) = row(ui, x, y, d.name, d.price, owned.then_some("possédé"), back);
                if click {
                    order = Some(BuyItem::Weapon(d.id));
                }
                if sell {
                    sold = Some(BuyItem::Weapon(d.id));
                }
                if sell_hot {
                    info = Some(sell_info(d.name, back.unwrap_or(0)));
                } else if hot {
                    info = Some(format!(
                        "{} — dégâts {:.0} · cadence {:.0}/min · chargeur {} · pénétration du gilet {:.0} % · prime {} $",
                        d.name,
                        d.damage,
                        d.rpm,
                        d.mag,
                        d.armor_pen * 100.0,
                        d.kill_reward
                    ));
                }
                y += 44.0;
            }
        }

        // Équipement
        ui.text("Équipement", cols[0], 376.0, 17.0, ACCENT);
        let full = me.armor >= 100;
        let gear: [(&str, u32, bool, BuyItem, &str); 3] = [
            ("Gilet pare-balles", PRICE_KEVLAR, full, BuyItem::Kevlar, "Gilet pare-balles — réduit les dégâts au corps"),
            (
                "Gilet + casque",
                if full { PRICE_HELMET_ONLY } else { PRICE_KEVLAR_HELMET },
                full && me.helmet,
                BuyItem::KevlarHelmet,
                "Gilet + casque — protège aussi des tirs à la tête",
            ),
            ("Kit de désamorçage", PRICE_KIT, me.kit, BuyItem::Kit, "Kit de désamorçage — désamorce en 5 s au lieu de 10 (Défenseurs)"),
        ];
        let mut y = 388.0;
        for (name, price, owned, item, text) in gear {
            let back = refund(item);
            // Le kit est réservé aux Défenseurs.
            let state = if owned { Some("possédé") } else if item == BuyItem::Kit && me.team != Team::CT { Some("Défenseurs") } else { None };
            let (click, hot, sell, sell_hot) = row(ui, cols[0], y, name, price, state, back);
            if click {
                order = Some(item);
            }
            if sell {
                sold = Some(item);
            }
            if sell_hot {
                info = Some(sell_info(name, back.unwrap_or(0)));
            } else if hot {
                info = Some(text.to_string());
            }
            y += 44.0;
        }

        // Grenades : quatre au total, une de chaque type (deux aveuglantes).
        let total: u8 = me.nades.iter().sum();
        ui.text(&format!("Grenades ({}/{})", total, MAX_GRENADES), cols[3], 166.0, 17.0, ACCENT);
        let labels = ["Fragmentation", "Fumigène", "Aveuglante", "Incendiaire"];
        let mut y = 178.0;
        for (k, g) in GRENADES.iter().enumerate() {
            let have = me.nades[k];
            let name = if g.max > 1 { format!("{} ({}/{})", labels[k], have, g.max) } else { labels[k].to_string() };
            let state = if have >= g.max {
                Some("possédée")
            } else if total >= MAX_GRENADES {
                Some("maximum")
            } else {
                None
            };
            let back = refund(BuyItem::Grenade(k as u8));
            let (click, hot, sell, sell_hot) = row(ui, cols[3], y, &name, g.price, state, back);
            if click {
                order = Some(BuyItem::Grenade(k as u8));
            }
            if sell {
                sold = Some(BuyItem::Grenade(k as u8));
            }
            if sell_hot {
                info = Some(sell_info(labels[k], back.unwrap_or(0)));
            } else if hot {
                info = Some(g.info.to_string());
            }
            y += 44.0;
        }
        let keys: Vec<String> = NADE_ACTIONS.iter().map(|a| st.bind(*a).label()).collect();
        ui.text(&format!("[{}] : une touche par grenade,", keys.join("] [")), cols[3], 384.0, 14.0, DIM);
        ui.text(&format!("[{}] : type suivant.", st.bind(Action::Grenade).label()), cols[3], 402.0, 14.0, DIM);
        ui.text("Clic gauche : lancer long", cols[3], 430.0, 14.0, DIM);
        ui.text("Clic droit : lancer court", cols[3], 448.0, 14.0, DIM);

        if let Some(t) = info {
            ui.text(&t, 56.0, 590.0, 16.0, TEXT);
        }
        ui.text(&format!("[{}] ou [Échap] pour fermer", st.bind(Action::Buy).label()), 56.0, 636.0, 15.0, DIM);
        if !self.refunds.is_empty() {
            ui.text("Cliquez sur « +… $ » pour revendre un achat de ce round", 400.0, 636.0, 15.0, ACCENT);
        }
        if let Some(item) = order {
            self.buy(item, audio);
        }
        if let Some(item) = sold {
            self.sell(item, audio);
        }
        if st.pressed(Action::Buy) && self.overlay_since < self.now {
            self.overlay = Overlay::None;
        }
    }

    /// Ligne de saisie de la discussion.
    pub fn draw_chat_input(&mut self, ui: &mut Ui) {
        let r = Rect::new(20.0, 596.0, 460.0, 36.0);
        ui.focus = Some(900);
        ui.textfield(900, r, &mut self.chat_input, 100);
        ui.text("Entrée pour envoyer, Échap pour annuler", 22.0, 652.0, 14.0, DIM);
        if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
            let text = self.chat_input.trim().to_string();
            if !text.is_empty() {
                self.net.reliable(ClientMsg::Chat(text));
            }
            self.chat_input.clear();
            self.overlay = Overlay::None;
            ui.focus = None;
        }
    }
}
