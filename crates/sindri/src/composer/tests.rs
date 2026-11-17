use super::*;

fn holding(text: &str) -> Composer {
    let mut composer = Composer::new();
    composer.paste(text);
    composer
}

fn numbered(lines: usize) -> Composer {
    let draft: Vec<String> = (1..=lines).map(|line| line.to_string()).collect();
    holding(&draft.join("\n"))
}

fn sent(outcome: Outcome) -> String {
    match outcome {
        Outcome::Send(prompt) => prompt.text().to_owned(),
        other => panic!("the draft should have been sent, and {other:?} came instead"),
    }
}

#[test]
fn a_draft_rewritten_is_the_whole_text_with_the_caret_after_its_last_glyph() {
    let mut composer = holding("half a thought, and the rest of it");
    composer.key(Key::Left);
    composer.rewrite("/settings");

    assert_eq!(
        composer.text(),
        "/settings",
        "a rewrite gives up the reader's own words for the text it is handed"
    );
    assert_eq!(
        composer.cursor(),
        "/settings".len(),
        "and leaves the caret after the last glyph, so the next key types on from there"
    );
    assert_eq!(
        composer.key(Key::Char('x')),
        Outcome::Edited,
        "which is a key the draft takes"
    );
    assert_eq!(composer.text(), "/settingsx");
}

#[test]
fn typing_extends_the_draft_at_the_cursor() {
    let mut composer = Composer::new();
    for glyph in "hello".chars() {
        assert_eq!(composer.key(Key::Char(glyph)), Outcome::Edited);
    }

    assert_eq!(composer.text(), "hello");
    assert_eq!(composer.cursor(), 5);
}

#[test]
fn moving_between_the_lines_of_a_draft_keeps_the_caret_in_the_cells_it_was_in() {
    let mut composer = holding("日x\nab");
    assert_eq!(
        composer.caret(),
        (1, 2),
        "the second line is two cells wide"
    );

    composer.key(Key::Up);

    assert_eq!(
        composer.caret(),
        (0, 2),
        "the caret comes back to the cell it left from, and never past the last glyph of \
             the line it lands in"
    );
}

#[test]
fn typing_between_two_glyphs_leaves_them_apart() {
    let mut composer = holding("ac");
    composer.key(Key::Left);
    composer.key(Key::Char('b'));

    assert_eq!(composer.text(), "abc");
}

#[test]
fn enter_sends_the_draft_and_leaves_the_box_empty() {
    let mut composer = holding("why did the retry test fail");

    assert_eq!(
        sent(composer.key(Key::Enter)),
        "why did the retry test fail",
        "the draft left the box as a prompt"
    );
    assert_eq!(composer.text(), "", "and the box is empty again");
    assert_eq!(
        composer.rows(),
        MIN_ROWS,
        "so the band is back at its floor"
    );
}

#[test]
fn a_box_with_nothing_in_it_has_nothing_to_send() {
    let mut composer = Composer::new();
    assert_eq!(composer.key(Key::Enter), Outcome::Unchanged);

    let mut spaces = holding("   ");
    assert_eq!(
        spaces.key(Key::Enter),
        Outcome::Unchanged,
        "spaces are not a prompt"
    );
    assert_eq!(spaces.text(), "   ", "and the box keeps them");
}

#[test]
fn shift_enter_and_ctrl_j_break_the_line_rather_than_sending() {
    let mut composer = holding("first");

    assert_eq!(composer.key(Key::Newline), Outcome::Edited);
    composer.key(Key::Char('s'));
    assert_eq!(composer.key(Key::Newline), Outcome::Edited);
    composer.key(Key::Char('t'));

    assert_eq!(composer.text(), "first\ns\nt");
    assert_eq!(composer.cursor(), 9);
}

#[test]
fn backspace_removes_the_glyph_before_the_cursor() {
    let mut composer = holding("ab");

    assert_eq!(composer.key(Key::Backspace), Outcome::Edited);
    assert_eq!(composer.text(), "a");
    assert_eq!(composer.key(Key::Backspace), Outcome::Edited);
    assert_eq!(composer.text(), "");
    assert_eq!(
        composer.key(Key::Backspace),
        Outcome::Unchanged,
        "an empty box has nothing left to take back"
    );
}

#[test]
fn backspace_eats_a_whole_glyph_however_many_bytes_it_took() {
    let mut composer = holding("✓ ok");

    for _ in 0..3 {
        composer.key(Key::Backspace);
    }
    assert_eq!(
        composer.text(),
        "✓",
        "the tick is still there, not taken half at a time"
    );

    composer.key(Key::Backspace);
    assert_eq!(composer.text(), "", "and one more takes it whole");
    assert_eq!(composer.cursor(), 0);
}

#[test]
fn an_empty_draft_takes_the_bands_five_row_floor() {
    assert_eq!(Composer::new().rows(), 5, "the shortest band is five rows");
    assert_eq!(
        Composer::new().visible_rows(),
        1,
        "one row of draft between the air under the edge and the air above the foot"
    );
    assert_eq!(
        holding("one line").rows(),
        MIN_ROWS,
        "and one line is still one row of the box"
    );
}

#[test]
fn a_line_per_row_grows_the_box_to_its_ceiling_and_no_further() {
    for lines in 1..=MAX_ROWS - EDGE_ROWS - FOOT_ROWS {
        assert_eq!(
            numbered(usize::from(lines)).rows(),
            lines + EDGE_ROWS + FOOT_ROWS,
            "{lines} lines take the band's two rows of air and one row each"
        );
    }
    assert_eq!(
        numbered(5).rows(),
        9,
        "five rows of draft are the band's nine-row ceiling"
    );
    assert_eq!(
        numbered(5).kept_back(),
        None,
        "a draft that fills the window exactly keeps no row back"
    );
    assert_eq!(
        numbered(6).rows(),
        MAX_ROWS,
        "the sixth line scrolls, and the band keeps its ceiling"
    );
    assert_eq!(
        numbered(6).kept_back(),
        Some(2),
        "with the row under it given to the count, so the window is four rows and the caret \
             inside it"
    );
    assert_eq!(numbered(9).rows(), MAX_ROWS);
}

#[test]
fn the_bands_ceiling_leaves_five_rows_for_the_draft_and_no_more() {
    assert_eq!(
        numbered(5).visible_rows(),
        5,
        "a band at its ceiling is its two rows of air and five rows of draft"
    );
    assert_eq!(
        numbered(9).visible_rows(),
        4,
        "and a draft past it gives one of those rows to the row that counts the rest"
    );
    assert_eq!(
        numbered(4).visible_rows(),
        4,
        "a draft under the ceiling is drawn whole, with no row kept back"
    );
}

#[test]
fn a_draft_past_the_ceiling_scrolls_and_counts_the_rows_it_keeps_back() {
    let composer = numbered(9);

    assert_eq!(
        composer.scroll(),
        5,
        "nine lines in a window of four: the window starts at the sixth"
    );
    assert_eq!(composer.lines(), vec!["6", "7", "8", "9"]);
    assert_eq!(
        composer.kept_back(),
        Some(5),
        "and the first five lines are the rows the band counts back"
    );
    assert_eq!(
        composer.caret(),
        (3, 1),
        "the cursor is on the last row the window draws, inside the band"
    );
}

#[test]
fn a_caret_in_the_middle_of_a_long_draft_keeps_its_row_and_counts_the_rows_outside_it() {
    let mut composer = numbered(9);
    for _ in 0..4 {
        composer.key(Key::Up);
    }

    assert_eq!(
        composer.lines(),
        vec!["5", "6", "7", "8"],
        "four rows up from the foot, the window starts at the fifth line"
    );
    assert_eq!(
        composer.kept_back(),
        Some(5),
        "and counts the rows it draws on neither side of the window: four above it and the \
             last one under it"
    );
    assert_eq!(
        composer.caret(),
        (0, 1),
        "the caret waits on the window's first row, and the count row stays under the window"
    );
}

#[test]
fn the_window_follows_the_cursor_back_up_the_draft() {
    let mut composer = numbered(9);

    for _ in 0..4 {
        assert_eq!(composer.key(Key::Up), Outcome::Edited);
    }
    assert_eq!(
        composer.caret(),
        (0, 1),
        "four lines up is the window's first row"
    );
    assert_eq!(composer.scroll(), 4, "and nothing has scrolled yet");

    composer.key(Key::Up);
    assert_eq!(
        composer.scroll(),
        3,
        "one more line up, and the window follows"
    );
    assert_eq!(composer.caret(), (0, 1));
}

#[test]
fn the_first_line_is_where_the_queue_may_take_the_up_arrow() {
    let mut composer = holding("one\ntwo");

    assert!(!composer.on_first_line());
    composer.key(Key::Up);
    assert!(composer.on_first_line());
    assert_eq!(
        composer.key(Key::Up),
        Outcome::Unchanged,
        "there is no line above the first to move to"
    );
}

#[test]
fn a_recalled_prompt_is_the_draft_again_line_for_line() {
    let mut composer = Composer::new();
    composer.recall(Prompt {
        text: String::from("the first line\nthe second"),
    });

    assert_eq!(
        composer.text(),
        "the first line\nthe second",
        "the prompt came back whole, its line break included"
    );
    assert_eq!(composer.caret(), (1, 10), "and the caret follows it");
}

#[test]
fn a_prompt_recalled_into_a_draft_leaves_what_was_typed_there() {
    let mut composer = holding("fix: ");
    composer.recall(Prompt {
        text: String::from("the retry"),
    });

    assert_eq!(
        composer.text(),
        "fix: the retry",
        "a recall puts the prompt where the caret was and takes nothing away"
    );
}

#[test]
fn the_cursor_changes_line_at_the_column_the_line_allows() {
    let mut composer = holding("long line\nx");

    composer.key(Key::Up);
    assert_eq!(
        composer.cursor(),
        1,
        "one glyph in, which is the column it came from"
    );
    composer.key(Key::End);
    assert_eq!(composer.cursor(), 9);
    composer.key(Key::Down);
    assert_eq!(
        composer.cursor(),
        11,
        "the second line has no column nine, so the caret stops at its end"
    );
    composer.key(Key::Up);
    assert_eq!(
        composer.cursor(),
        1,
        "and comes back up from the column the short line left the caret in"
    );
    composer.key(Key::Home);
    assert_eq!(
        composer.cursor(),
        0,
        "column zero is the first line's start"
    );
}

#[test]
fn a_draft_wider_than_the_room_wraps_where_its_words_end() {
    let mut composer = Composer::new();
    composer.set_room(10);
    composer.paste("one two three four");

    assert_eq!(
        composer.lines(),
        vec!["one two ", "three four"],
        "ten columns hold neither `three` nor the blank before it on the first row, so the \
             draft wraps on the blank rather than inside a word"
    );
    assert_eq!(
        composer.rows(),
        6,
        "two rows of draft take the edge, the air under it, the air above the foot and the foot"
    );
}

#[test]
fn the_caret_wraps_onto_the_row_below_it_when_the_room_runs_out() {
    let mut composer = Composer::new();
    composer.set_room(10);
    composer.paste("one two three four");

    assert_eq!(
        composer.caret(),
        (1, 10),
        "the draft's last glyph is ten cells into the row the wrap gave it"
    );
    for _ in 0..3 {
        composer.key(Key::Left);
    }
    assert_eq!(
        composer.caret(),
        (1, 7),
        "and the caret walks back along that row"
    );
    composer.key(Key::Home);
    composer.key(Key::Up);
    composer.key(Key::End);
    assert_eq!(
        composer.caret(),
        (0, 8),
        "a row of the draft ends where its words do, and not where the draft does"
    );
}

#[test]
fn an_arrow_moves_between_the_rows_the_wrap_gave_the_draft() {
    let mut composer = Composer::new();
    composer.set_room(10);
    composer.paste("one two three four");

    assert_eq!(composer.key(Key::Up), Outcome::Edited);
    assert_eq!(
        composer.caret(),
        (0, 8),
        "the row above holds eight cells, so the caret stops at its end rather than \
             leaving the draft"
    );
    assert_eq!(
        composer.key(Key::Down),
        Outcome::Edited,
        "and comes back down to the row the wrap gave the draft"
    );
    assert_eq!(
        composer.caret(),
        (1, 8),
        "in the column it left from, not the one the long row ended in"
    );
}

#[test]
fn a_word_wider_than_the_room_is_broken_where_a_glyph_ends() {
    let mut composer = Composer::new();
    composer.set_room(5);
    composer.paste("日本語です");

    assert_eq!(
        composer.lines(),
        vec!["日本", "語で", "す"],
        "two wide glyphs fill five columns and the fifth is left blank, so no glyph is drawn \
             half on one row and half on the next"
    );
    assert_eq!(composer.rows(), 7);
}

#[test]
fn a_draft_taller_than_the_band_scrolls_by_the_rows_the_wrap_gave_it() {
    let mut composer = Composer::new();
    composer.set_room(4);
    composer.paste("abcdefghijklmnopqrstuvwxyz");

    assert_eq!(
        composer.rows(),
        MAX_ROWS,
        "seven rows of draft are drawn in the band's nine-row ceiling"
    );
    assert_eq!(
        composer.scroll(),
        3,
        "and the row under the window counts three back, so the window starts at the fourth"
    );
    assert_eq!(
        composer.lines(),
        vec!["mnop", "qrst", "uvwx", "yz"],
        "which are the rows the caret's own is the last of"
    );
    assert_eq!(composer.kept_back(), Some(3));
    assert_eq!(
        composer.caret(),
        (3, 2),
        "the caret waits on the window's last row and not under it"
    );
}

#[test]
fn a_pasted_prompt_is_one_draft_with_its_line_breaks_kept() {
    let mut composer = holding("fix this: ");

    composer.paste("the first line\r\nthe second\rand the third\n");

    assert_eq!(
        composer.text(),
        "fix this: the first line\nthe second\nand the third\n",
        "every break the clipboard carried is still a break"
    );
    assert_eq!(
        composer.lines(),
        vec![
            "fix this: the first line",
            "the second",
            "and the third",
            ""
        ],
        "one draft of four lines, not four prompts"
    );
    assert_eq!(composer.rows(), 8);
}

#[test]
fn a_pasted_prompt_goes_in_at_the_cursor() {
    let mut composer = holding("two");
    composer.key(Key::Home);

    composer.paste("one\rthree\r");

    assert_eq!(composer.text(), "one\nthree\ntwo");
}

#[test]
fn ctrl_c_during_a_run_keeps_the_draft() {
    let mut composer = holding("half a prompt");
    composer.set_state(State::Running);

    assert_eq!(composer.key(Key::Cancel), Outcome::Cancel);
    assert_eq!(
        composer.text(),
        "half a prompt",
        "the draft is still the reader's"
    );
    assert_eq!(composer.cursor(), 13);
    assert_eq!(
        composer.state(),
        State::Running,
        "and the run is still the run"
    );
}

#[test]
fn ctrl_c_when_idle_clears_the_draft() {
    let mut composer = holding("half a prompt");

    assert_eq!(composer.key(Key::Cancel), Outcome::Edited);
    assert_eq!(composer.text(), "");
    assert_eq!(composer.cursor(), 0);
    assert_eq!(composer.rows(), MIN_ROWS);
    assert_eq!(
        composer.key(Key::Cancel),
        Outcome::Unchanged,
        "an empty box has nothing left to clear"
    );
}

#[test]
fn ctrl_q_asks_for_the_launch_to_end_and_leaves_the_draft_alone() {
    let mut composer = holding("a draft nobody sent");

    assert_eq!(composer.key(Key::Quit), Outcome::Quit);
    assert_eq!(composer.text(), "a draft nobody sent");
}
