//! The frames this front end paints, frozen: one test per fixture, each rendering
//! the same painting code the terminal is handed, at a fixed size, and holding it
//! to the picture checked in beside it (§12, ADR-0012).
//!
//! A test here is named after the fixture it holds, its dashes underscored and its
//! name first, so `-- boot_120x40` runs exactly one frame and `make frame-update
//! F=boot-120x40` blesses one deliberate change while every other frame is still
//! compared. The harness prints one `check: frame <frame>-<cols>x<rows> ... ok`
//! line per fixture, which is the line `make check`'s summary ends on.

mod fixture; // one frame rendered at a fixed size into the text a fixture holds

#[test]
fn boot_120x40_is_the_frame_a_launch_opens_on() {
    fixture::assert_frame("boot", 120, 40);
}

#[test]
fn typing_120x40_is_a_composer_at_its_two_row_floor() {
    fixture::assert_frame("typing", 120, 40);
}

#[test]
fn grown_120x40_is_a_composer_at_its_six_row_ceiling() {
    fixture::assert_frame("grown", 120, 40);
}

#[test]
fn scrolled_120x40_is_a_composer_showing_the_last_five_rows_of_a_taller_draft() {
    fixture::assert_frame("scrolled", 120, 40);
}
