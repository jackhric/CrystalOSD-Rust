#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RtcTime {
    pub year: i32,
    pub month: i32,
    pub day: i32,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColdInputs {
    pub pal: bool,
    pub language: i32,
    pub city: i32,
    pub aspect: i32,
    pub time_zone: i32,
    pub summer_time: bool,
    pub date_format: i32,
    pub time_format: i32,
    pub rtc: RtcTime,
    pub clock_offset: Option<i64>,
    pub rand_state: u32,
    pub screen_width: u32,
    pub screen_height: u32,
    pub wide: bool,
    pub screen_code: u32,
    pub field: u32,
    pub gs_allocator: u32,
}

impl Default for ColdInputs {
    fn default() -> Self {
        Self {
            pal: false,
            language: 0,
            city: 0x33,
            aspect: 0,
            time_zone: 0,
            summer_time: false,
            date_format: 0,
            time_format: 0,
            rtc: RtcTime::default(),
            clock_offset: None,
            rand_state: 1,
            screen_width: 640,
            screen_height: 224,
            wide: false,
            screen_code: 0,
            field: 0,
            gs_allocator: 0,
        }
    }
}
