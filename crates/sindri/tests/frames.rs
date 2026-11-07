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
fn transcript_120x40_is_a_run_laid_out_at_the_tail() {
    fixture::assert_frame("transcript", 120, 40);
}

#[test]
fn transcript_scrolled_120x40_cuts_the_top_entry_mid_row() {
    fixture::assert_frame("transcript-scrolled", 120, 40);
}

#[test]
fn streaming_120x40_is_a_run_whose_answer_and_thought_are_still_landing() {
    fixture::assert_frame("streaming", 120, 40);
}

#[test]
fn calls_120x40_is_a_call_settled_four_ways() {
    fixture::assert_frame("calls", 120, 40);
}

#[test]
fn queue_120x40_counts_the_prompts_typed_during_a_run_above_the_composer() {
    fixture::assert_frame("queue", 120, 40);
}

#[test]
fn keys_120x40_lists_the_keys_of_the_stack_it_sits_on() {
    fixture::assert_frame("keys", 120, 40);
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

#[test]
fn rail_120x40_is_the_ledger_down_the_frames_right_hand_columns() {
    fixture::assert_frame("rail", 120, 40);
}

#[test]
fn rail_100x40_is_the_last_frame_that_holds_a_ledger() {
    fixture::assert_frame("rail", 100, 40);
}

#[test]
fn rail_99x24_is_the_first_frame_that_is_one_strip() {
    fixture::assert_frame("rail", 99, 24);
}

#[test]
fn rail_80x24_is_the_strip_giving_facts_up_in_a12s_order() {
    fixture::assert_frame("rail", 80, 24);
}

#[test]
fn rail_40x24_is_the_strip_down_to_the_fact_that_never_goes() {
    fixture::assert_frame("rail", 40, 24);
}
