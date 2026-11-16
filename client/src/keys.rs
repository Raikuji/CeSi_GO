//! Noms des touches : identifiants pour le fichier de réglages et libellés
//! affichés, adaptés à la disposition du clavier (AZERTY, QWERTY…).

use macroquad::prelude::KeyCode;

use KeyCode::*;

pub const ALL_KEYS: &[KeyCode] = &[
    Space, Apostrophe, Comma, Minus, Period, Slash, Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9,
    Semicolon, Equal, A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z, LeftBracket,
    Backslash, RightBracket, GraveAccent, World1, World2, Enter, Tab, Backspace, Insert, Delete, Right, Left, Down,
    Up, PageUp, PageDown, Home, End, CapsLock, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12, Kp0, Kp1, Kp2,
    Kp3, Kp4, Kp5, Kp6, Kp7, Kp8, Kp9, KpDecimal, KpDivide, KpMultiply, KpSubtract, KpAdd, KpEnter, LeftShift,
    LeftControl, LeftAlt, RightShift, RightControl, RightAlt,
];

pub fn key_id(k: KeyCode) -> String {
    format!("{:?}", k)
}

pub fn key_from_id(s: &str) -> Option<KeyCode> {
    ALL_KEYS.iter().copied().find(|k| key_id(*k) == s)
}

/// Code matériel (scancode) des touches dont le symbole dépend de la disposition.
fn scancode(k: KeyCode) -> Option<u32> {
    Some(match k {
        Q => 0x10,
        W => 0x11,
        E => 0x12,
        R => 0x13,
        T => 0x14,
        Y => 0x15,
        U => 0x16,
        I => 0x17,
        O => 0x18,
        P => 0x19,
        A => 0x1E,
        S => 0x1F,
        D => 0x20,
        F => 0x21,
        G => 0x22,
        H => 0x23,
        J => 0x24,
        K => 0x25,
        L => 0x26,
        Z => 0x2C,
        X => 0x2D,
        C => 0x2E,
        V => 0x2F,
        B => 0x30,
        N => 0x31,
        M => 0x32,
        Semicolon => 0x27,
        Apostrophe => 0x28,
        GraveAccent => 0x29,
        Comma => 0x33,
        Period => 0x34,
        Slash => 0x35,
        LeftBracket => 0x1A,
        RightBracket => 0x1B,
        Backslash => 0x2B,
        Minus => 0x0C,
        Equal => 0x0D,
        World2 => 0x56,
        _ => return None,
    })
}

#[cfg(windows)]
fn layout_char(sc: u32) -> Option<char> {
    #[link(name = "user32")]
    extern "system" {
        fn MapVirtualKeyW(code: u32, map_type: u32) -> u32;
    }
    // 1 = scancode -> touche virtuelle, 2 = touche virtuelle -> caractère.
    let ch = unsafe {
        let vk = MapVirtualKeyW(sc, 1);
        if vk == 0 {
            return None;
        }
        MapVirtualKeyW(vk, 2) & 0x7FFF_FFFF
    };
    char::from_u32(ch).filter(|c| !c.is_control() && *c != ' ')
}

#[cfg(not(windows))]
fn layout_char(_sc: u32) -> Option<char> {
    None
}

/// Libellé affiché dans le menu des touches.
pub fn key_label(k: KeyCode) -> String {
    let fixed = match k {
        Space => "Espace",
        Enter => "Entrée",
        Tab => "Tab",
        Backspace => "Retour arrière",
        Insert => "Inser",
        Delete => "Suppr",
        Right => "Flèche droite",
        Left => "Flèche gauche",
        Down => "Flèche bas",
        Up => "Flèche haut",
        PageUp => "Page préc.",
        PageDown => "Page suiv.",
        Home => "Début",
        End => "Fin",
        CapsLock => "Verr. maj",
        LeftShift => "Maj gauche",
        RightShift => "Maj droite",
        LeftControl => "Ctrl gauche",
        RightControl => "Ctrl droite",
        LeftAlt => "Alt",
        RightAlt => "Alt Gr",
        KpDecimal => "Pavé .",
        KpDivide => "Pavé /",
        KpMultiply => "Pavé *",
        KpSubtract => "Pavé -",
        KpAdd => "Pavé +",
        KpEnter => "Pavé Entrée",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    if let Some(c) = scancode(k).and_then(layout_char) {
        return c.to_uppercase().collect();
    }
    let id = key_id(k);
    if let Some(d) = id.strip_prefix("Key") {
        return d.to_string();
    }
    if let Some(d) = id.strip_prefix("Kp") {
        return format!("Pavé {}", d);
    }
    id
}
