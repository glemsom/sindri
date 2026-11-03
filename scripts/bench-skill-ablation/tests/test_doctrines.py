"""The doctrine as the bench moves it: the third arm's text, and the two ways a
binary is asked which text it carries.

An arm that is the shipped arm under another name reports one wording twice, so
the removal refuses a text that does not carry the sentence, and the shipped file
is read here rather than trusted: the bench and the product agree on one line,
and this is where a reformat that loses it is a failure instead of a number.
"""
import contextlib
import io
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))

import doctrines  # noqa: E402

SHIPPED = (pathlib.Path(__file__).resolve().parents[3]
           / "crates/core/src/agent/skill/doctrine.md")


class TheThirdArmIsTheShippedDoctrineOneSentenceShorter(unittest.TestCase):
    def test_the_sentence_the_arm_removes_is_printed_for_a_shell_that_asks(self):
        # A shell looking for the sentence asks for it instead of holding a copy
        # that drifts from the one the arm removes.
        with contextlib.redirect_stdout(io.StringIO()) as printed:
            doctrines.main(["sentence"])
        self.assertEqual(printed.getvalue().strip(), doctrines.TRIGGER)

    def test_the_shipped_doctrine_carries_the_sentence_the_third_arm_removes(self):
        self.assertIn(doctrines.TRIGGER, SHIPPED.read_text())

    def test_the_sentence_removed_is_the_only_line_that_changed(self):
        shipped = SHIPPED.read_text()
        shorter = doctrines.without_trigger(shipped)
        self.assertNotIn(doctrines.TRIGGER, shorter)
        self.assertEqual(
            [line for line in shipped.splitlines()
             if doctrines.TRIGGER not in line],
            shorter.splitlines(),
            "every other line is the shipped one, byte for byte",
        )

    def test_a_sentence_that_shares_its_line_with_other_words_is_refused(self):
        # Removing the line would take its neighbours with it, and the arm would
        # be a rewrite rather than the absence of one sentence.
        with self.assertRaises(SystemExit) as refused:
            doctrines.without_trigger(
                f"Loads one skill. {doctrines.TRIGGER} Loads once.\n")
        self.assertIn("shares its line", str(refused.exception))

    def test_a_doctrine_that_lost_its_sentence_is_refused_not_shipped_as_an_arm(self):
        # The state a reformat leaves behind: the third arm would be the shipped
        # one under another name, and the sweep would report one wording twice.
        with self.assertRaises(SystemExit) as refused:
            doctrines.without_trigger("Loads one skill.\n")
        self.assertIn("no trigger sentence", str(refused.exception))


class ABinaryIsAskedWhichTextItCarries(unittest.TestCase):
    def binary(self, text):
        root = tempfile.TemporaryDirectory()
        self.addCleanup(root.cleanup)
        path = pathlib.Path(root.name) / "sindri"
        path.write_text(text)
        return path

    def test_a_binary_carrying_the_sentence_answers_yes(self):
        self.assertTrue(doctrines.carries(self.binary(f"x {doctrines.TRIGGER} y")))

    def test_a_binary_without_the_sentence_answers_no(self):
        self.assertFalse(doctrines.carries(self.binary("Loads one skill.\n")))

    def test_a_rewrapped_sentence_is_refused_by_the_arm_and_read_as_absent(self):
        # One fact and not two: the doctrine is compiled in byte for byte, so a
        # prose rewrapped around the sentence is a text the arm build refuses and
        # a binary that answers no — never a binary that answers yes while the
        # arm cannot be built from the same file.
        wrapped = doctrines.TRIGGER.replace(" ", "\n", 2)
        self.assertFalse(doctrines.carries(self.binary(wrapped)))
        with self.assertRaises(SystemExit):
            doctrines.without_trigger(f"{wrapped}\n")


if __name__ == "__main__":
    unittest.main()
