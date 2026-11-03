"""What the ablation bench asserts without a model: the record it writes, the
rule it scores with, and the table it prints.

Every test builds the lines it reads rather than borrowing another's, and every
name states the behaviour it proves — a bench whose own arithmetic is wrong
reports a number nobody can trust.
"""
import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import bench  # noqa: E402


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
            entries += skill_call(1, bench.SKILL)
            entries += [call(2, "sh tests/run.sh"), result(2, 0)]
        fields = dict(task="refused-write", arm="with", tag="a",
                      repetition=1, attempt=1, model="m", run=run,
                      entries=entries, fixed=True, elapsed_s=9)
        fields.update(overrides)
        return bench.record(**fields)

    def test_the_summed_input_tokens_come_from_the_runs_own_usage(self):
        self.assertEqual(self.line()["in_tokens"], 1234)

    def test_a_run_that_reported_no_usage_carries_no_token_count(self):
        # Absent is not zero: a run that never reached the model is not a
        # measurement, and a null is how the median drops it.
        run = {"turns": 0, "tool_calls": 0, "stopped": "error", "rc": 1}
        line = bench.record(task="t", arm="with", tag="a", repetition=1,
                            attempt=1, model="m", run=run, entries=[],
                            fixed=False, elapsed_s=1)
        self.assertIsNone(line["in_tokens"])

    def test_whether_the_skill_was_activated_is_a_field_of_its_own(self):
        without = [call(1, "sh tests/run.sh"), result(1, 0)]
        self.assertTrue(self.line()["activated"])
        self.assertFalse(self.line(entries=without, arm="without")["activated"])

    def test_the_arm_is_on_the_line_because_two_arms_share_one_file(self):
        self.assertEqual(self.line()["arm"], "with")

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
        self.assertTrue(bench.activated(skill_call(1, bench.SKILL), bench.SKILL))

    def test_a_second_call_to_a_loaded_skill_is_not_a_second_activation(self):
        entries = skill_call(1, bench.SKILL, outcome="already_loaded")
        self.assertFalse(bench.activated(entries, bench.SKILL))

    def test_another_skills_body_is_not_this_ones_activation(self):
        self.assertFalse(bench.activated(skill_call(1, "house-style"), bench.SKILL))

    def test_a_call_the_loop_refused_without_a_tool_counts_as_no_activation(self):
        entries = [{"type": "tool_call", "turn": 1, "call_id": "c1",
                    "tool": "refused", "named": "skill", "problem": "no such tool"}]
        self.assertFalse(bench.activated(entries, bench.SKILL))


class TheTableIsTheMeansOfOneArmAgainstTheOther(unittest.TestCase):
    def file(self):
        with_right = [{"arm": "with", "in_tokens": 1000, "activated": True,
                       "checks": {"ok": True}},
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
                                         "median": 1200, "activated": 1, "right": 1})
        self.assertEqual(table["without"], {"runs": 2, "billed": 1, "mean": 300,
                                            "median": 300, "activated": 0, "right": 1})

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


if __name__ == "__main__":
    unittest.main()
