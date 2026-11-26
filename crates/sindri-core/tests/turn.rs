use serde_json::json;
use sindri_core::conversation::Conversation;
use sindri_core::provider;
use sindri_core::records;
use sindri_core::run;
use sindri_core::turn::Turn;

#[test]
fn the_wire_the_run_and_a_conversation_speak_one_turn() {
    let turn = Turn::answered(1, "hi my name is Glenn", "nice to meet you, Glenn");

    let kept: run::Turn = turn.clone();
    let replayed: provider::Turn = kept.clone();
    let mut conversation = Conversation::new();
    conversation.join(replayed.clone());

    assert_eq!(
        kept, replayed,
        "the run's history element and the wire's replay form are the same value"
    );
    let joined: &[Turn] = conversation.turns();
    assert_eq!(
        joined,
        [turn],
        "and a conversation keeps the very turn a run ended on"
    );
}

#[test]
fn the_usage_a_turn_spends_is_the_one_total_a_record_keeps() {
    let spent = provider::Usage {
        input: Some(3),
        output: Some(4),
        total: Some(7),
        cached_input: Some(1),
        reasoning: Some(2),
        ..records::Usage::default()
    };
    let mut kept = records::Usage::default();
    kept.charge(&spent);

    assert_eq!(
        kept, spent,
        "a record sums the one usage a turn carries, not a second reading of it"
    );
    assert_eq!(
        serde_json::to_value(kept).expect("usage is JSON"),
        json!({"input": 3, "output": 4, "total": 7, "cached_input": 1, "reasoning": 2}),
        "the session document keeps the five counts under their own names"
    );
    assert_eq!(
        serde_json::to_value(records::Usage::default()).expect("usage is JSON"),
        json!({}),
        "a count nobody reported is absent, not a zero"
    );
}

#[test]
fn observed_request_and_compaction_metrics_survive_reporting_without_invented_counts() {
    let observed = json!({"cache_read":4,"cache_write":9,"request_ms":12,"client_compactions":1,"native_compactions":2});
    let usage: records::Usage = serde_json::from_value(observed.clone()).unwrap();
    let mut total = records::Usage::default();
    total.charge(&usage);
    assert_eq!(serde_json::to_value(total).unwrap(), observed);
}
