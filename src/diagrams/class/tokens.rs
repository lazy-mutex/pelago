#[allow(non_upper_case_globals, dead_code)]
pub mod t {
    pub const END: u16 = 0;
    pub const ERROR: u16 = 1;
    pub const CLASS_DIAGRAM: u16 = 2;
    pub const NEWLINE: u16 = 3;
    pub const EOF: u16 = 4;
    pub const SQS: u16 = 5;
    pub const STR: u16 = 6;
    pub const SQE: u16 = 7;
    pub const DOT: u16 = 8;
    pub const GENERICTYPE: u16 = 9;
    pub const LABEL: u16 = 10;
    pub const acc_title: u16 = 11;
    pub const acc_title_value: u16 = 12;
    pub const acc_descr: u16 = 13;
    pub const acc_descr_value: u16 = 14;
    pub const acc_descr_multiline_value: u16 = 15;
    pub const STRUCT_START: u16 = 16;
    pub const STRUCT_STOP: u16 = 17;
    pub const NAMESPACE: u16 = 18;
    pub const STYLE_SEPARATOR: u16 = 19;
    pub const ANNOTATION_START: u16 = 20;
    pub const ANNOTATION_END: u16 = 21;
    pub const CLASS: u16 = 22;
    pub const MEMBER: u16 = 23;
    pub const NOTE_FOR: u16 = 24;
    pub const NOTE: u16 = 25;
    pub const CLASSDEF: u16 = 26;
    pub const ALPHA: u16 = 27;
    pub const COMMA: u16 = 28;
    pub const direction_tb: u16 = 29;
    pub const direction_bt: u16 = 30;
    pub const direction_rl: u16 = 31;
    pub const direction_lr: u16 = 32;
    pub const AGGREGATION: u16 = 33;
    pub const EXTENSION: u16 = 34;
    pub const COMPOSITION: u16 = 35;
    pub const DEPENDENCY: u16 = 36;
    pub const LOLLIPOP: u16 = 37;
    pub const LINE: u16 = 38;
    pub const DOTTED_LINE: u16 = 39;
    pub const CALLBACK: u16 = 40;
    pub const LINK: u16 = 41;
    pub const LINK_TARGET: u16 = 42;
    pub const CLICK: u16 = 43;
    pub const CALLBACK_NAME: u16 = 44;
    pub const CALLBACK_ARGS: u16 = 45;
    pub const HREF: u16 = 46;
    pub const STYLE: u16 = 47;
    pub const CSSCLASS: u16 = 48;
    pub const COLON: u16 = 49;
    pub const BRKT: u16 = 50;
    pub const PCT: u16 = 51;
    pub const MINUS: u16 = 52;
    pub const UNICODE_TEXT: u16 = 53;
    pub const BQUOTE_STR: u16 = 54;
    pub const EDGE_STATE: u16 = 55;
    pub const EOF_IN_STRUCT: u16 = 56;
    pub const OPEN_IN_STRUCT: u16 = 57;
    pub const PLUS: u16 = 58;
    pub const EQUALS: u16 = 59;
    pub const PUNCTUATION: u16 = 60;
}

pub const TOKEN_NAMES: [&str; 61] = [
    "$end",
    "INVALID",
    "CLASS_DIAGRAM",
    "NEWLINE",
    "EOF",
    "SQS",
    "STR",
    "SQE",
    "DOT",
    "GENERICTYPE",
    "LABEL",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "STRUCT_START",
    "STRUCT_STOP",
    "NAMESPACE",
    "STYLE_SEPARATOR",
    "ANNOTATION_START",
    "ANNOTATION_END",
    "CLASS",
    "MEMBER",
    "NOTE_FOR",
    "NOTE",
    "CLASSDEF",
    "ALPHA",
    "COMMA",
    "direction_tb",
    "direction_bt",
    "direction_rl",
    "direction_lr",
    "AGGREGATION",
    "EXTENSION",
    "COMPOSITION",
    "DEPENDENCY",
    "LOLLIPOP",
    "LINE",
    "DOTTED_LINE",
    "CALLBACK",
    "LINK",
    "LINK_TARGET",
    "CLICK",
    "CALLBACK_NAME",
    "CALLBACK_ARGS",
    "HREF",
    "STYLE",
    "CSSCLASS",
    "COLON",
    "BRKT",
    "PCT",
    "MINUS",
    "UNICODE_TEXT",
    "BQUOTE_STR",
    "EDGE_STATE",
    "EOF_IN_STRUCT",
    "OPEN_IN_STRUCT",
    "PLUS",
    "EQUALS",
    "PUNCTUATION",
];
