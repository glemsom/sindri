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
fn markdown_headings_80x24_is_a_heading_at_each_of_the_four_renderings() {
    fixture::assert_frame("markdown-headings", 80, 24);
}

#[test]
fn markdown_lists_80x24_is_a_bullet_and_an_ordinal_two_spaces_per_depth() {
    fixture::assert_frame("markdown-lists", 80, 24);
}

#[test]
fn markdown_table_80x24_is_columns_ruled_under_their_header_and_never_bordered() {
    fixture::assert_frame("markdown-table", 80, 24);
}

#[test]
fn markdown_quote_80x24_is_prose_set_behind_one_dim_bar() {
    fixture::assert_frame("markdown-quote", 80, 24);
}

#[test]
fn markdown_code_80x24_is_a_guttered_block_highlighted_and_one_unknown_language() {
    fixture::assert_frame("markdown-code", 80, 24);
}

#[test]
fn markdown_inline_80x24_is_a_run_of_code_set_reversed_in_a_paragraph() {
    fixture::assert_frame("markdown-inline", 80, 24);
}

#[test]
fn markdown_link_80x24_is_a_link_that_prints_the_url_its_words_hide() {
    fixture::assert_frame("markdown-link", 80, 24);
}

#[test]
fn markdown_table_narrow_40x24_is_a_wide_table_wrapped_inside_its_cells() {
    fixture::assert_frame("markdown-table-narrow", 40, 24);
}
