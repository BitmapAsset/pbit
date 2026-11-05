"""Tests for python/probbit.py against the release binary (stdlib unittest; run: python3 python/test_probbit.py)."""
import json, os, unittest
import probbit
from mock_judge import MockJudge

EX = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "examples")


def load(name):
    with open(os.path.join(EX, name)) as f:
        return json.load(f)


class ProbbitWrapper(unittest.TestCase):
    def test_run_exact_knapsack(self):
        a = probbit.run(load("knapsack-20.json"))
        self.assertEqual(a["verdict"], "exact"); self.assertEqual(len(a["plan"]), 20)
        self.assertTrue(all(0.0 <= p <= 1.0 for o in a["marginals"].values() for p in o.values()))

    def test_exact_op_and_text_input(self):
        with open(os.path.join(EX, "agent-plan-6.json")) as f:
            a = probbit.exact(f.read())
        self.assertEqual(a["verdict"], "exact")

    def test_sample_fixed_work_is_deterministic(self):
        prog = load("knapsack-20.json")
        a, b = (probbit.sample(prog, sweeps=300, seed=3, polish_ms=0) for _ in range(2))
        self.assertIn(a["verdict"], ("diagnostics_passed", "partial", "refused"))
        self.assertEqual((a["plan"], a["marginals"]), (b["plan"], b["marginals"]))

    def test_decide_router_demo(self):
        a = probbit.decide(probbit.demo(tasks=12, seed=1), budget_ms=100, collective=True)
        self.assertIn(a["verdict"], ("exact", "diagnostics_passed", "partial", "refused")); self.assertEqual(len(a["plan"]), 12)

    def test_infeasible_is_an_answer(self):
        prog = {"probbit_ir": 1, "values": ["a"], "vars": [{"id": "x"}, {"id": "y"}], "caps": [{"value": "a", "limit": 1}]}
        a = probbit.run(prog)
        self.assertEqual(a["verdict"], "infeasible")

    def test_input_error_is_typed(self):
        with self.assertRaises(probbit.ProbbitInputError) as e:
            probbit.run({"variables": "x"})
        self.assertEqual((e.exception.code, e.exception.path, e.exception.exit_code), ("schema", "variables", 2))

    def test_flag_error_is_typed(self):
        with self.assertRaises(probbit.ProbbitInputError) as e:
            probbit.run(load("knapsack-20.json"), budget_ms=-1)
        self.assertEqual(e.exception.code, "flag"); self.assertIn("--budget-ms", e.exception.message)

    def test_bad_op_and_timeout(self):
        with self.assertRaises(ValueError):
            probbit.run({}, op="nope")
        with self.assertRaises(probbit.ProbbitTimeout):
            probbit.decide(probbit.demo(tasks=24, seed=1), mode="sample", budget_ms=3000, timeout_s=0.2)

    def test_emoji_ids_round_trip(self):
        # json.dumps escapes them by default ("\ud83d\ude00"); they decoded to U+FFFD, came back changed and collided
        doc = probbit.demo(tasks=3, seed=1); doc["tasks"][0]["id"] = "T\U0001F600"; doc["tasks"][1]["id"] = "T\U0001F601"
        a = probbit.decide(doc, budget_ms=50)
        self.assertEqual(sorted(a["plan"]), sorted(t["id"] for t in doc["tasks"]))

    def test_switches_take_no_value(self):
        # pretty=True became `--pretty on` (exit 2: unknown argument "on"); a boolean switch is now the bare flag, False omits it
        doc = probbit.demo(tasks=12, seed=1)
        a = probbit.decide(doc, pretty=True, summary=True)
        self.assertEqual(a["summary"], 1); self.assertIn("counts", a); self.assertNotIn("plan", a)
        b = probbit.decide(doc, pretty=False, summary=False, collective=False)
        self.assertIn("plan", b); self.assertEqual(a["verdict"], b["verdict"])

    def test_deadline(self):
        a = probbit.run(load("denoise-8x12.json"), deadline_ms=500)  # `met` depends on machine load; the contract is the field
        self.assertEqual(a["deadline"]["ms"], 500); self.assertIsInstance(a["deadline"]["met"], bool)
        self.assertEqual([x["phase"] for x in a["phases"]][0], "parse")



KEY_VAR = "PROBBIT_TEST_JUDGE_KEY"


def judge_rows(req):
    """The example's judge answers as mock rows: noul -> P(true), choice / score -> probabilities"""
    return {k: (a["noul"] if a["type"] == "noul" else a["probabilities"]) for k, a in req["probbit"]["judge"]["answers"].items()}


class Evaluate(unittest.TestCase):
    """probbit.evaluate: judge -> probbit, with the judge a URL (a stdlib mock server that speaks the verified System One response
    shape), a callable, or answers already in the request."""
    def setUp(self):
        self.full = load(os.path.join("evaluate", "support-12.json"))
        self.ask = {k: v for k, v in self.full.items() if k != "probbit"}
        self.rules = {"rules": self.full["probbit"]["rules"]}
        os.environ[KEY_VAR] = "k-123"

    def tearDown(self):
        os.environ.pop(KEY_VAR, None)

    def test_judge_url_round_trip(self):
        with MockJudge(judge_rows(self.full), model="example-judge", key="k-123") as m:
            a = probbit.evaluate(dict(self.ask, probbit=self.rules), judge=m.url, auth_env=KEY_VAR)
        (path, headers, body), = m.requests
        self.assertEqual(path, "/v1/systemone"); self.assertEqual(headers["Authorization"], "Bearer k-123")
        self.assertEqual(json.loads(body), self.ask)  # state, instructions, criteria carried untouched; no probbit block
        self.assertEqual((a["verdict"], a["violations"], a["model"]), ("exact", 0, "example-judge"))
        self.assertEqual([k for k, x in a["answers"].items() if x["probbit"]["changed"]], ["team", "severity", "refund_action", "escalate_to_human", "reply_channel"])
        inline = probbit.evaluate(self.full)  # the same judge numbers given inline: the same answer
        strip = lambda d: {k: v for k, v in d.items() if k not in ("ms", "telemetry", "phases", "usage")}
        self.assertEqual(strip(a), strip(inline))

    def test_callable_judge(self):
        seen = []
        a = probbit.evaluate(dict(self.ask, probbit=self.rules), judge=lambda ask: seen.append(ask) or judge_rows(self.full))  # probabilities
        b = probbit.evaluate(dict(self.ask, probbit=self.rules), judge=lambda ask: self.full["probbit"]["judge"])  # a System One response
        want = probbit.evaluate(self.full)
        self.assertEqual(seen, [self.ask]); self.assertEqual(a["answers"], want["answers"]); self.assertEqual(b["answers"], want["answers"])
        self.assertEqual(b["usage"], {"input_tokens": 412, "output_tokens": 24})

    def test_workers_ai_endpoint_and_envelope(self):
        with MockJudge(judge_rows(self.full), model="clef", envelope=True) as m:
            a = probbit.evaluate(dict(self.ask, probbit=self.rules), judge=m.url + "/client/v4/accounts/acct/ai/run/@cf/cloudflare/clef")
        self.assertEqual(m.requests[0][0], "/client/v4/accounts/acct/ai/run/@cf/cloudflare/clef"); self.assertNotIn("Authorization", m.requests[0][1])
        self.assertEqual((a["model"], a["verdict"], a["answers"]["team"]["choice"]), ("clef", "exact", "technical"))

    def test_judge_failures_are_typed(self):
        with MockJudge(key="right") as m:
            os.environ.pop(KEY_VAR)
            with self.assertRaises(probbit.ProbbitJudgeError):
                probbit.evaluate(self.ask, judge=m.url, auth_env=KEY_VAR)
            self.assertEqual(m.requests, [])  # no key: nothing sent
            os.environ[KEY_VAR] = "wrong"
            with self.assertRaises(probbit.ProbbitJudgeError) as e:
                probbit.evaluate(self.ask, judge=m.url, auth_env=KEY_VAR)
            self.assertEqual(e.exception.status, 401)
            url = m.url
        with self.assertRaises(probbit.ProbbitJudgeError):
            probbit.evaluate(self.ask, judge=url, judge_timeout_s=2)  # the server is gone
        with self.assertRaises(ValueError):
            probbit.evaluate(self.ask, judge="ftp://example.invalid")

    def test_flags_refusal_and_bad_input(self):
        a = probbit.evaluate(self.full, op="sample", sweeps=400, polish_ms=0)  # exit 3 is an answer
        self.assertEqual(a["verdict"], "refused"); self.assertTrue(all(not x["probbit"]["released"] for x in a["answers"].values()))
        s = probbit.evaluate(self.full, summary=True)
        self.assertEqual(s["summary"], 1); self.assertIn("answers", s); self.assertNotIn("marginals", s)
        with self.assertRaises(probbit.ProbbitInputError) as e:
            probbit.evaluate({"questions": {"a": {"type": "boolean"}}})
        self.assertEqual((e.exception.code, e.exception.path), ("value", "questions.a.type"))


if __name__ == "__main__":
    unittest.main()
