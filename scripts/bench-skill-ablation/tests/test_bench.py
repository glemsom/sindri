"""What the ablation bench asserts without a model: the record it writes, the
rule it scores with, and the table it prints.

Every test builds the lines it reads rather than borrowing another's, and every
name states the behaviour it proves — a bench whose own arithmetic is wrong
reports a number nobody can trust.
"""
import contextlib
import io
import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import bench  # noqa: E402
import tasks  # noqa: E402

# The skill the capability task ablates: the name every load in these tests has to
# carry to be this task's activation, and the one the table's sentence says.
SKILL = tasks.TASKS["refused-write"]["skill"]

# One tree per process, because a line's score is read out of the tree the run left
# behind: `tests/run.sh` exits 0, so every line built here is a tree the change
# held in.
_PASSING = pathlib.Path(tempfile.mkdtemp(prefix="bench-repo-"))
(_PASSING / "tests").mkdir()
(_PASSING / "tests/run.sh").write_text("#!/bin/sh\necho 'all tests pass'\n")


def call(index, command):
    """One `bash` call as the transcript writes it, with the turn it ran in."""
    return {"type": "tool_call", "turn": index, "call_id": f"c{index}",
            "tool": "bash", "command": command}


def result(index, exit_code, outcome="exited"):
    return {"type": "tool_result", "turn": index, "call_id": f"c{index}",
            "content": "", "is_error": False, "outcome": outcome,
            "exit_code": exit_code}


def skill_call(index, name, outcome="loaded"):
    """The one call that is not a command: a skill's body entering the run."""
    return [{"type": "tool_call", "turn": index, "call_id": f"s{index}",
             "tool": "skill", "name": name},
            {"type": "tool_result", "turn": index, "call_id": f"s{index}",
             "content": "<skill_content>", "is_error": False, "outcome": outcome}]


class TheRecordLineCarriesTheFourFieldsTheAblationTurnsOn(unittest.TestCase):
    def line(self, entries=None, **overrides):
        """One line, from a run whose suite passed after the change; the
        transcript it read is the argument, so an arm can be told from a
        transcript and never from its own label."""
        run = {"turns": 4, "tool_calls": 3, "stopped": "final",
               "usage": {"input_tokens": 1234, "output_tokens": 56}}
        if entries is None:
            entries = [call(1, "sed -n '1,20p' src/greet.sh")]
            entries += skill_call(1, SKILL)
            entries += [call(2, "sh tests/run.sh"), result(2, 0)]
        fields = dict(task="refused-write", arm="with", tag="a",
                      repetition=1, attempt=1, model="m", run=run,
                      entries=entries, repo=_PASSING, answer="", elapsed_s=9)
        fields.update(overrides)
        return bench.record(**fields)

    def test_the_summed_input_tokens_come_from_the_runs_own_usage(self):
        self.assertEqual(self.line()["in_tokens"], 1234)

    def test_a_run_that_reported_no_usage_carries_no_token_count(self):
        # Absent is not zero: a run that never reached the model is not a
        # measurement, and a null is how the median drops it.
        run = {"turns": 0, "tool_calls": 0, "stopped": "error", "rc": 1}
        line = bench.record(task="refused-write", arm="with", tag="a",
                            repetition=1,
                            attempt=1, model="m", run=run, entries=[],
                            repo=_PASSING, answer="", elapsed_s=1)
        self.assertIsNone(line["in_tokens"])

    def test_a_load_before_the_first_command_is_a_fact_of_the_record(self):
        # The arm the doctrine is moved by is scored on this and not on the
        # arm's own label: the transcript says whether a body entered the
        # conversation before anything was run.
        before = skill_call(1, SKILL) + [call(2, "sh tests/run.sh"),
                                              result(2, 0)]
        after = [call(2, "sh tests/run.sh"), result(2, 0)] + \
            skill_call(3, SKILL)
        self.assertTrue(self.line(entries=before)["loaded_before_bash"])
        self.assertFalse(self.line(entries=after)["loaded_before_bash"])

    def test_a_refused_second_call_is_not_a_body_loaded_before_the_command(self):
        # `already_loaded` is no body, so it is not the fact the third arm is
        # scored on, however early in the run it came.
        entries = skill_call(1, SKILL, outcome="already_loaded") + \
            [call(2, "sh tests/run.sh"), result(2, 0)]
        self.assertFalse(self.line(entries=entries)["loaded_before_bash"])

    def test_another_skills_body_is_not_this_arms_load(self):
        entries = skill_call(1, "house-style") + [call(2, "sh tests/run.sh"),
                                                 result(2, 0)]
        self.assertFalse(self.line(entries=entries)["loaded_before_bash"])

    def test_a_load_asked_in_the_same_wave_as_the_command_is_not_before_it(self):
        # A wave's calls are written before any of its results, and the body a
        # load carries arrives with them — so the command ran without it, and the
        # call before the command is not a load before the command.
        asked = {"type": "tool_call", "turn": 1, "call_id": "s1", "tool": "skill",
                 "name": SKILL}
        command = {"type": "tool_call", "turn": 1, "call_id": "c1", "tool": "bash",
                   "command": "sh tests/run.sh"}
        entries = [asked, command, skill_call(1, SKILL)[1], result(1, 2)]
        self.assertFalse(self.line(entries=entries)["loaded_before_bash"])

    def test_a_run_that_ran_no_command_has_no_first_command_to_be_before(self):
        # The fact is undefined rather than false: a run that loaded the body and
        # answered without running anything is not evidence that the sentence
        # failed to reach it, and counting it as one deflates the shipped arm.
        self.assertIsNone(self.line(entries=skill_call(1, SKILL))
                          ["loaded_before_bash"])

    def test_a_run_that_left_no_transcript_has_no_load_to_count(self):
        # Absent is not zero, as everywhere in this file: a run that never
        # reached the model wrote no entry to read, so it is left out of the
        # rate instead of being counted as a run the sentence failed to reach.
        run = {"turns": 0, "tool_calls": 0, "stopped": "error", "rc": 1}
        line = bench.record(task="refused-write", arm="with", tag="a",
                            repetition=1,
                            attempt=1, model="m", run=run, entries=[],
                            repo=_PASSING, answer="", elapsed_s=1)
        self.assertIsNone(line["loaded_before_bash"])

    def test_whether_the_skill_was_activated_is_a_field_of_its_own(self):
        without = [call(1, "sh tests/run.sh"), result(1, 0)]
        self.assertTrue(self.line()["activated"])
        self.assertFalse(self.line(entries=without, arm="without")["activated"])

    def test_the_arm_is_on_the_line_because_two_arms_share_one_file(self):
        self.assertEqual(self.line()["arm"], "with")

    def test_a_line_for_a_task_the_bench_does_not_hold_is_refused_by_name(self):
        # The names are `tasks.py`'s, and the refusal says which ones there are: a
        # table of a fixture nobody wrote is not a measurement of anything.
        for field, value in (("task", "sideways"), ("arm", "sideways")):
            with self.subTest(field=field):
                with self.assertRaises(SystemExit) as refused:
                    self.line(**{field: value})
                self.assertIn("sideways", str(refused.exception))
                self.assertIn("refused-write, inspect" if field == "task"
                              else "with, without or no-trigger",
                              str(refused.exception))

    def test_the_score_is_the_rule_and_the_rule_is_written_down(self):
        self.assertEqual(self.line()["checks"],
                         {"ok": True, "fixed": True, "proved": True})


class TheRightOrWrongRuleIsMechanical(unittest.TestCase):
    def test_a_suite_that_passes_after_the_change_is_fixed(self):
        with tempfile.TemporaryDirectory() as root:
            repo = pathlib.Path(root)
            (repo / "tests").mkdir()
            (repo / "tests/run.sh").write_text('#!/bin/sh\necho "all tests pass"\n')
            self.assertTrue(bench.fixed(repo))
            (repo / "tests/run.sh").write_text('#!/bin/sh\nexit 1\n')
            self.assertFalse(bench.fixed(repo))

    def test_a_tree_with_no_suite_at_all_is_not_fixed(self):
        with tempfile.TemporaryDirectory() as root:
            self.assertFalse(bench.fixed(pathlib.Path(root)))

    def test_the_last_suite_invocation_exiting_zero_is_the_proof(self):
        entries = [call(1, "sh tests/run.sh"), result(1, 2),
                   call(2, "sed -i s/Hello/Hello,/ src/greet.sh"),
                   call(3, "sh tests/run.sh"), result(3, 0)]
        self.assertTrue(bench.proved(entries))

    def test_a_suite_that_failed_last_is_not_proved_by_an_earlier_pass(self):
        entries = [call(1, "sh tests/run.sh"), result(1, 0),
                   call(2, "sh tests/run.sh"), result(2, 1)]
        self.assertFalse(bench.proved(entries))

    def test_a_run_that_never_ran_the_suite_proved_nothing(self):
        entries = [call(1, "sed -n '1,20p' src/greet.sh"), result(1, 0)]
        self.assertFalse(bench.proved(entries))

    def test_a_command_that_only_names_the_suite_is_not_a_run(self):
        # A mention exits 0 as easily as a run does, so the rule reads the shell
        # word at the position a shell would start a command in and nothing else.
        for mention in ("echo '# tests/run.sh is the suite'",
                        "echo sh tests/run.sh",
                        "rg -n 'sh tests/run.sh' .",
                        "cat tests/run.sh"):
            with self.subTest(mention=mention):
                self.assertFalse(bench.proved([call(1, mention), result(1, 0)]))

    def test_a_shell_word_in_command_position_is_the_run_it_looks_like(self):
        for command in ("sh tests/run.sh", "bash -e tests/run.sh",
                        "cd src && sh tests/run.sh 2>&1", "X=1 ./tests/run.sh",
                        "if sh tests/run.sh; then echo green; fi"):
            with self.subTest(command=command):
                self.assertTrue(bench.proved([call(1, command), result(1, 0)]))

    def test_right_is_the_change_holding_and_the_run_showing_it(self):
        held = [call(1, "sh tests/run.sh"), result(1, 0)]
        claimed = [call(1, "sed -i s/Hello/Hello,/ src/greet.sh"), result(1, 0)]
        self.assertTrue(bench.scored(entries=held, fixed=True)["ok"])
        self.assertFalse(bench.scored(entries=claimed, fixed=True)["ok"])
        self.assertFalse(bench.scored(entries=held, fixed=False)["ok"])


class AnUnusedBuiltInIsToldFromAUselessOne(unittest.TestCase):
    def test_activation_is_a_skill_call_whose_result_says_loaded(self):
        self.assertTrue(bench.activated(skill_call(1, SKILL), SKILL))

    def test_a_second_call_to_a_loaded_skill_is_not_a_second_activation(self):
        entries = skill_call(1, SKILL, outcome="already_loaded")
        self.assertFalse(bench.activated(entries, SKILL))

    def test_another_skills_body_is_not_this_ones_activation(self):
        self.assertFalse(bench.activated(skill_call(1, "house-style"), SKILL))

    def test_a_call_the_loop_refused_without_a_tool_counts_as_no_activation(self):
        entries = [{"type": "tool_call", "turn": 1, "call_id": "c1",
                    "tool": "refused", "named": "skill", "problem": "no such tool"}]
        self.assertFalse(bench.activated(entries, SKILL))


class TheTableIsTheMeansOfOneArmAgainstTheOther(unittest.TestCase):
    def file(self):
        with_right = [{"arm": "with", "in_tokens": 1000, "activated": True,
                       "loaded_before_bash": True, "checks": {"ok": True}},
                      {"arm": "with", "in_tokens": 1400, "activated": False,
                       "checks": {"ok": False}}]
        without = [{"arm": "without", "in_tokens": 300, "activated": False,
                    "checks": {"ok": False}},
                   {"arm": "without", "in_tokens": None, "activated": False,
                    "checks": {"ok": True}}]
        one_sweep = [dict(line, model="mock", task="refused-write")
                     for line in with_right + without]
        return one_sweep

    def test_each_arm_is_one_row_of_runs_tokens_activations_and_rights(self):
        table = bench.summarize(self.file())
        self.assertEqual(table["with"], {"runs": 2, "billed": 2, "mean": 1200,
                                         "median": 1200, "activated": 1,
                                         "loaded_first": 1, "commanded": 1,
                                         "right": 1})
        self.assertEqual(table["without"], {"runs": 2, "billed": 1, "mean": 300,
                                            "median": 300, "activated": 0,
                                            "loaded_first": 0, "commanded": 0,
                                            "right": 1})

    def test_an_arm_with_a_run_that_never_reached_the_model_says_so_beside_its_mean(self):
        # The mean is over one run and the row counts two: a reader who is not
        # told cannot see that the arm made a run the arithmetic never saw.
        table = bench.report(self.file())
        self.assertIn("`without` made 2 runs and 1 of them carried a token count", table)
        self.assertIn("the arms were billed for different numbers of runs "
                      "(with 2, without 1)", table)

    def test_two_sweeps_in_one_file_are_refused_rather_than_averaged(self):
        lines = self.file()
        lines.append({"arm": "with", "model": "another-model",
                      "task": "refused-write", "in_tokens": 1,
                      "checks": {"ok": True}})
        with self.assertRaises(SystemExit):
            bench.report(lines)

    def test_a_file_with_no_run_in_it_has_measured_nothing(self):
        self.assertEqual(bench.report([]), bench.NOT_MEASURED)

    def test_the_report_is_a_table_with_one_row_per_arm(self):
        table = bench.report(self.file())
        self.assertIn("| arm |", table)
        self.assertIn("| with |", table)
        self.assertIn("| without |", table)
        self.assertIn("1/2", table)


def sweep(with_=None, without=None):
    """Six repetitions of both arms, since the rule states its thresholds over
    six: `with` is a list of (in_tokens, activated, right) and `without` of
    (in_tokens, right). One model and one task, so the file is one sweep."""
    lines = []
    for tokens, activated, right in with_ or [(1000, True, True)] * 6:
        lines.append({"arm": "with", "model": "m", "task": "refused-write",
                      "in_tokens": tokens, "activated": activated,
                      "checks": {"ok": right}})
    for tokens, right in without or [(1000, False)] * 6:
        lines.append({"arm": "without", "model": "m", "task": "refused-write",
                      "in_tokens": tokens, "activated": False,
                      "checks": {"ok": right}})
    return lines


class TheShippingRuleIsAppliedToTheTable(unittest.TestCase):
    def test_a_built_in_reached_and_decisive_and_cheap_enough_earns_its_line(self):
        # The page states the three checks before any number arrives:
        # loaded in at least five of six, six of six right against at most four
        # of six, and no more than 1.25x the mean input tokens without it.
        report = bench.report(sweep(
            with_=[(1000, True, True)] * 6,
            without=[(1200, False)] * 6,
        ))
        self.assertIn("`sandbox-refusal` earns its catalog line.", report)

    def test_a_body_never_loaded_is_unused_and_the_description_is_what_to_fix(self):
        # The page's first state: the line is a tax no run is reached by, and
        # shipping the body again would not fix a catalog entry nobody opens.
        report = bench.report(sweep(
            with_=[(1000, False, True)] * 6,
            without=[(1200, False)] * 6,
        ))
        self.assertIn("unused", report)
        self.assertIn("rewrite the description", report)
        self.assertNotIn("earns its catalog line", report)

    def test_a_body_loaded_to_no_effect_is_useless_and_is_dropped(self):
        # The page's second state: the body changed the conversation and not the
        # outcome, so the run paid for it twice.
        report = bench.report(sweep(
            with_=[(1000, True, True)] * 6,
            without=[(1200, True)] * 6,
        ))
        self.assertIn("useless", report)
        self.assertIn("drop it", report)

    def test_a_gap_of_exactly_one_run_is_said_to_be_one_the_rule_cannot_express(self):
        # Five of six without the skill is the spread of six repetitions, so the
        # page says the rule is the thing to revise rather than a verdict.
        report = bench.report(sweep(
            with_=[(1000, True, True)] * 6,
            without=[(1200, True)] * 5 + [(1200, False)],
        ))
        self.assertIn("cannot express", report)
        self.assertNotIn("earns its catalog line", report)

    def test_a_body_that_costs_more_than_a_quarter_again_does_not_pay_for_itself(self):
        report = bench.report(sweep(
            with_=[(1300, True, True)] * 6,
            without=[(1000, False)] * 6,
        ))
        self.assertIn("1.30x", report)
        self.assertIn("drop it", report)

    def test_a_sweep_with_no_billed_run_has_no_price_to_read(self):
        report = bench.report(sweep(
            with_=[(None, True, True)] * 6,
            without=[(None, False)] * 6,
        ))
        self.assertIn("no mean without a billed run", report)
        self.assertNotIn("earns its catalog line", report)

    def test_a_file_holding_one_arm_has_no_comparison_to_make(self):
        report = bench.report(sweep()[:6])
        self.assertIn("needs both arms", report)


class TheThirdArmMovesTheSentenceAndTheTableSaysSo(unittest.TestCase):
    """The shipped arm, the shipped arm one sentence shorter, and the arm with no
    built-in at all: the third one is what makes the trigger sentence a number
    rather than an argument, so the report prints all three and names what the
    sentence came to."""

    def file(self, with_first=6, quiet_first=2):
        """Three arms of six runs over one model and one task. `with_first` and
        `quiet_first` are how many of each shipped-catalog arm's six runs loaded
        a skill before their first command; both arms were right every time and
        the arm with no skill was never right, so the shipping rule holds."""
        lines = []
        for arm, tokens, first, right in (("with", 1000, with_first, True),
                                          ("no-trigger", 1040, quiet_first, True),
                                          ("without", 1200, 0, False)):
            for index in range(6):
                lines.append({"arm": arm, "model": "m", "task": "refused-write",
                              "in_tokens": tokens, "activated": arm != "without",
                              "loaded_before_bash": index < first,
                              "checks": {"ok": right}})
        return lines

    def test_the_report_prints_one_row_per_arm_and_the_third_beside_the_two(self):
        table = bench.report(self.file())
        for arm in ("| with |", "| without |", "| no-trigger |"):
            self.assertIn(arm, table)

    def test_the_runs_whose_load_came_before_the_command_are_counted_per_arm(self):
        table = bench.summarize(self.file(with_first=6, quiet_first=2))
        self.assertEqual(table["with"]["loaded_first"], 6)
        self.assertEqual(table["no-trigger"]["loaded_first"], 2)
        self.assertEqual(table["without"]["loaded_first"], 0)
        self.assertEqual(table["no-trigger"]["commanded"], 6)

    def test_an_arm_whose_runs_measured_no_first_command_reads_no_rate(self):
        # The same discipline as a mean over no billed run: a rate over no
        # transcript is not a rate, and saying so is not the same as saying zero.
        lines = self.file()
        for line in lines:
            line["loaded_before_bash"] = None
        sentence = bench.wording(bench.summarize(lines))
        self.assertIn("has no rate to read", sentence)
        self.assertNotIn("earns its place", sentence)

    def test_the_column_says_before_bash_rather_than_activated_twice(self):
        # `activated` says a body entered the conversation and `loaded first`
        # says it entered before anything ran: an arm can be activated in every
        # run and still have reached none of them in time.
        table = bench.report(self.file(with_first=6, quiet_first=2))
        self.assertIn("| activated | loaded before bash | right |", table)

    def test_the_sentence_that_lifts_the_rate_is_named_as_earning_its_place(self):
        table = bench.report(self.file(with_first=6, quiet_first=2))
        self.assertIn("### The sentence", table)
        self.assertIn("earns its place", table)
        self.assertIn("6 of 6 runs with the trigger sentence and 2 of 6 without it",
                      table)

    def test_a_sentence_that_lifts_no_rate_is_said_to_move_nothing(self):
        self.assertIn("moves nothing", bench.report(self.file(quiet_first=6)))

    def test_a_sentence_that_lowers_the_rate_is_the_words_to_rewrite(self):
        self.assertIn("rewrite", bench.report(self.file(with_first=2,
                                                        quiet_first=6)))

    def test_a_sweep_of_two_arms_says_nothing_about_a_sentence_it_did_not_move(self):
        self.assertNotIn("### The sentence", bench.report(sweep()))

    def test_the_arm_names_are_stated_once_and_read_back_by_the_scripts(self):
        # `build_arm.sh` and `run_one.sh` ask for this list instead of spelling
        # it, so a fourth arm is one line and no script can hold a stale set.
        with contextlib.redirect_stdout(io.StringIO()) as printed:
            bench.main(["arms"])
        self.assertEqual(printed.getvalue().split(), list(bench.ARMS))
        self.assertEqual(bench.ARMS[2], "no-trigger")

    def test_a_record_line_is_written_for_the_third_arm_by_name(self):
        with tempfile.TemporaryDirectory() as root:
            rundir = pathlib.Path(root)
            (rundir / "out.json").write_text('{"usage": {"input_tokens": 10}}')
            with contextlib.redirect_stdout(io.StringIO()) as printed:
                bench.main(["record", str(rundir), "--task", "refused-write",
                            "--arm", "no-trigger"])
        self.assertEqual(json.loads(printed.getvalue())["arm"], "no-trigger")


class ATaskScoredOnPriceIsComparedByTheTokensItSpent(unittest.TestCase):
    """The reading skill buys no outcome a model would not otherwise reach — it buys
    the same answer for fewer tokens — so its arms are compared by the mean input
    tokens and its rule's outcome check asks only that the answer was reached. A
    sweep of the two arms over one task, six repetitions each, built here so the
    arithmetic owes nothing to a fixture."""

    def sweep(self, tokens, without_tokens, activated=6, right=6):
        lines = []
        for arm, spent in (("with", tokens), ("without", without_tokens)):
            for index in range(6):
                lines.append({"arm": arm, "model": "m", "task": "inspect",
                              "in_tokens": spent,
                              "activated": arm == "with" and index < activated,
                              "loaded_before_bash": None,
                              "checks": {"ok": index < right}})
        return lines

    def test_a_reading_skill_that_costs_less_earns_its_catalog_line(self):
        report = bench.report(self.sweep(600, 1400))
        self.assertIn("`search-and-read` earns its catalog line", report)
        self.assertIn("0.43x the mean input tokens without it", report)

    def test_a_reading_skill_a_quarter_dearer_has_bought_nothing(self):
        # The outcome axis allows a body a quarter again in tokens to buy an
        # outcome. The price axis cannot: there the tokens *are* the purchase, so
        # 1.20x is a loss the table has to read as one, and the rule's third check
        # is a floor of its own.
        report = bench.report(self.sweep(1200, 1000))
        self.assertIn("`search-and-read` does not pay for itself", report)
        self.assertIn("1.20x", report)
        self.assertIn("| price | 1.20x | under 1.00x | no |", report)
        self.assertNotIn("earns its catalog line", report)

    def test_a_reading_skill_that_costs_more_does_not_pay_for_itself(self):
        # The one state the price axis adds: the same answer, bought dearer. A
        # reading skill that costs more than the reads it replaces has bought
        # nothing, and the sentence says so rather than keeping it on outcome.
        report = bench.report(self.sweep(1400, 600))
        self.assertIn("`search-and-read` does not pay for itself", report)
        self.assertIn("2.33x", report)
        self.assertNotIn("earns its catalog line", report)

    def test_a_body_whose_answers_were_wrong_is_useless_however_cheap_it_was(self):
        report = bench.report(self.sweep(600, 1400, right=4))
        self.assertIn("**useless**", report)
        self.assertNotIn("earns its catalog line", report)

    def test_a_body_no_run_loaded_is_unused_and_the_description_is_what_to_fix(self):
        report = bench.report(self.sweep(600, 1400, activated=0))
        self.assertIn("**unused**", report)
        self.assertIn("rewrite the description", report)

    def test_the_outcome_row_asks_only_that_the_answer_was_reached(self):
        # Both arms right is the premise of a reading task, so a rule reading it as
        # the outcome axis would call the body useless for doing its job.
        report = bench.report(self.sweep(600, 1400))
        self.assertIn(
            "| outcome | 6/6 with, 6/6 without | every run with it right | yes |",
            report,
        )

    def test_the_report_says_which_task_and_axis_it_read(self):
        # Two tables of one bench are two questions: a reader told neither cannot
        # tell which numbers were the score.
        report = bench.report(self.sweep(600, 1400))
        self.assertIn("`inspect` ablates `search-and-read`, scored on price", report)

    def test_a_file_naming_a_task_the_bench_does_not_hold_is_refused(self):
        lines = self.sweep(600, 1400)
        for line in lines:
            line["task"] = "sideways"
        with self.assertRaises(SystemExit):
            bench.report(lines)


if __name__ == "__main__":
    unittest.main()
