pub mod t {
    pub const END: i32 = 0;
    pub const ERROR: i32 = 1;
    pub const TITLE: i32 = 2;
    pub const ACC_TITLE: i32 = 3;
    pub const ACC_TITLE_VALUE: i32 = 4;
    pub const ACC_DESCR: i32 = 5;
    pub const ACC_DESCR_VALUE: i32 = 6;
    pub const ACC_DESCR_MULTILINE_VALUE: i32 = 7;
    pub const DIRECTION_TB: i32 = 8;
    pub const DIRECTION_BT: i32 = 9;
    pub const DIRECTION_RL: i32 = 10;
    pub const DIRECTION_LR: i32 = 11;
    pub const NEWLINE: i32 = 12;
    pub const EOF: i32 = 13;
    pub const RD: i32 = 14;
    pub const STRUCT_START: i32 = 15;
    pub const STRUCT_STOP: i32 = 16;
    pub const STYLE_SEPARATOR: i32 = 17;
    pub const COLONSEP: i32 = 18;
    pub const ID: i32 = 19;
    pub const TEXT: i32 = 20;
    pub const RISK: i32 = 21;
    pub const VERIFYMTHD: i32 = 22;
    pub const REQUIREMENT: i32 = 23;
    pub const FUNCTIONAL_REQUIREMENT: i32 = 24;
    pub const INTERFACE_REQUIREMENT: i32 = 25;
    pub const PERFORMANCE_REQUIREMENT: i32 = 26;
    pub const PHYSICAL_REQUIREMENT: i32 = 27;
    pub const DESIGN_CONSTRAINT: i32 = 28;
    pub const LOW_RISK: i32 = 29;
    pub const MED_RISK: i32 = 30;
    pub const HIGH_RISK: i32 = 31;
    pub const VERIFY_ANALYSIS: i32 = 32;
    pub const VERIFY_DEMONSTRATION: i32 = 33;
    pub const VERIFY_INSPECTION: i32 = 34;
    pub const VERIFY_TEST: i32 = 35;
    pub const ELEMENT: i32 = 36;
    pub const CONTAINS: i32 = 37;
    pub const COPIES: i32 = 38;
    pub const DERIVES: i32 = 39;
    pub const SATISFIES: i32 = 40;
    pub const VERIFIES: i32 = 41;
    pub const REFINES: i32 = 42;
    pub const TRACES: i32 = 43;
    pub const TYPE: i32 = 44;
    pub const DOCREF: i32 = 45;
    pub const STYLE: i32 = 46;
    pub const CLASSDEF: i32 = 47;
    pub const CLASS: i32 = 48;
    pub const ALPHA: i32 = 49;
    pub const COLON: i32 = 50;
    pub const SEMICOLON: i32 = 51;
    pub const PERCENT: i32 = 52;
    pub const MINUS: i32 = 53;
    pub const BRKT: i32 = 54;
    pub const END_ARROW_L: i32 = 55;
    pub const END_ARROW_R: i32 = 56;
    pub const LINE: i32 = 57;
    pub const Q_STRING: i32 = 58;
    pub const UNQ_STRING: i32 = 59;
    pub const COMMA: i32 = 60;
}

pub const TOKEN_NAMES: &[&str; 61] = &[
    "$end",
    "INVALID",
    "title",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "direction_tb",
    "direction_bt",
    "direction_rl",
    "direction_lr",
    "NEWLINE",
    "EOF",
    "RD",
    "STRUCT_START",
    "STRUCT_STOP",
    "STYLE_SEPARATOR",
    "COLONSEP",
    "ID",
    "TEXT",
    "RISK",
    "VERIFYMTHD",
    "REQUIREMENT",
    "FUNCTIONAL_REQUIREMENT",
    "INTERFACE_REQUIREMENT",
    "PERFORMANCE_REQUIREMENT",
    "PHYSICAL_REQUIREMENT",
    "DESIGN_CONSTRAINT",
    "LOW_RISK",
    "MED_RISK",
    "HIGH_RISK",
    "VERIFY_ANALYSIS",
    "VERIFY_DEMONSTRATION",
    "VERIFY_INSPECTION",
    "VERIFY_TEST",
    "ELEMENT",
    "CONTAINS",
    "COPIES",
    "DERIVES",
    "SATISFIES",
    "VERIFIES",
    "REFINES",
    "TRACES",
    "TYPE",
    "DOCREF",
    "STYLE",
    "CLASSDEF",
    "CLASS",
    "ALPHA",
    "COLON",
    "SEMICOLON",
    "PERCENT",
    "MINUS",
    "BRKT",
    "END_ARROW_L",
    "END_ARROW_R",
    "LINE",
    "qString",
    "unqString",
    "COMMA",
];

// Every keyword is matched without regard to case and must end at a word boundary.
pub fn keyword_token(lower: &str) -> Option<i32> {
    match lower {
        "requirementdiagram" => Some(t::RD),
        "id" => Some(t::ID),
        "text" => Some(t::TEXT),
        "risk" => Some(t::RISK),
        "verifymethod" => Some(t::VERIFYMTHD),
        "requirement" => Some(t::REQUIREMENT),
        "functionalrequirement" => Some(t::FUNCTIONAL_REQUIREMENT),
        "interfacerequirement" => Some(t::INTERFACE_REQUIREMENT),
        "performancerequirement" => Some(t::PERFORMANCE_REQUIREMENT),
        "physicalrequirement" => Some(t::PHYSICAL_REQUIREMENT),
        "designconstraint" => Some(t::DESIGN_CONSTRAINT),
        "low" => Some(t::LOW_RISK),
        "medium" => Some(t::MED_RISK),
        "high" => Some(t::HIGH_RISK),
        "analysis" => Some(t::VERIFY_ANALYSIS),
        "demonstration" => Some(t::VERIFY_DEMONSTRATION),
        "inspection" => Some(t::VERIFY_INSPECTION),
        "test" => Some(t::VERIFY_TEST),
        "element" => Some(t::ELEMENT),
        "contains" => Some(t::CONTAINS),
        "copies" => Some(t::COPIES),
        "derives" => Some(t::DERIVES),
        "satisfies" => Some(t::SATISFIES),
        "verifies" => Some(t::VERIFIES),
        "refines" => Some(t::REFINES),
        "traces" => Some(t::TRACES),
        "type" => Some(t::TYPE),
        "docref" => Some(t::DOCREF),
        "style" => Some(t::STYLE),
        "classdef" => Some(t::CLASSDEF),
        "class" => Some(t::CLASS),
        _ => None,
    }
}
