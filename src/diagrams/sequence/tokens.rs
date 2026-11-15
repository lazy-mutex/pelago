#[allow(non_upper_case_globals)]
pub mod t {
    pub const END: usize = 0;
    pub const ERROR: usize = 1;
    pub const INVALID: usize = 2;
    pub const NEWLINE: usize = 3;
    pub const SD: usize = 4;
    pub const create: usize = 5;
    pub const r#box: usize = 6;
    pub const restOfLine: usize = 7;
    pub const end: usize = 8;
    pub const autonumber: usize = 9;
    pub const NUM: usize = 10;
    pub const off: usize = 11;
    pub const activate: usize = 12;
    pub const deactivate: usize = 13;
    pub const title: usize = 14;
    pub const legacy_title: usize = 15;
    pub const acc_title: usize = 16;
    pub const acc_title_value: usize = 17;
    pub const acc_descr: usize = 18;
    pub const acc_descr_value: usize = 19;
    pub const acc_descr_multiline_value: usize = 20;
    pub const r#loop: usize = 21;
    pub const rect: usize = 22;
    pub const opt: usize = 23;
    pub const alt: usize = 24;
    pub const par: usize = 25;
    pub const par_over: usize = 26;
    pub const critical: usize = 27;
    pub const r#break: usize = 28;
    pub const option: usize = 29;
    pub const and: usize = 30;
    pub const r#else: usize = 31;
    pub const participant: usize = 32;
    pub const AS: usize = 33;
    pub const participant_actor: usize = 34;
    pub const destroy: usize = 35;
    pub const note: usize = 36;
    pub const over: usize = 37;
    pub const links: usize = 38;
    pub const link: usize = 39;
    pub const properties: usize = 40;
    pub const details: usize = 41;
    pub const COMMA: usize = 42;
    pub const left_of: usize = 43;
    pub const right_of: usize = 44;
    pub const PLUS: usize = 45;
    pub const MINUS: usize = 46;
    pub const CENTRAL: usize = 47;
    pub const ACTOR: usize = 48;
    pub const CONFIG_START: usize = 49;
    pub const CONFIG_CONTENT: usize = 50;
    pub const CONFIG_END: usize = 51;
    pub const TXT: usize = 52;
    pub const SOLID_OPEN_ARROW: usize = 53;
    pub const DOTTED_OPEN_ARROW: usize = 54;
    pub const SOLID_ARROW: usize = 55;
    pub const BIDIRECTIONAL_SOLID_ARROW: usize = 56;
    pub const DOTTED_ARROW: usize = 57;
    pub const BIDIRECTIONAL_DOTTED_ARROW: usize = 58;
    pub const SOLID_CROSS: usize = 59;
    pub const DOTTED_CROSS: usize = 60;
    pub const SOLID_POINT: usize = 61;
    pub const DOTTED_POINT: usize = 62;
    pub const SOLID_ARROW_TOP: usize = 63;
    pub const SOLID_ARROW_BOTTOM: usize = 64;
    pub const STICK_ARROW_TOP: usize = 65;
    pub const STICK_ARROW_BOTTOM: usize = 66;
    pub const SOLID_ARROW_TOP_DOTTED: usize = 67;
    pub const SOLID_ARROW_BOTTOM_DOTTED: usize = 68;
    pub const STICK_ARROW_TOP_DOTTED: usize = 69;
    pub const STICK_ARROW_BOTTOM_DOTTED: usize = 70;
    pub const SOLID_ARROW_TOP_REVERSE: usize = 71;
    pub const SOLID_ARROW_BOTTOM_REVERSE: usize = 72;
    pub const STICK_ARROW_TOP_REVERSE: usize = 73;
    pub const STICK_ARROW_BOTTOM_REVERSE: usize = 74;
    pub const SOLID_ARROW_TOP_REVERSE_DOTTED: usize = 75;
    pub const SOLID_ARROW_BOTTOM_REVERSE_DOTTED: usize = 76;
    pub const STICK_ARROW_TOP_REVERSE_DOTTED: usize = 77;
    pub const STICK_ARROW_BOTTOM_REVERSE_DOTTED: usize = 78;
    pub const COUNT: usize = 79;
}

// Names as Mermaid's grammar spells them, for error messages and for comparison with its parser.
pub const TOKEN_NAMES: [&str; t::COUNT] = [
    "$end",
    "INVALID",
    "INVALID",
    "NEWLINE",
    "SD",
    "create",
    "box",
    "restOfLine",
    "end",
    "autonumber",
    "NUM",
    "off",
    "activate",
    "deactivate",
    "title",
    "legacy_title",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "loop",
    "rect",
    "opt",
    "alt",
    "par",
    "par_over",
    "critical",
    "break",
    "option",
    "and",
    "else",
    "participant",
    "AS",
    "participant_actor",
    "destroy",
    "note",
    "over",
    "links",
    "link",
    "properties",
    "details",
    ",",
    "left_of",
    "right_of",
    "+",
    "-",
    "()",
    "ACTOR",
    "CONFIG_START",
    "CONFIG_CONTENT",
    "CONFIG_END",
    "TXT",
    "SOLID_OPEN_ARROW",
    "DOTTED_OPEN_ARROW",
    "SOLID_ARROW",
    "BIDIRECTIONAL_SOLID_ARROW",
    "DOTTED_ARROW",
    "BIDIRECTIONAL_DOTTED_ARROW",
    "SOLID_CROSS",
    "DOTTED_CROSS",
    "SOLID_POINT",
    "DOTTED_POINT",
    "SOLID_ARROW_TOP",
    "SOLID_ARROW_BOTTOM",
    "STICK_ARROW_TOP",
    "STICK_ARROW_BOTTOM",
    "SOLID_ARROW_TOP_DOTTED",
    "SOLID_ARROW_BOTTOM_DOTTED",
    "STICK_ARROW_TOP_DOTTED",
    "STICK_ARROW_BOTTOM_DOTTED",
    "SOLID_ARROW_TOP_REVERSE",
    "SOLID_ARROW_BOTTOM_REVERSE",
    "STICK_ARROW_TOP_REVERSE",
    "STICK_ARROW_BOTTOM_REVERSE",
    "SOLID_ARROW_TOP_REVERSE_DOTTED",
    "SOLID_ARROW_BOTTOM_REVERSE_DOTTED",
    "STICK_ARROW_TOP_REVERSE_DOTTED",
    "STICK_ARROW_BOTTOM_REVERSE_DOTTED",
];
