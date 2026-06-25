//! macOS virtual keycode -> thock key name. Mirrors thock's KeyMapper exactly so
//! the existing soundpacks map to the same keys.

/// Returns the key name for a macOS keycode, or "default" when unmapped.
pub fn key_name(code: i64) -> &'static str {
    match code {
        // numbers
        18 => "1", 19 => "2", 20 => "3", 21 => "4", 23 => "5", 22 => "6",
        26 => "7", 28 => "8", 25 => "9", 29 => "0",
        // numpad
        83 => "1", 84 => "2", 85 => "3", 86 => "4", 87 => "5", 88 => "6",
        89 => "7", 91 => "8", 92 => "9", 82 => "0", 67 => "*", 75 => "/",
        69 => "+", 78 => "-", 81 => "=", 65 => ".", 71 => "clear",
        // letters
        12 => "q", 13 => "w", 14 => "e", 15 => "r", 17 => "t", 16 => "y",
        32 => "u", 34 => "i", 31 => "o", 35 => "p", 0 => "a", 1 => "s",
        2 => "d", 3 => "f", 5 => "g", 4 => "h", 38 => "j", 40 => "k",
        37 => "l", 6 => "z", 7 => "x", 8 => "c", 9 => "v", 11 => "b",
        45 => "n", 46 => "m",
        // symbols
        24 => "=", 27 => "-", 33 => "[", 30 => "]", 41 => ";", 39 => "'",
        43 => ",", 47 => ".", 44 => "/", 42 => "\\", 50 => "`",
        // modifiers / control
        48 => "tab", 49 => "space", 51 => "del", 53 => "esc", 57 => "capsLock",
        59 => "ctrlLeft", 63 => "fn", 36 => "enter", 76 => "enter",
        54 => "command", 55 => "command", 56 => "shiftLeft", 60 => "shiftRight",
        58 => "optionLeft", 61 => "optionRight",
        // navigation
        123 => "arrLeft", 124 => "arrRight", 125 => "arrDown", 126 => "arrUp",
        115 => "home", 119 => "end", 116 => "pgUp", 121 => "pgDn",
        // function
        122 => "f1", 120 => "f2", 99 => "f3", 118 => "f4", 96 => "f5",
        97 => "f6", 98 => "f7", 100 => "f8", 101 => "f9", 109 => "f10",
        103 => "f11", 111 => "f12",
        _ => "default",
    }
}

