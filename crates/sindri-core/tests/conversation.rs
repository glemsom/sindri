//! What a conversation promises: the prompt of each run that ended beside the answer it got, in the
//! order they ended, and nothing at all once the reader starts over.

use sindri_core::conversation::Conversation;
use sindri_core::provider::Usage;
use sindri_core::run::Exchange;

fn ended(prompt: &str, text: &str) -> Exchange {
    Exchange {
        turn: 1,
        prompt: String::from(prompt),
        text: String::from(text),
        calls: Vec::new(),
        usage: Usage::default(),
    }
}

fn replayed(conversation: &Conversation) -> Vec<(&str, &str)> {
    conversation
        .kept()
        .iter()
        .map(|kept| (kept.prompt.as_str(), kept.text.as_str()))
        .collect()
}

#[test]
fn a_conversation_that_has_held_no_prompt_hands_the_run_that_joins_it_nothing() {
    let conversation = Conversation::new();

    assert!(conversation.is_empty());
    assert_eq!(
        replayed(&conversation),
        [],
        "a launch's first prompt is asked with nothing before it"
    );
}

#[test]
fn a_prompt_joins_beside_the_answer_it_got() {
    let mut conversation = Conversation::new();

    conversation.join(ended("hi my name is Glenn", "nice to meet you, Glenn"));

    assert_eq!(
        replayed(&conversation),
        [("hi my name is Glenn", "nice to meet you, Glenn")],
        "the words asked and the words they got, whole"
    );
    assert_eq!(conversation.len(), 1, "one prompt is one exchange");
}

#[test]
fn the_prompts_join_in_the_order_they_ended() {
    let mut conversation = Conversation::new();
    for (prompt, answer) in [
        ("hi my name is Glenn", "nice to meet you, Glenn"),
        ("is it raining?", "it is raining"),
        ("what is my name?", "Glenn"),
    ] {
        conversation.join(ended(prompt, answer));
    }

    assert_eq!(
        replayed(&conversation),
        [
            ("hi my name is Glenn", "nice to meet you, Glenn"),
            ("is it raining?", "it is raining"),
            ("what is my name?", "Glenn"),
        ],
        "the newest prompt is last, which is where the wire asks for it"
    );
}

#[test]
fn a_prompt_that_got_no_answer_joins_whole_as_the_words_it_was_asked_with() {
    let mut conversation = Conversation::new();

    conversation.join(ended("read the failing test", ""));

    assert_eq!(
        replayed(&conversation),
        [("read the failing test", "")],
        "a run that answered nothing is not dropped: the prompt that came before it is still what a \
         reader asked"
    );
}

#[test]
fn starting_over_empties_what_the_run_that_joins_next_is_asked_with() {
    let mut conversation = Conversation::new();
    conversation.join(ended("hi my name is Glenn", "nice to meet you, Glenn"));

    conversation.clear();

    assert!(
        conversation.is_empty(),
        "`/new` starts the conversation again"
    );
    assert_eq!(
        replayed(&conversation),
        [],
        "so the prompt after it carries nothing from before it"
    );
}
