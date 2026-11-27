mod caps;
mod fixture;
mod frame;

#[test]
fn boot_120x40_is_the_frame_a_launch_opens_on() {
    fixture::assert_frame("boot", 120, 40);
}

#[test]
fn boot_80x24_is_every_line_of_the_splash_screen_and_no_rail_beside_it() {
    fixture::assert_frame("boot", 80, 24);
}

#[test]
fn boot_face_120x40_is_the_block_beside_the_rails_one_face() {
    fixture::assert_frame("boot-face", 120, 40);
}

#[test]
fn boot_face_small_120x14_is_the_splash_screens_block_beside_a_rail_whose_face_takes_every_row() {
    fixture::assert_frame("boot-face-small", 120, 14);
}

#[test]
fn prompt_flat_120x40_is_a_prompt_and_the_draft_beside_it_on_a_terminal_that_spells_no_colour() {
    fixture::assert_frame("prompt-flat", 120, 40);
}

#[test]
fn transcript_120x40_is_a_run_laid_out_at_the_tail() {
    fixture::assert_frame("transcript", 120, 40);
}

#[test]
fn gutter_120x40_is_one_of_every_kind_of_entry_under_one_column() {
    fixture::assert_frame("gutter", 120, 40);
}

#[test]
fn gutter_80x24_is_that_transcript_on_the_narrower_band() {
    fixture::assert_frame("gutter", 80, 24);
}

#[test]
fn transcript_scrolled_120x40_cuts_the_top_entry_mid_row() {
    fixture::assert_frame("transcript-scrolled", 120, 40);
}

#[test]
fn transcript_scrolled_80x24_cuts_the_top_entry_and_says_how_many_rows_back_it_stands() {
    fixture::assert_frame("transcript-scrolled", 80, 24);
}

#[test]
fn thumb_head_120x40_is_the_thumb_opening_a_band_scrolled_to_the_first_row() {
    fixture::assert_frame("thumb-head", 120, 40);
}

#[test]
fn thumb_middle_120x40_is_the_thumb_half_way_down_a_band_scrolled_half_way_back() {
    fixture::assert_frame("thumb-middle", 120, 40);
}

#[test]
fn thumb_tail_120x40_is_the_band_with_no_thumb_because_the_reader_follows_the_run() {
    fixture::assert_frame("thumb-tail", 120, 40);
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
fn tools_120x40_is_one_call_of_each_tool_settled_and_in_flight() {
    fixture::assert_frame("tools", 120, 40);
}

#[test]
fn fold_120x40_is_a_settled_exchanges_work_drawn_as_one_row() {
    fixture::assert_frame("fold", 120, 40);
}

#[test]
fn fold_open_120x40_is_that_work_opened_and_the_fold_its_result_keeps() {
    fixture::assert_frame("fold-open", 120, 40);
}

#[test]
fn prompt_fold_120x40_is_a_prompt_longer_than_its_head_drawn_as_that_head_and_a_count() {
    fixture::assert_frame("prompt-fold", 120, 40);
}

#[test]
fn prompt_fold_open_120x40_is_that_prompt_opened_to_every_word_the_reader_typed() {
    fixture::assert_frame("prompt-fold-open", 120, 40);
}

#[test]
fn fold_answer_120x40_is_an_answer_drawn_whole_beside_a_folded_result() {
    fixture::assert_frame("fold-answer", 120, 40);
}

#[test]
fn mark_120x40_is_a_drag_over_the_transcript_it_covers() {
    fixture::assert_frame("mark", 120, 40);
}

#[test]
fn picture_120x40_is_a_field_drawn_to_the_band_beside_a_refused_sources_caption() {
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
fn stops_120x40_is_one_entry_of_every_stop_a_run_can_end_on() {
    fixture::assert_frame("stops", 120, 40);
}

#[test]
fn stop_120x40_is_the_marker_and_the_stop_entry_a_failed_run_leaves() {
    fixture::assert_frame("stop", 120, 40);
}

#[test]
fn refused_120x40_is_the_entry_a_run_that_could_not_start_leaves_beside_its_draft() {
    fixture::assert_frame("refused", 120, 40);
}

#[test]
fn queue_120x40_draws_the_prompts_typed_during_a_run_under_the_draft() {
    fixture::assert_frame("queue", 120, 40);
}

#[test]
fn queue_long_120x24_is_a_queue_taller_than_its_band_counting_the_rows_it_keeps_back() {
    fixture::assert_frame("queue-long", 120, 24);
}

#[test]
fn draft_long_120x24_is_a_draft_past_the_band_counting_the_rows_it_keeps_back() {
    fixture::assert_frame("draft-long", 120, 24);
}

#[test]
fn keys_120x40_lists_the_keys_of_the_stack_it_sits_on() {
    fixture::assert_frame("keys", 120, 40);
}

#[test]
fn card_120x40_is_a_card_as_tall_as_the_keys_it_lists_over_the_transcript_it_was_opened_from() {
    fixture::assert_frame("card", 120, 40);
}

#[test]
fn card_40x24_is_a_card_bled_to_the_frames_edges_that_counts_the_rows_it_keeps_back() {
    fixture::assert_frame("card", 40, 24);
}

#[test]
fn files_120x40_is_the_eight_files_an_empty_query_leaves_and_the_count_of_all_seventeen() {
    fixture::assert_frame("files", 120, 40);
}

#[test]
fn files_narrow_120x40_is_the_capped_list_of_a_query_ten_files_answer() {
    fixture::assert_frame("files-narrow", 120, 40);
}

#[test]
fn files_few_120x40_is_the_card_of_the_two_matches_the_query_leaves() {
    fixture::assert_frame("files-few", 120, 40);
}

#[test]
fn files_none_120x40_is_one_line_saying_no_file_answers() {
    fixture::assert_frame("files-none", 120, 40);
}

#[test]
fn models_120x40_is_a_card_as_tall_as_the_two_names_the_wire_served() {
    fixture::assert_frame("models", 120, 40);
}

#[test]
fn models_80x24_is_that_card_of_two_names_over_a_shorter_frame() {
    fixture::assert_frame("models", 80, 24);
}

#[test]
fn models_40x24_is_a_bled_card_as_tall_as_the_two_names_it_holds() {
    fixture::assert_frame("models", 40, 24);
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
fn stats_120x40_is_the_facts_the_rail_gives_up_and_the_rows_a_whole_frame_cannot_hold() {
    fixture::assert_frame("stats", 120, 40);
}

#[test]
fn stats_80x24_is_that_screen_on_a_card_that_counts_the_rows_it_keeps_back() {
    fixture::assert_frame("stats", 80, 24);
}

#[test]
fn empty_120x40_is_the_band_with_nothing_in_it_saying_what_it_can_do() {
    fixture::assert_frame("empty", 120, 40);
}

#[test]
fn wrapped_80x24_is_a_draft_wider_than_the_band_set_under_the_gutter() {
    fixture::assert_frame("wrapped", 80, 24);
}

#[test]
fn typing_120x40_is_a_composer_at_its_five_row_floor() {
    fixture::assert_frame("typing", 120, 40);
}

#[test]
fn edge_120x40_is_a_band_that_only_takes_the_draft() {
    fixture::assert_frame("edge", 120, 40);
}

#[test]
fn edge_live_120x40_is_that_band_with_a_run_live_in_it_and_a_head_on_its_edge() {
    fixture::assert_frame("edge-live", 120, 40);
}

#[test]
fn grown_120x40_is_a_composer_at_its_nine_row_ceiling() {
    fixture::assert_frame("grown", 120, 40);
}

#[test]
fn scrolled_120x40_is_a_composer_showing_the_last_four_rows_of_a_taller_draft() {
    fixture::assert_frame("scrolled", 120, 40);
}

#[test]
fn commands_120x40_is_the_card_a_slash_opens_with_every_command_and_the_line_that_explains_it() {
    fixture::assert_frame("commands", 120, 40);
}

#[test]
fn commands_narrow_120x40_is_that_card_narrowed_to_the_commands_the_word_holds() {
    fixture::assert_frame("commands-narrow", 120, 40);
}

#[test]
fn commands_none_120x40_is_a_draft_naming_no_command_and_no_card_over_it() {
    fixture::assert_frame("commands-none", 120, 40);
}

#[test]
fn markdown_headings_80x24_is_six_levels_from_a_ruled_bold_to_a_dim_bold() {
    fixture::assert_frame("markdown-headings", 80, 24);
}

#[test]
fn markdown_headings_40x24_is_the_same_six_levels_wrapped_at_the_band() {
    fixture::assert_frame("markdown-headings", 40, 24);
}

#[test]
fn markdown_headings_large_80x24_is_the_two_highest_levels_at_twice_the_cell() {
    fixture::assert_frame("markdown-headings-large", 80, 24);
}

#[test]
fn markdown_headings_large_40x24_is_those_levels_wrapped_and_measured_by_what_they_leave() {
    fixture::assert_frame("markdown-headings-large", 40, 24);
}

#[test]
fn markdown_lists_80x24_is_a_bullet_and_an_ordinal_two_spaces_per_depth() {
    fixture::assert_frame("markdown-lists", 80, 24);
}

#[test]
fn markdown_tasks_80x24_is_boxes_in_dim_and_ticks_in_ok_beside_an_ordinary_bullet() {
    fixture::assert_frame("markdown-tasks", 80, 24);
}

#[test]
fn markdown_table_80x24_is_a_bold_header_ruled_in_dim_and_never_bordered() {
    fixture::assert_frame("markdown-table", 80, 24);
}

#[test]
fn markdown_quote_80x24_is_a_heading_and_prose_behind_one_dim_bar() {
    fixture::assert_frame("markdown-quote", 80, 24);
}

#[test]
fn markdown_alert_80x24_is_the_five_kinds_each_named_in_an_ink_of_its_own() {
    fixture::assert_frame("markdown-alert", 80, 24);
}

#[test]
fn markdown_alert_marker_80x24_is_two_spellings_of_a_kind_a_marker_naming_none_and_one_in_an_item()
{
    fixture::assert_frame("markdown-alert-marker", 80, 24);
}

#[test]
fn markdown_code_80x24_is_one_block_whose_words_are_told_apart_in_their_inks() {
    fixture::assert_frame("markdown-code", 80, 24);
}

#[test]
fn markdown_fence_nested_80x24_is_one_block_whose_depth_divides_its_leading_whitespace() {
    fixture::assert_frame("markdown-fence-nested", 80, 24);
}

#[test]
fn markdown_fence_unknown_80x24_is_a_tag_and_plain_words_no_definition_names() {
    fixture::assert_frame("markdown-fence-unknown", 80, 24);
}

#[test]
fn markdown_diff_80x24_is_a_diff_whose_every_row_is_tinted_by_what_it_holds() {
    fixture::assert_frame("markdown-diff", 80, 24);
}

#[test]
fn markdown_diff_quoted_80x24_is_the_same_diff_behind_a_quote_bar() {
    fixture::assert_frame("markdown-diff-quoted", 80, 24);
}

#[test]
fn markdown_inline_80x24_is_a_chip_of_accent_ink_in_a_paragraph() {
    fixture::assert_frame("markdown-inline", 80, 24);
}

#[test]
fn markdown_emphasis_80x24_is_bold_italic_both_and_both_inside_a_heading() {
    fixture::assert_frame("markdown-emphasis", 80, 24);
}

#[test]
fn markdown_struck_80x24_is_a_struck_run_in_prose_in_a_heading_and_in_a_quote() {
    fixture::assert_frame("markdown-struck", 80, 24);
}

#[test]
fn markdown_link_80x24_is_a_link_drawn_as_its_words_and_a_path_the_run_judged_openable() {
    fixture::assert_frame("markdown-link", 80, 24);
}

#[test]
fn markdown_table_aligned_80x24_is_every_column_drawn_on_the_edge_it_was_written_to() {
    fixture::assert_frame("markdown-table-aligned", 80, 24);
}

#[test]
fn markdown_table_unaligned_80x24_is_the_same_table_written_with_no_alignment() {
    fixture::assert_frame("markdown-table-unaligned", 80, 24);
}

#[test]
fn markdown_table_aligned_narrow_40x24_is_an_aligned_table_wrapping_on_its_columns_edges() {
    fixture::assert_frame("markdown-table-aligned-narrow", 40, 24);
}

#[test]
fn markdown_table_narrow_40x24_is_a_wide_table_wrapped_inside_its_cells() {
    fixture::assert_frame("markdown-table-narrow", 40, 24);
}

#[test]
fn markdown_rule_80x24_is_a_hairline_in_prose_a_quote_and_an_item() {
    fixture::assert_frame("markdown-rule", 80, 24);
}

#[test]
fn markdown_footnote_80x24_is_a_marker_in_prose_a_quote_and_a_cell_under_one_foot() {
    fixture::assert_frame("markdown-footnote", 80, 24);
}

#[test]
fn markdown_footnote_undefined_80x24_is_a_reference_drawn_as_written_and_no_unused_definition() {
    fixture::assert_frame("markdown-footnote-undefined", 80, 24);
}

#[test]
fn markdown_break_80x24_is_a_break_in_prose_an_item_and_a_table_cell() {
    fixture::assert_frame("markdown-break", 80, 24);
}

#[test]
fn rail_160x40_is_a_wide_frame_s_rail_at_its_ceiling_with_the_served_name_whole() {
    fixture::assert_frame("rail", 160, 40);
}

#[test]
fn rail_108x40_is_the_rail_grown_past_its_floor_and_under_its_ceiling() {
    fixture::assert_frame("rail", 108, 40);
}

#[test]
fn rail_120x40_is_the_rail_grown_into_a_wide_frame_with_the_served_name_whole() {
    fixture::assert_frame("rail", 120, 40);
}

#[test]
fn rail_100x40_is_the_narrowest_frame_that_holds_a_ledger_and_its_value_column_cuts() {
    fixture::assert_frame("rail", 100, 40);
}

#[test]
fn rail_99x24_is_the_first_frame_that_is_one_strip() {
    fixture::assert_frame("rail", 99, 24);
}

#[test]
fn rail_80x24_is_the_strip_holding_four_of_its_five_facts_and_no_count() {
    fixture::assert_frame("rail", 80, 24);
}

#[test]
fn rail_40x24_is_the_strip_down_to_the_size_and_the_state() {
    fixture::assert_frame("rail", 40, 24);
}

#[test]
fn footer_120x24_is_an_idle_draft_s_whole_set_of_six() {
    fixture::assert_frame("footer", 120, 24);
}

#[test]
fn footer_72x24_is_an_idle_frame_giving_up_the_newline() {
    fixture::assert_frame("footer", 72, 24);
}

#[test]
fn footer_52x24_is_an_idle_frame_giving_up_the_quit_and_the_newline() {
    fixture::assert_frame("footer", 52, 24);
}

#[test]
fn footer_40x24_is_the_three_hints_a_narrow_frame_keeps() {
    fixture::assert_frame("footer", 40, 24);
}

#[test]
fn footer_live_120x24_is_a_live_run_s_whole_set_of_six() {
    fixture::assert_frame("footer-live", 120, 24);
}

#[test]
fn footer_live_72x24_is_a_live_run_giving_up_the_newline() {
    fixture::assert_frame("footer-live", 72, 24);
}

#[test]
fn footer_live_52x24_is_a_live_run_giving_up_the_quit_and_the_newline() {
    fixture::assert_frame("footer-live", 52, 24);
}

#[test]
fn footer_live_40x24_is_cancelling_the_commands_and_the_files() {
    fixture::assert_frame("footer-live", 40, 24);
}

#[test]
fn footer_queue_120x24_is_three_waiting_beside_the_key_that_recalls_one() {
    fixture::assert_frame("footer-queue", 120, 24);
}

#[test]
fn footer_queue_72x24_is_a_queued_run_giving_up_the_newline() {
    fixture::assert_frame("footer-queue", 72, 24);
}

#[test]
fn footer_queue_52x24_is_the_count_beside_cancelling_the_commands_and_the_files() {
    fixture::assert_frame("footer-queue", 52, 24);
}

#[test]
fn footer_queue_40x24_is_the_count_and_the_commands_alone() {
    fixture::assert_frame("footer-queue", 40, 24);
}

#[test]
fn face_120x40_is_the_face_over_the_rails_top_rows() {
    fixture::assert_frame("face", 120, 40);
}

#[test]
fn face_off_120x40_is_the_facts_from_the_rails_first_row_and_no_band() {
    fixture::assert_frame("face-off", 120, 40);
}

#[test]
fn files_120x40_is_the_same_cells_where_the_pointer_is_taken_and_where_it_is_not() {
    fixture::assert_pointer_draws_the_same_cells("files", 120, 40);
}

#[test]
fn commands_120x40_is_the_same_cells_where_the_pointer_is_taken_and_where_it_is_not() {
    fixture::assert_pointer_draws_the_same_cells("commands", 120, 40);
}

#[test]
fn markdown_fence_cut_80x24_is_a_fence_line_the_band_cuts_and_marks() {
    fixture::assert_frame("markdown-fence-cut", 80, 24);
}

#[test]
fn markdown_fence_fits_80x24_is_a_fence_line_the_band_holds_whole() {
    fixture::assert_frame("markdown-fence-fits", 80, 24);
}

#[test]
fn markdown_fence_walked_80x24_is_the_line_the_band_cut_carried_eight_columns_on() {
    fixture::assert_frame("markdown-fence-walked", 80, 24);
}

#[test]
fn markdown_fence_named_80x24_is_a_fence_that_names_the_file_it_was_written_in() {
    fixture::assert_frame("markdown-fence-named", 80, 24);
}

#[test]
fn markdown_fence_named_cut_40x24_is_that_name_cut_at_the_bands_edge() {
    fixture::assert_frame("markdown-fence-named-cut", 40, 24);
}

#[test]
fn settings_80x24_pins_actions_below_the_scrollable_fields() {
    fixture::assert_frame("settings", 80, 24);
}

#[test]
fn settings_40x24_stacks_labels_and_values_in_a_borderless_card() {
    fixture::assert_frame("settings", 40, 24);
}

#[test]
fn settings_refused_40x14_keeps_the_refused_field_and_recovery_actions_visible() {
    fixture::assert_frame("settings-refused", 40, 14);
}

#[test]
fn settings_mono_80x24_keeps_section_names_and_selection_without_colour() {
    fixture::assert_frame("settings-mono", 80, 24);
}

#[test]
fn settled_exchange_tally_and_its_counted_calls_at_narrow_and_medium_widths() {
    for width in [40, 80] {
        fixture::assert_frame("fold", width, 24);
        fixture::assert_frame("fold-open", width, 24);
    }
}
