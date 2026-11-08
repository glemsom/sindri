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
fn mark_120x40_is_a_drag_over_the_transcript_it_covers() {
    fixture::assert_frame("mark", 120, 40);
}

#[test]
fn picture_120x40_is_a_picture_block_its_words_its_gutter_and_a_refused_caption() {
    fixture::assert_frame("picture", 120, 40);
}

#[test]
fn pulse_live_120x40_is_the_marker_carrying_a_live_runs_elapsed_time() {
    fixture::assert_frame("pulse-live", 120, 40);
}

#[test]
fn pulse_settled_120x40_is_the_marker_a_stopped_run_settled_on() {
    fixture::assert_frame("pulse-settled", 120, 40);
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
fn files_120x40_is_the_eight_files_nearest_the_workspace() {
    fixture::assert_frame("files", 120, 40);
}

#[test]
fn models_120x40_is_the_names_the_wire_served_as_one_screen() {
    fixture::assert_frame("models", 120, 40);
}

#[test]
fn settings_120x40_is_the_keys_the_schema_has_and_the_value_each_holds() {
    fixture::assert_frame("settings", 120, 40);
}

#[test]
fn settings_refused_120x40_is_a_value_the_screen_will_not_take_naming_its_key() {
    fixture::assert_frame("settings-refused", 120, 40);
}

#[test]
fn stats_120x40_is_the_facts_the_rail_gives_up_drawn_whole() {
    fixture::assert_frame("stats", 120, 40);
}

#[test]
fn stats_80x24_is_that_screen_on_a_band_that_counts_the_rows_it_cannot_hold() {
    fixture::assert_frame("stats", 80, 24);
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
fn rail_80x24_is_the_strip_holding_its_four_facts_and_no_count() {
    fixture::assert_frame("rail", 80, 24);
}

#[test]
fn rail_40x24_is_the_strip_down_to_the_size_and_the_model() {
    fixture::assert_frame("rail", 40, 24);
}

#[test]
fn footer_130x24_is_seven_hints_with_columns_to_spare() {
    fixture::assert_frame("footer", 130, 24);
}

#[test]
fn footer_120x24_is_the_narrowest_frame_that_holds_seven_hints() {
    fixture::assert_frame("footer", 120, 24);
}

#[test]
fn footer_119x24_is_the_first_width_that_gives_the_seventh_hint_up() {
    fixture::assert_frame("footer", 119, 24);
}

#[test]
fn footer_100x24_is_the_narrowest_frame_that_holds_five_hints() {
    fixture::assert_frame("footer", 100, 24);
}

#[test]
fn footer_99x24_is_the_first_width_down_to_the_two_that_never_go() {
    fixture::assert_frame("footer", 99, 24);
}

#[test]
fn footer_60x24_is_the_floor_the_hints_never_fall_below() {
    fixture::assert_frame("footer", 60, 24);
}
