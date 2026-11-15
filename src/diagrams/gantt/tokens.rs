pub mod t {
    pub const END: usize = 0;
    pub const ERROR: usize = 1;
    pub const OPEN_DIRECTIVE: usize = 2;
    pub const ACC_TITLE: usize = 3;
    pub const ACC_TITLE_VALUE: usize = 4;
    pub const ACC_DESCR: usize = 5;
    pub const ACC_DESCR_VALUE: usize = 6;
    pub const ACC_DESCR_MULTILINE_VALUE: usize = 7;
    pub const NL: usize = 8;
    pub const HREF: usize = 9;
    pub const CALLBACKNAME: usize = 10;
    pub const CALLBACKARGS: usize = 11;
    pub const CLICK: usize = 12;
    pub const GANTT: usize = 13;
    pub const DATE_FORMAT: usize = 14;
    pub const INCLUSIVE_END_DATES: usize = 15;
    pub const TOP_AXIS: usize = 16;
    pub const AXIS_FORMAT: usize = 17;
    pub const TICK_INTERVAL: usize = 18;
    pub const INCLUDES: usize = 19;
    pub const EXCLUDES: usize = 20;
    pub const TODAY_MARKER: usize = 21;
    pub const WEEKDAY_MONDAY: usize = 22;
    pub const WEEKDAY_TUESDAY: usize = 23;
    pub const WEEKDAY_WEDNESDAY: usize = 24;
    pub const WEEKDAY_THURSDAY: usize = 25;
    pub const WEEKDAY_FRIDAY: usize = 26;
    pub const WEEKDAY_SATURDAY: usize = 27;
    pub const WEEKDAY_SUNDAY: usize = 28;
    pub const WEEKEND_FRIDAY: usize = 29;
    pub const WEEKEND_SATURDAY: usize = 30;
    pub const DATE: usize = 31;
    pub const TITLE: usize = 32;
    pub const ACC_DESCRIPTION: usize = 33;
    pub const SECTION: usize = 34;
    pub const TASK_TXT: usize = 35;
    pub const TASK_DATA: usize = 36;
    pub const COLON: usize = 37;
    pub const EOF: usize = 38;
}

pub const TOKEN_NAMES: [&str; 39] = [
    "$end",
    "INVALID",
    "open_directive",
    "acc_title",
    "acc_title_value",
    "acc_descr",
    "acc_descr_value",
    "acc_descr_multiline_value",
    "NL",
    "href",
    "callbackname",
    "callbackargs",
    "click",
    "gantt",
    "dateFormat",
    "inclusiveEndDates",
    "topAxis",
    "axisFormat",
    "tickInterval",
    "includes",
    "excludes",
    "todayMarker",
    "weekday_monday",
    "weekday_tuesday",
    "weekday_wednesday",
    "weekday_thursday",
    "weekday_friday",
    "weekday_saturday",
    "weekday_sunday",
    "weekend_friday",
    "weekend_saturday",
    "date",
    "title",
    "accDescription",
    "section",
    "taskTxt",
    "taskData",
    ":",
    "EOF",
];
