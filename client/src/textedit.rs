//! Édition d'une ligne de texte : curseur, sélection, insertion et suppression.
//! Les positions sont des indices de caractères, pas d'octets.

/// Raccourci d'édition.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmd {
    SelectAll,
    Copy,
    Cut,
    Paste,
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Edit {
    /// Position du curseur.
    pub caret: usize,
    /// Autre extrémité de la sélection (égale au curseur quand rien n'est sélectionné).
    pub anchor: usize,
}

fn byte(text: &str, i: usize) -> usize {
    text.char_indices().nth(i).map_or(text.len(), |(b, _)| b)
}

impl Edit {
    pub fn at_end(text: &str) -> Edit {
        let n = text.chars().count();
        Edit { caret: n, anchor: n }
    }

    /// Ramène le curseur dans le texte, s'il a été modifié ailleurs.
    pub fn clamp(&mut self, text: &str) {
        let n = text.chars().count();
        self.caret = self.caret.min(n);
        self.anchor = self.anchor.min(n);
    }

    pub fn range(&self) -> (usize, usize) {
        (self.caret.min(self.anchor), self.caret.max(self.anchor))
    }

    pub fn has_selection(&self) -> bool {
        self.caret != self.anchor
    }

    pub fn selected<'a>(&self, text: &'a str) -> &'a str {
        let (a, b) = self.range();
        &text[byte(text, a)..byte(text, b)]
    }

    pub fn select_all(&mut self, text: &str) {
        self.anchor = 0;
        self.caret = text.chars().count();
    }

    /// Place le curseur ; `extend` : en gardant l'autre bout de la sélection.
    pub fn move_to(&mut self, i: usize, extend: bool) {
        self.caret = i;
        if !extend {
            self.anchor = i;
        }
    }

    /// Flèche gauche (`dir` négatif) ou droite. Sans `extend`, une sélection se
    /// referme sur son bord.
    pub fn step(&mut self, text: &str, dir: i32, extend: bool) {
        let (a, b) = self.range();
        let to = if !extend && a != b {
            if dir < 0 { a } else { b }
        } else if dir < 0 {
            self.caret.saturating_sub(1)
        } else {
            (self.caret + 1).min(text.chars().count())
        };
        self.move_to(to, extend);
    }

    pub fn delete_selection(&mut self, text: &mut String) -> bool {
        let (a, b) = self.range();
        if a == b {
            return false;
        }
        text.replace_range(byte(text, a)..byte(text, b), "");
        self.move_to(a, false);
        true
    }

    /// Remplace la sélection par `s`, dans la limite de `max` caractères.
    /// Les caractères de contrôle (retours à la ligne d'un texte collé…) sont ignorés.
    pub fn insert(&mut self, text: &mut String, s: &str, max: usize) -> bool {
        let mut changed = self.delete_selection(text);
        let mut n = text.chars().count();
        for c in s.chars().filter(|c| !c.is_control()) {
            if n >= max {
                break;
            }
            text.insert(byte(text, self.caret), c);
            self.caret += 1;
            n += 1;
            changed = true;
        }
        self.anchor = self.caret;
        changed
    }

    /// Retour arrière, ou Suppr si `forward` : efface la sélection, sinon un caractère.
    pub fn erase(&mut self, text: &mut String, forward: bool) -> bool {
        if self.delete_selection(text) {
            return true;
        }
        let i = if forward {
            self.caret
        } else if self.caret > 0 {
            self.caret - 1
        } else {
            return false;
        };
        if i >= text.chars().count() {
            return false;
        }
        text.remove(byte(text, i));
        self.move_to(i, false);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_replaces_the_selection() {
        let mut t = String::from("192.168.1.42:27500");
        let mut e = Edit { anchor: 13, caret: 18 };
        assert_eq!(e.selected(&t), "27500");
        assert!(e.insert(&mut t, "27015", 60));
        assert_eq!(t, "192.168.1.42:27015");
        assert_eq!(e, Edit { caret: 18, anchor: 18 });
    }

    #[test]
    fn paste_respects_the_limit_and_drops_line_breaks() {
        let mut t = String::from("ab");
        let mut e = Edit { caret: 1, anchor: 1 };
        assert!(e.insert(&mut t, "12\r\n3456", 6));
        assert_eq!(t, "a1234b");
        assert_eq!(e.caret, 5);
        assert!(!e.insert(&mut t, "x", 6), "le champ est plein");
    }

    #[test]
    fn erasing_and_moving() {
        let mut t = String::from("abcd");
        let mut e = Edit::at_end(&t);
        assert!(e.erase(&mut t, false));
        assert!(!e.erase(&mut t, true), "rien après le curseur");
        e.move_to(0, false);
        assert!(!e.erase(&mut t, false), "rien avant le curseur");
        assert!(e.erase(&mut t, true));
        assert_eq!(t, "bc");
        // Maj+flèche étend la sélection, une flèche seule la referme sur son bord.
        e.step(&t, 1, true);
        e.step(&t, 1, true);
        e.step(&t, 1, true);
        assert_eq!(e.selected(&t), "bc");
        e.step(&t, -1, false);
        assert_eq!(e, Edit { caret: 0, anchor: 0 });
        e.select_all(&t);
        assert!(e.erase(&mut t, false));
        assert!(t.is_empty());
    }

    #[test]
    fn positions_count_characters_not_bytes() {
        let mut t = String::from("Élève");
        let mut e = Edit { anchor: 1, caret: 3 };
        assert_eq!(e.selected(&t), "lè");
        assert!(e.delete_selection(&mut t));
        assert_eq!(t, "Éve");
        assert!(e.insert(&mut t, "tê", 16));
        assert_eq!(t, "Étêve");
        t.clear();
        e.clamp(&t);
        assert_eq!(e, Edit::default());
    }
}
