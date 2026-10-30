//! The wall clock a turn header names: `14:02`, in the terminal's own zone.
//! One function, because the frame asks the clock nothing else.

/// The local time of this moment, `HH:MM`. A clock the system will not
/// convert is said as `--:--`, never as an hour that might be wrong.
pub fn now() -> String {
    // SAFETY: `time` fills the `time_t` it is given and reads nothing else;
    // the pointer is to a live local.
    let secs = unsafe { libc::time(std::ptr::null_mut()) };
    if secs == -1 {
        return "--:--".to_string();
    }
    // SAFETY: `tm` is a plain C struct of integers, and all-zero is a valid
    // one for `localtime_r` to overwrite.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: `localtime_r` reads the `time_t` and writes the `tm` it is
    // given, both live locals here, and touches no other memory.
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return "--:--".to_string();
    }
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}
