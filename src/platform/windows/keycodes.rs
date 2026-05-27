#![allow(dead_code)]
/// Windows Virtual Key codes used by mozkeys.
/// We define only what we need; avoids pulling in large generated tables.

pub type Vk = u16;

pub const VK_LBUTTON: Vk    = 0x01;
pub const VK_RBUTTON: Vk    = 0x02;
pub const VK_MBUTTON: Vk    = 0x04;
pub const VK_BACK: Vk       = 0x08;
pub const VK_TAB: Vk        = 0x09;
pub const VK_RETURN: Vk     = 0x0D;
pub const VK_SHIFT: Vk      = 0x10;
pub const VK_CONTROL: Vk    = 0x11;
pub const VK_MENU: Vk       = 0x12; // Alt
pub const VK_CAPITAL: Vk    = 0x14; // CapsLock
pub const VK_ESCAPE: Vk     = 0x1B;
pub const VK_SPACE: Vk      = 0x20;
pub const VK_PRIOR: Vk      = 0x21; // Page Up
pub const VK_NEXT: Vk       = 0x22; // Page Down
pub const VK_END: Vk        = 0x23;
pub const VK_HOME: Vk       = 0x24;
pub const VK_LEFT: Vk       = 0x25;
pub const VK_UP: Vk         = 0x26;
pub const VK_RIGHT: Vk      = 0x27;
pub const VK_DOWN: Vk       = 0x28;
pub const VK_INSERT: Vk     = 0x2D;
pub const VK_DELETE: Vk     = 0x2E;
pub const VK_LSHIFT: Vk     = 0xA0;
pub const VK_RSHIFT: Vk     = 0xA1;
pub const VK_LCONTROL: Vk   = 0xA2;
pub const VK_RCONTROL: Vk   = 0xA3;
pub const VK_LMENU: Vk      = 0xA4; // Left Alt
pub const VK_RMENU: Vk      = 0xA5; // Right Alt

/// Parse a human-readable key name (from config) into a VK code.
/// Returns None if unrecognised.
pub fn parse_key(s: &str) -> Option<Vk> {
    match s.to_lowercase().as_str() {
        "capslock"  => Some(VK_CAPITAL),
        "shift"     => Some(VK_SHIFT),
        "ctrl" | "control" => Some(VK_CONTROL),
        "alt" | "menu"     => Some(VK_MENU),
        "up"        => Some(VK_UP),
        "down"      => Some(VK_DOWN),
        "left"      => Some(VK_LEFT),
        "right"     => Some(VK_RIGHT),
        "space"     => Some(VK_SPACE),
        "escape" | "esc"   => Some(VK_ESCAPE),
        "return" | "enter" => Some(VK_RETURN),
        "pageup"    => Some(VK_PRIOR),
        "pagedown"  => Some(VK_NEXT),
        "home"      => Some(VK_HOME),
        "end"       => Some(VK_END),
        "insert"    => Some(VK_INSERT),
        "delete" | "del"   => Some(VK_DELETE),
        "lshift"    => Some(VK_LSHIFT),
        "rshift"    => Some(VK_RSHIFT),
        "lctrl"     => Some(VK_LCONTROL),
        "rctrl"     => Some(VK_RCONTROL),
        "lalt"      => Some(VK_LMENU),
        "ralt"      => Some(VK_RMENU),
        _           => None,
    }
}
