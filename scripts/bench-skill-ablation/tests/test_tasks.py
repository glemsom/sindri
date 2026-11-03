"""What the bench's tasks are: the skill each one ablates, the axis it is scored
on, and the check the inspect-shaped one scores an answer with.

The two tasks exist because a skill is worth two different things. A body that
buys an outcome is scored by whether the work held afterwards; a body that buys a
cheaper read is scored by the tokens the run spent to reach the same answer, and a
reading skill that costs more than it saves is a line to drop whatever its outcome.
"""
import io
import contextlib
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import bench  # noqa: E402
import tasks  # noqa: E402

BENCH = pathlib.Path(__file__).resolve().parents[1]


def inspect_repo():
    """The fixture the inspect task stands in, as the bench hands it to a run."""
    return BENCH / tasks.TASKS["inspect"]["fixture"]


def right_answer():
    """An answer that named what the prompt asked for, built from the fixture so no
    test carries a line number the fixture could move."""
    line = tasks.deciding_line(inspect_repo() / tasks.SOURCE)
    named = "\n".join(sorted(tasks.mentioning("inspect")))
    return (
        f"After attempt 7 the delay is {tasks.CAP}s, held by the check at "
        f"`{tasks.SOURCE}:{line}`:\n\n    return _ceiling\n\n"
        f"Files mentioning `_ceiling`:\n{named}\n"
    )


class TheBenchHoldsTwoTasksShapedDifferently(unittest.TestCase):
    def test_the_two_tasks_ablate_the_two_built_ins_the_tree_ships_procedures_for(self):
        self.assertEqual(tasks.TASKS["refused-write"]["skill"], "sandbox-refusal")
        self.assertEqual(tasks.TASKS["inspect"]["skill"], "search-and-read")

    def test_the_inspect_task_is_scored_on_price_and_the_capability_one_on_outcome(self):
        self.assertEqual(tasks.axis("inspect"), "price")
        self.assertEqual(tasks.axis("refused-write"), "outcome")

    def test_each_task_stands_in_a_fixture_of_its_own(self):
        one, other = (BENCH / tasks.TASKS[name]["fixture"] for name in tasks.TASKS)
        self.assertTrue(one.is_dir(), f"{one} is not a repository")
        self.assertTrue(other.is_dir(), f"{other} is not a repository")
        self.assertNotEqual(one.resolve(), other.resolve())

    def test_an_unknown_task_is_refused_by_the_names_the_bench_holds(self):
        with self.assertRaises(SystemExit) as refused:
            tasks.task("sideways")
        self.assertIn("refused-write", str(refused.exception))
        self.assertIn("inspect", str(refused.exception))

    def test_the_task_names_are_read_back_by_the_scripts_rather_than_spelled_again(self):
        with contextlib.redirect_stdout(io.StringIO()) as printed:
            bench.main(["tasks"])
        self.assertEqual(printed.getvalue().split(), list(tasks.TASKS))


class TheInspectFixtureHoldsTheAnswerTheCheckReads(unittest.TestCase):
    def test_the_cap_the_check_expects_is_the_one_the_fixture_applies(self):
        # Not the check's own arithmetic read back: the fixture's function is
        # called, and its answer for attempt 7 is what the check looks for.
        sys.path.insert(0, str(inspect_repo()))
        try:
            from src import backoff  # noqa: PLC0415 - the fixture, imported once
        finally:
            sys.path.remove(str(inspect_repo()))
        self.assertEqual(backoff.retry_delay(7), tasks.CAP)

    def test_the_deciding_line_is_the_one_that_returns_the_cap(self):
        line = tasks.deciding_line(inspect_repo() / tasks.SOURCE)
        source = (inspect_repo() / tasks.SOURCE).read_text().splitlines()
        self.assertIn("return _ceiling", source[line - 1])

    def test_the_mentioning_files_are_the_three_the_fixture_carries(self):
        self.assertEqual(
            tasks.mentioning("inspect"),
            {"src/backoff.py", "tests/test_backoff.py", "README.md"},
        )


class AnInspectAnswerIsScoredOnWhatItNamed(unittest.TestCase):
    def test_an_answer_naming_the_line_the_delay_and_every_file_is_right(self):
        self.assertTrue(tasks.inspected("inspect", right_answer())["ok"])

    def test_an_answer_that_misses_a_file_that_mentions_the_cap_is_not_right(self):
        answer = right_answer().replace("README.md", "")
        check = tasks.inspected("inspect", answer)
        self.assertFalse(check["ok"])
        self.assertEqual(check["missing"], ["README.md"])

    def test_an_answer_with_the_uncapped_delay_is_not_right(self):
        answer = right_answer().replace(f"delay is {tasks.CAP}s", "delay is 32.0s")
        self.assertFalse(tasks.inspected("inspect", answer)["ok"])

    def test_an_answer_naming_the_wrong_line_is_not_right(self):
        answer = right_answer().replace(f"{tasks.SOURCE}:", f"{tasks.SOURCE}:1 ")
        check = tasks.inspected("inspect", answer)
        self.assertFalse(check["ok"])
        self.assertFalse(check["line"])

    def test_a_run_that_answered_nothing_is_not_right(self):
        self.assertFalse(tasks.inspected("inspect", "")["ok"])

    def test_a_run_that_pasted_its_search_instead_of_answering_is_not_right(self):
        # The material the answer is cut from carries the line, every file's name
        # and the cap's own value, so a check that only read for those would score
        # a dump as an answer. The delay has to be *said*.
        dump = (
            "src/backoff.py:23:_ceiling = 8.0\n"
            "src/backoff.py:55:        return _ceiling\n"
            "tests/test_backoff.py:12: backoff.retry_delay(7)\nREADME.md:9: cap\n"
        )
        check = tasks.inspected("inspect", dump)
        self.assertFalse(check["ok"], "a dump of the search is not an answer")
        self.assertFalse(check["delay"])

    def test_the_answer_is_read_from_the_bench_and_not_from_the_tree_that_was_run(self):
        # A run is given a copy to read; a check that read its expectation out of
        # that copy would move the answer to whatever the run did to it.
        with tempfile.TemporaryDirectory() as root:
            edited = pathlib.Path(root)
            for path in inspect_repo().rglob("*"):
                if path.is_file():
                    (edited / path.relative_to(inspect_repo())).parent.mkdir(
                        parents=True, exist_ok=True
                    )
                    (edited / path.relative_to(inspect_repo())).write_text("")
            self.assertTrue(
                tasks.inspected("inspect", right_answer())["ok"],
                "the answer is scored against the fixture, not against the run's tree",
            )


class ALineSaysWhichSkillAndWhichQuestionItCameFrom(unittest.TestCase):
    def line(self, task, **overrides):
        fields = dict(task=task, arm="with", tag="a", repetition=1, attempt=1,
                      model="m", run={"usage": {"input_tokens": 700}},
                      entries=[], repo=inspect_repo(), answer="", elapsed_s=3)
        fields.update(overrides)
        return bench.record(**fields)

    def test_an_inspect_line_is_scored_by_the_answer_it_carried(self):
        self.assertEqual(
            self.line("inspect", answer=right_answer())["checks"],
            {"ok": True, "line": True, "delay": True, "missing": []},
        )

    def test_an_inspect_line_reads_the_delay_and_the_cap_from_the_answer(self):
        self.assertTrue(self.line("inspect", answer=right_answer())["checks"]["ok"])
        self.assertFalse(self.line("inspect", answer="It waits a while.")["checks"]["ok"])


if __name__ == "__main__":
    unittest.main()
