//! Revente des achats du round : tant que les achats sont ouverts, un objet acheté
//! pendant ce round peut être rendu contre son prix, et l'on retrouve ce qu'on
//! avait avant (l'ancienne arme, l'ancien gilet…).

use crate::game::Server;
use cesi_common::protocol::{BuyItem, ServerMsg};
use cesi_common::weapons::{weapon, Class};

/// Inventaire au moment d'un achat.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Inv {
    primary: u8,
    secondary: u8,
    armor: i32,
    helmet: bool,
    kit: bool,
    nades: [u8; 4],
}

/// Un achat du round, avec l'inventaire avant et après.
#[derive(Clone, Debug)]
pub(crate) struct Purchase {
    item: BuyItem,
    price: u32,
    before: Inv,
    after: Inv,
}

/// Catégorie d'un objet : seul le dernier achat d'une catégorie se revend.
fn category(item: BuyItem) -> u8 {
    match item {
        BuyItem::Weapon(id) if weapon(id).class == Class::Pistol => 1,
        BuyItem::Weapon(_) => 0,
        BuyItem::Kevlar | BuyItem::KevlarHelmet => 2,
        BuyItem::Kit => 3,
        BuyItem::Grenade(k) => 4 + k,
    }
}

/// Partie de l'inventaire concernée par une catégorie.
fn part(inv: &Inv, cat: u8) -> (i32, bool) {
    match cat {
        0 => (inv.primary as i32, false),
        1 => (inv.secondary as i32, false),
        2 => (inv.armor, inv.helmet),
        3 => (0, inv.kit),
        k => (inv.nades[(k - 4) as usize] as i32, false),
    }
}

impl Server {
    pub(crate) fn inventory(&self, i: usize) -> Inv {
        let p = &self.players[i];
        Inv { primary: p.primary, secondary: p.secondary, armor: p.armor, helmet: p.helmet, kit: p.kit, nades: p.nades }
    }

    /// Note un achat réussi (`before` : inventaire avant, `paid` : prix payé).
    pub(crate) fn record_purchase(&mut self, i: usize, item: BuyItem, before: Inv, paid: u32) {
        let after = self.inventory(i);
        self.players[i].bought.push(Purchase { item, price: paid, before, after });
    }

    /// Objets revendables maintenant, avec le montant rendu.
    pub(crate) fn refunds(&self, i: usize) -> Vec<(BuyItem, u32)> {
        let p = &self.players[i];
        if !p.alive || !self.can_buy() || !self.info.in_buy_zone(p.team, p.pos) {
            return Vec::new();
        }
        let now = self.inventory(i);
        let mut out = Vec::new();
        for cat in 0..8 {
            // Le dernier achat de la catégorie, s'il n'a pas changé depuis (arme jetée, gilet abîmé…).
            if let Some(b) = p.bought.iter().rev().find(|b| category(b.item) == cat) {
                if part(&now, cat) == part(&b.after, cat) {
                    out.push((b.item, b.price));
                }
            }
        }
        out
    }

    /// Revend `item` s'il fait partie des achats revendables : remboursement et retour
    /// à l'état d'avant l'achat pour cette catégorie.
    pub(crate) fn sell(&mut self, i: usize, item: BuyItem) -> bool {
        if !self.refunds(i).iter().any(|(it, _)| *it == item) {
            return false;
        }
        let cat = category(item);
        let p = &mut self.players[i];
        let Some(k) = p.bought.iter().rposition(|b| category(b.item) == cat) else { return false };
        let b = p.bought.remove(k);
        p.money += b.price;
        match cat {
            0 => p.primary = b.before.primary,
            1 => p.secondary = b.before.secondary,
            2 => (p.armor, p.helmet) = (b.before.armor, b.before.helmet),
            3 => p.kit = b.before.kit,
            k => p.nades[(k - 4) as usize] = b.before.nades[(k - 4) as usize],
        }
        true
    }

    /// Une arme quitte l'emplacement (jetée ou échangée) : ses achats ne se revendent plus.
    pub(crate) fn forget_weapon_purchases(&mut self, i: usize, pistol: bool) {
        let cat = if pistol { 1 } else { 0 };
        self.players[i].bought.retain(|b| category(b.item) != cat);
    }

    /// Envoie aux joueurs humains la liste de leurs objets revendables quand elle change.
    pub(crate) fn sync_refunds(&mut self) {
        for i in 0..self.players.len() {
            if self.players[i].is_bot {
                continue;
            }
            let list = self.refunds(i);
            if list != self.players[i].refunds_sent {
                let pid = self.players[i].id;
                self.players[i].refunds_sent = list.clone();
                self.send_to(pid, ServerMsg::Refunds(list));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::game::{Player, ServerConfig};
    use crate::Server;
    use cesi_common::protocol::{BuyItem, Phase, Team};
    use cesi_common::weapons::{PRICE_HELMET_ONLY, PRICE_KEVLAR, PRICE_KIT};

    /// Serveur local avec un défenseur vivant dans sa zone d'achat, pendant la préparation.
    fn server(money: u32) -> Server {
        let cfg = ServerConfig { port: 0, local_only: true, ..Default::default() };
        let mut s = Server::bind(cfg, None, Box::new(|_| {})).unwrap();
        let mut p = Player::new(1, "Test".into(), Team::CT, money, None);
        p.alive = true;
        p.pos = s.info.ct_spawns[0].0;
        s.players.push(p);
        s.phase = Phase::Freeze;
        s
    }

    #[test]
    fn weapon_resale_gives_back_the_money_and_the_old_weapon() {
        let mut s = server(10000);
        let pistol = s.players[0].secondary;
        assert!(s.try_buy(0, BuyItem::Weapon(7)));
        assert!(s.try_buy(0, BuyItem::Weapon(8)));
        let spent = 10000 - s.players[0].money;
        // Seul le dernier achat de l'emplacement se revend.
        assert_eq!(s.refunds(0).iter().filter(|(it, _)| matches!(it, BuyItem::Weapon(_))).count(), 1);
        assert!(!s.sell(0, BuyItem::Weapon(7)));
        assert!(s.sell(0, BuyItem::Weapon(8)));
        assert_eq!(s.players[0].primary, 7, "on retrouve l'arme d'avant");
        assert!(s.sell(0, BuyItem::Weapon(7)));
        assert_eq!(s.players[0].primary, 0);
        assert_eq!(s.players[0].money, 10000, "tout est remboursé ({} dépensés)", spent);
        assert_eq!(s.players[0].secondary, pistol);
        assert!(!s.sell(0, BuyItem::Weapon(7)), "rien à revendre deux fois");
    }

    #[test]
    fn vests_and_kit_resale() {
        let mut s = server(5000);
        assert!(s.try_buy(0, BuyItem::Kevlar));
        assert!(s.try_buy(0, BuyItem::KevlarHelmet));
        assert!(s.try_buy(0, BuyItem::Kit));
        assert_eq!(s.players[0].money, 5000 - PRICE_KEVLAR - PRICE_HELMET_ONLY - PRICE_KIT);
        // Le casque d'abord (dernier achat de gilet), puis le gilet.
        assert!(!s.sell(0, BuyItem::Kevlar));
        assert!(s.sell(0, BuyItem::KevlarHelmet));
        assert_eq!((s.players[0].armor, s.players[0].helmet), (100, false));
        assert!(s.sell(0, BuyItem::Kevlar));
        assert_eq!(s.players[0].armor, 0);
        assert!(s.sell(0, BuyItem::Kit));
        assert!(!s.players[0].kit);
        assert_eq!(s.players[0].money, 5000);
    }

    #[test]
    fn no_resale_once_used_or_too_late() {
        let mut s = server(5000);
        assert!(s.try_buy(0, BuyItem::Kevlar));
        s.players[0].armor = 80; // touché : le gilet n'est plus neuf
        assert!(!s.sell(0, BuyItem::Kevlar));
        assert!(s.try_buy(0, BuyItem::Weapon(7)));
        s.forget_weapon_purchases(0, false); // arme jetée
        assert!(!s.sell(0, BuyItem::Weapon(7)));
        assert!(s.try_buy(0, BuyItem::Kit));
        s.phase = Phase::Live;
        s.round_elapsed = 1000.0; // achats fermés
        assert!(s.refunds(0).is_empty());
        assert!(!s.sell(0, BuyItem::Kit));
    }
}
