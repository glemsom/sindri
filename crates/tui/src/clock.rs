//! The wall clock a turn header names: `14:02`, and the date a record was
//! started on, in the terminal's own zone. Two functions, because the frame
//! asks the clock nothing else.

/// The local time of this moment, `HH:MM`. A clock the system will not
/// convert is said as `--:--`, never as an hour that might be wrong.
pub fn now() -> String {
    // SAFETY: `time` fills the `time_t` it is given and reads nothing else;
    // the pointer is to a live local.
    let secs = unsafe { libc::time(std::ptr::null_mut()) };
    if secs == -1 {
        return "--:--".to_string();
    }
    match local(secs) {
        Some(tm) => format!("{:02}:{:02}", tm.tm_hour, tm.tm_min),
        None => "--:--".to_string(),
    }
}

/// The local moment `ms` since the epoch names, `YYYY-MM-DD HH:MM`: how a
/// record is dated in the picker. A clock the system will not convert is said
/// as `---- --:--`, never as a date that might be wrong.
pub fn at(ms: u64) -> String {
    let secs = libc::time_t::try_from(ms / 1_000).unwrap_or(libc::time_t::MAX);
    match local(secs) {
        Some(tm) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min
        ),
        None => "---- --:--".to_string(),
    }
}

/// One `time_t` in the terminal's own zone.
fn local(secs: libc::time_t) -> Option<libc::tm> {
    // SAFETY: `tm` is a plain C struct of integers, and all-zero is a valid
    // one for `localtime_r` to overwrite.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: `localtime_r` reads the `time_t` and writes the `tm` it is
    // given, both live locals here, and touches no other memory.
    let converted = unsafe { libc::localtime_r(&secs, &mut tm) };
    (!converted.is_null()).then_some(tm)
}
