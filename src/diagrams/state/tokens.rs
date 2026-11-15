#[allow(non_upper_case_globals, dead_code)]
pub mod t {
    pub const END: u16 = 0;
    pub const ERROR: u16 = 1;
    pub const NL: u16 = 2;
    pub const SD: u16 = 3;
    pub const DESCR: u16 = 4;
    pub const ARROW: u16 = 5;
    pub const HIDE_EMPTY: u16 = 6;
    pub const scale: u16 = 7;
    pub const WIDTH: u16 = 8;
    pub const COMPOSIT_STATE: u16 = 9;
    pub const STRUCT_START: u16 = 10;
    pub const STRUCT_STOP: u16 = 11;
    pub const STATE_DESCR: u16 = 12;
    pub const AS: u16 = 13;
    pub const ID: u16 = 14;
    pub const FORK: u16 = 15;
    pub const JOIN: u16 = 16;
    pub const CHOICE: u16 = 17;
    pub const CONCURRENT: u16 = 18;
    pub const note: u16 = 19;
    pub const NOTE_TEXT: u16 = 20;
    pub const acc_title: u16 = 21;
    pub const acc_title_value: u16 = 22;
    pub const acc_descr: u16 = 23;
    pub const acc_descr_value: u16 = 24;
    pub const acc_descr_multiline_value: u16 = 25;
    pub const CLICK: u16 = 26;
    pub const STRING: u16 = 27;
    pub const HREF: u16 = 28;
    pub const classDef: u16 = 29;
    pub const CLASSDEF_ID: u16 = 30;
    pub const DEFAULT_CLASSDEF_ID: u16 = 31;
    pub const CLASSDEF_STYLEOPTS: u16 = 32;
    pub const DEFAULT: u16 = 33;
    pub const style: u16 = 34;
    pub const STYLE_IDS: u16 = 35;
    pub const STYLEDEF_STYLEOPTS: u16 = 36;
    pub const class: u16 = 37;
    pub const CLASSENTITY_IDS: u16 = 38;
    pub const STYLECLASS: u16 = 39;
    pub const direction_tb: u16 = 40;
    pub const direction_bt: u16 = 41;
    pub const direction_rl: u16 = 42;
    pub const direction_lr: u16 = 43;
    pub const EDGE_STATE: u16 = 44;
    pub const STYLE_SEPARATOR: u16 = 45;
    pub const left_of: u16 = 46;
    pub const right_of: u16 = 47;
    pub const INVALID: u16 = 48;
}

pub const TOKEN_NAMES: [&str; 49] = [
    "$end",
    "INVALID",
    "NL",
    "SD",
    "DESCR",
    "-->",
    "HIDE_EMPTY",
    "scale",
    "WIDTH",
    "COMPOSIT_STATE",
    "STRUCT_START",
    "STRUCT_STOP",
    "STATE_DESCR",
    "AS",
    "ID",
    "FORK",
    "JOIN",
    "CHOICE",
    "CONCURRENT",
    "note",
    "NOTE_TEXT",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "CLICK",
    "STRING",
    "HREF",
    "classDef",
    "CLASSDEF_ID",
    "DEFAULT_CLASSDEF_ID",
    "CLASSDEF_STYLEOPTS",
    "DEFAULT",
    "style",
    "STYLE_IDS",
    "STYLEDEF_STYLEOPTS",
    "class",
    "CLASSENTITY_IDS",
    "STYLECLASS",
    "direction_tb",
    "direction_bt",
    "direction_rl",
    "direction_lr",
    "EDGE_STATE",
    "STYLE_SEPARATOR",
    "left_of",
    "right_of",
    "INVALID",
];
