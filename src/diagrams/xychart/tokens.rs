pub mod t {
    pub const END: usize = 0;
    pub const ERROR: usize = 1;
    pub const NEWLINE: usize = 2;
    pub const TITLE: usize = 3;
    pub const ACC_TITLE: usize = 4;
    pub const ACC_TITLE_VALUE: usize = 5;
    pub const ACC_DESCR: usize = 6;
    pub const ACC_DESCR_VALUE: usize = 7;
    pub const ACC_DESCR_MULTILINE_VALUE: usize = 8;
    pub const XYCHART: usize = 9;
    pub const CHART_ORIENTATION: usize = 10;
    pub const X_AXIS: usize = 11;
    pub const Y_AXIS: usize = 12;
    pub const ARROW_DELIMITER: usize = 13;
    pub const LINE: usize = 14;
    pub const BAR: usize = 15;
    pub const NUMBER_WITH_DECIMAL: usize = 16;
    pub const SQUARE_BRACES_START: usize = 17;
    pub const SQUARE_BRACES_END: usize = 18;
    pub const STR: usize = 19;
    pub const ALPHA: usize = 20;
    pub const COLON: usize = 21;
    pub const PLUS: usize = 22;
    pub const COMMA: usize = 23;
    pub const EQUALS: usize = 24;
    pub const MULT: usize = 25;
    pub const BRKT: usize = 26;
    pub const UNDERSCORE: usize = 27;
    pub const DOT: usize = 28;
    pub const AMP: usize = 29;
    pub const MINUS: usize = 30;
    pub const NUM: usize = 31;
    pub const SEMI: usize = 32;
    pub const EOF: usize = 33;
}

pub const TOKEN_NAMES: [&str; 34] = [
    "$end",
    "INVALID",
    "NEWLINE",
    "title",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "XYCHART",
    "CHART_ORIENTATION",
    "X_AXIS",
    "Y_AXIS",
    "ARROW_DELIMITER",
    "LINE",
    "BAR",
    "NUMBER_WITH_DECIMAL",
    "SQUARE_BRACES_START",
    "SQUARE_BRACES_END",
    "STR",
    "ALPHA",
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
    "SEMI",
    "EOF",
];
