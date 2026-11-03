"""The delay, its cap and the jitter, asserted against the arithmetic they are.

The cap is the file's one decision, so the tests state it twice: as `retry_delay`
answering the number, and as `_ceiling` being the value the sequence flattens at.
"""

import unittest

from src import backoff


class TheDelayDoublesAndStopsAtTheCap(unittest.TestCase):
    def test_the_first_retry_waits_the_base_delay(self):
        self.assertEqual(backoff.retry_delay(0), backoff.BASE_DELAY)

    def test_each_spent_attempt_doubles_the_wait_until_the_cap(self):
        for attempt in range(5):
            with self.subTest(attempt=attempt):
                self.assertEqual(
                    backoff.retry_delay(attempt), backoff.BASE_DELAY * 2**attempt
                )

    def test_the_fifth_retry_is_the_first_one_the_cap_holds(self):
        # 0.25 * 2 ** 5 is 8.0 and 0.25 * 2 ** 4 is 4.0: the cap is reached exactly
        # at the fifth attempt and every attempt after it waits the same.
        self.assertEqual(backoff.retry_delay(4), 4.0)
        self.assertEqual(backoff.retry_delay(5), backoff._ceiling)

    def test_a_long_outage_waits_no_longer_than_the_cap(self):
        for attempt in range(5, 40):
            with self.subTest(attempt=attempt):
                self.assertEqual(backoff.retry_delay(attempt), backoff._ceiling)


class TheJitterShortensTheWaitAndNeverGrowsIt(unittest.TestCase):
    def test_a_chooser_at_its_highest_takes_the_whole_share_back(self):
        self.assertEqual(
            backoff.jittered(1.0, spread=0.1, rng=lambda: 1.0), 0.9
        )

    def test_a_chooser_at_zero_leaves_the_wait_whole(self):
        self.assertEqual(backoff.jittered(1.0, spread=0.1, rng=lambda: 0.0), 1.0)

    def test_the_ceiling_is_still_the_longest_wait_once_jittered(self):
        for attempt in range(0, 40):
            with self.subTest(attempt=attempt):
                self.assertLessEqual(
                    backoff.attempt_delay(attempt, rng=lambda: 0.0),
                    backoff._ceiling,
                )


class TheSequenceAndTheGivingUp(unittest.TestCase):
    def test_the_schedule_is_one_wait_per_attempt_spent(self):
        self.assertEqual(len(backoff.schedule(6, rng=lambda: 0.0)), 6)

    def test_the_schedule_grows_then_flattens_at_the_cap(self):
        waits = backoff.schedule(9, rng=lambda: 0.0)
        self.assertEqual(waits[0], backoff.BASE_DELAY)
        self.assertEqual(waits[-1], backoff._ceiling)

    def test_the_loop_gives_up_where_the_attempt_cap_says(self):
        self.assertFalse(backoff.gave_up(backoff.MAX_ATTEMPTS - 1))
        self.assertTrue(backoff.gave_up(backoff.MAX_ATTEMPTS))

    def test_a_dead_lettered_attempt_is_described_as_one(self):
        self.assertIn("dead-lettered", backoff.describe(backoff.MAX_ATTEMPTS))


class APolicyOfItsOwnMovesTheCapAndNothingElse(unittest.TestCase):
    def test_a_policy_with_a_lower_cap_holds_sooner(self):
        policy = backoff.Policy(cap=2.0)
        self.assertEqual(policy.delay(3), 2.0)
        self.assertEqual(backoff.retry_delay(3), 2.0)

    def test_a_policy_with_a_higher_cap_waits_past_the_modules_one(self):
        policy = backoff.Policy(cap=64.0)
        self.assertEqual(policy.delay(7), 32.0)
        self.assertEqual(backoff.retry_delay(7), backoff._ceiling)

    def test_the_growth_is_one_function_the_module_and_a_policy_share(self):
        # The two differ in their cap and in nothing else, so a policy's uncapped
        # growth is the module's: a divergence here is two arithmetics to keep.
        policy = backoff.Policy(base=1.0, cap=64.0)
        self.assertEqual(policy.delay(3), backoff.grown(1.0, 3))


if __name__ == "__main__":
    unittest.main()
