pub mod t {
    pub const END: usize = 0;
    pub const ERROR: usize = 1;
    pub const NEWLINE: usize = 2;
    pub const TITLE: usize = 3;
    pub const TITLE_VALUE: usize = 4;
    pub const ACC_TITLE: usize = 5;
    pub const ACC_TITLE_VALUE: usize = 6;
    pub const ACC_DESCR: usize = 7;
    pub const ACC_DESCR_VALUE: usize = 8;
    pub const ACC_DESCR_MULTILINE_VALUE: usize = 9;
    pub const X_AXIS: usize = 10;
    pub const Y_AXIS: usize = 11;
    pub const DELIMITER: usize = 12;
    pub const QUADRANT_1: usize = 13;
    pub const QUADRANT_2: usize = 14;
    pub const QUADRANT_3: usize = 15;
    pub const QUADRANT_4: usize = 16;
    pub const CLASSDEF: usize = 17;
    pub const MD_STR: usize = 18;
    pub const STR: usize = 19;
    pub const CLASS_NAME: usize = 20;
    pub const POINT_START: usize = 21;
    pub const POINT_X: usize = 22;
    pub const POINT_Y: usize = 23;
    pub const QUADRANT: usize = 24;
    pub const ALPHA: usize = 25;
    pub const UNICODE_TEXT: usize = 26;
    pub const COLON: usize = 27;
    pub const PLUS: usize = 28;
    pub const COMMA: usize = 29;
    pub const EQUALS: usize = 30;
    pub const MULT: usize = 31;
    pub const BRKT: usize = 32;
    pub const UNDERSCORE: usize = 33;
    pub const DOT: usize = 34;
    pub const AMP: usize = 35;
    pub const MINUS: usize = 36;
    pub const NUM: usize = 37;
    pub const SPACE: usize = 38;
    pub const SEMI: usize = 39;
    pub const PUNCTUATION: usize = 40;
    pub const EOF: usize = 41;
}

pub const TOKEN_NAMES: [&str; 42] = [
    "$end",
    "INVALID",
    "NEWLINE",
    "title",
    "title_value",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "X-AXIS",
    "Y-AXIS",
    "AXIS-TEXT-DELIMITER",
    "QUADRANT_1",
    "QUADRANT_2",
    "QUADRANT_3",
    "QUADRANT_4",
    "CLASSDEF",
    "MD_STR",
    "STR",
    "class_name",
    "point_start",
    "point_x",
    "point_y",
    "QUADRANT",
    "ALPHA",
    "UNICODE_TEXT",
    "COLON",
    "PLUS",
    "COMMA",
    "EQUALS",
    "MULT",
    "BRKT",
    "UNDERSCORE",
    "DOT",
    "AMP",
    "MINUS",
    "NUM",
    "SPACE",
    "SEMI",
    "PUNCTUATION",
    "EOF",
];
