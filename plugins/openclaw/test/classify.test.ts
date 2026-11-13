import assert from "node:assert/strict";
import { test } from "node:test";
import { classify, extractCurrentMessage, keywordFlags, markerFlags, type DecisionEvaluate, type Flag } from "../src/classify.js";

const KW = { timeoutMs: 300, threshold: 0.6 };

const cases: Array<[string, Flag[], string]> = [
  ["thanks, that's perfect", ["praise"], "keyword"],
  ["yes, ship it", ["praise"], "keyword"],
  ["Great work on the launch", ["praise"], "keyword"],
  ["that's wrong, the floor is 0.02", ["correction"], "keyword"],
  ["No, I meant the other group", ["correction"], "keyword"],
  ["why did you restart it?", ["correction", "security"], "keyword"],
  ["thanks but that's not what I asked", ["correction"], "keyword"],
  ["the cron stalled again and the build failed", ["failure"], "keyword"],
  ["rotate the token and check the gateway config", ["security"], "keyword"],
  ["can you summarise the article for me", [], "keyword"],
  ["look at https://example.com/config?token=abc", [], "keyword"],
  ["know your audience; nobody said no", [], "keyword"],
  ["when it achieves something great or good, depending on interest", [], "keyword"],
  ["I want you to also make sure the levels of motivation work", [], "keyword"],
  ["Great work on the launch, well done", ["praise"], "keyword"],
  ["[praise] the deploy broke but you handled it", ["praise"], "marker"],
  ["[upset] [security] who touched the keys", ["owner_upset", "security"], "marker"],
  ["[CORRECTION] redo it", ["correction"], "marker"],
  ["<<<BEGIN_OPENCLAW_INTERNAL_CONTEXT>>>\nconfig gateway token\n<<<END_OPENCLAW_INTERNAL_CONTEXT>>>\nhello there", [], "keyword"],
  ["", [], "none"],
  // v0.2: win (good news that happened to the user)
  ["she replied!", ["win"], "keyword"],
  ["I got the interview for Thursday", ["win"], "keyword"],
  ["we got the offer", ["win"], "keyword"],
  ["Daniel signed the contract this morning", ["win"], "keyword"],
  ["we closed the deal", ["win"], "keyword"],
  ["it worked", ["win"], "keyword"],
  ["the site went live an hour ago", ["win"], "keyword"],
  ["first customer just paid", ["win"], "keyword"],
  ["they accepted the proposal", ["win"], "keyword"],
  ["no reply yet", [], "keyword"],
  ["the deal fell through", [], "keyword"],
  ["offer expired", [], "keyword"],
  ["it didn't work", [], "keyword"],
  ["has she replied?", [], "keyword"],
  ["not signed yet, maybe Friday", [], "keyword"],
  ["I hope they accepted", [], "keyword"],
  ["I signed in again", [], "keyword"],
  // v0.2: wants_energy
  ["give me more energy today", ["wants_energy"], "keyword"],
  ["can you be more upbeat?", ["wants_energy"], "keyword"],
  ["hype me up", ["wants_energy"], "keyword"],
  ["motivate me, it's a long one", ["wants_energy"], "keyword"],
  ["celebrate with me", ["wants_energy"], "keyword"],
  ["no cheerleading", [], "keyword"],
  ["less hype please", [], "keyword"],
  ["don't hype me up, just the numbers", [], "keyword"],
  // v0.2 precedence: win sits beside praise, correction and failure; correction still drops praise
  ["thanks, it worked!", ["praise", "win"], "keyword"],
  ["that's wrong, but we got the offer", ["correction", "win"], "keyword"],
  ["the cron failed but the site went live", ["failure", "win"], "keyword"],
  ["thanks but that's not what I asked, though she replied", ["correction", "win"], "keyword"],
  ["celebrate with me, we got the offer", ["win", "wants_energy"], "keyword"],
  ["[win] small one", ["win"], "marker"],
  ["[ENERGY] long day", ["wants_energy"], "marker"],
  ["[win] [correction] the deal fell through after all", ["correction", "win"], "marker"],
];

for (const [text, flags, method] of cases) {
  test(`classify ${JSON.stringify(text.slice(0, 40))} -> ${flags.join("+") || "none"} via ${method}`, async () => {
    const c = await classify(text, KW);
    assert.deepEqual(c.flags, flags);
    assert.equal(c.method, method);
  });
}

test("markers and keywords are pure functions", () => {
  assert.deepEqual(markerFlags("[failure] x [failure]"), ["failure"]);
  assert.deepEqual(keywordFlags("great, but wrong"), ["correction"]);
  assert.deepEqual(markerFlags("[energy] [win] [energy]"), ["win", "wants_energy"]);
  assert.deepEqual(keywordFlags("Has it worked? It worked."), ["win"], "the question does not count, the statement does");
});

function fakeDecisions(answers: Record<string, number>, calls: unknown[]): DecisionEvaluate {
  return async (request, options) => {
    calls.push({ request, options });
    const out: Record<string, { type: string; probabilityTrue: number }> = {};
    for (const [k, p] of Object.entries(answers)) out[k] = { type: "boolean", probabilityTrue: p };
    return { status: "ok", result: { answers: out } };
  };
}

test("decision model: p >= 0.6 accepted, 0.59 rejected, failure still from keywords", async () => {
  const calls: unknown[] = [];
  const c = await classify("hmm the job errored, whatever", {
    ...KW,
    agentId: "main",
    evaluate: fakeDecisions({ praise: 0.59, correction: 0.6, owner_upset: 0.91, security: 0.1, win: 0.2, wants_energy: 0.1 }, calls),
  });
  assert.equal(c.method, "decision-model");
  assert.deepEqual(c.flags, ["correction", "owner_upset"]);
  assert.deepEqual(c.probabilities, { praise: 0.59, correction: 0.6, owner_upset: 0.91, security: 0.1, win: 0.2, wants_energy: 0.1 });
  assert.equal(calls.length, 1);
  const req = (calls[0] as { request: { questions: Record<string, { type: string }> }; options: { purpose: string; timeoutMs: number; rubricVersion: string } });
  assert.deepEqual(Object.keys(req.request.questions).sort(), ["correction", "owner_upset", "praise", "security", "time_pressure", "wants_energy", "win"]);
  assert.equal(req.options.timeoutMs, 300);
  assert.equal(req.options.rubricVersion, "2");
});

test("decision model v0.2: win and wants_energy answered by the model, with the brief's question text", async () => {
  const calls: unknown[] = [];
  const c = await classify("you will not believe the call I just had", {
    ...KW,
    evaluate: fakeDecisions({ praise: 0.1, correction: 0.05, owner_upset: 0.02, security: 0.01, win: 0.88, wants_energy: 0.64 }, calls),
  });
  assert.equal(c.method, "decision-model");
  assert.deepEqual(c.flags, ["win", "wants_energy"]);
  const q = (calls[0] as { request: { questions: Record<string, { instructions: string }> } }).request.questions;
  assert.match(q.win!.instructions, /real good news that happened to them .*not praise of the assistant\?$/);
  assert.equal(q.wants_energy!.instructions, "Does the user ask the assistant for more energy, enthusiasm or motivation?");
});

test("decision model v0.2: the model's 'no' on win wins over a keyword hit (only failure merges from keywords)", async () => {
  const c = await classify("it worked, thanks", {
    ...KW,
    evaluate: fakeDecisions({ praise: 0.9, correction: 0.0, owner_upset: 0.0, security: 0.0, win: 0.3, wants_energy: 0.0 }, []),
  });
  assert.deepEqual(c.flags, ["praise"]);
});

test("decision model: keyword failure is merged in", async () => {
  const c = await classify("the build failed", { ...KW, evaluate: fakeDecisions({ praise: 0.1, correction: 0.1, owner_upset: 0.7, security: 0.2 }, []) });
  assert.equal(c.method, "decision-model");
  assert.deepEqual(c.flags, ["failure", "owner_upset"]);
});

test("decision model unavailable -> keyword fallback with the reason recorded", async () => {
  const evaluate: DecisionEvaluate = async () => ({ status: "unavailable", reason: "not-configured" });
  const c = await classify("thanks", { ...KW, evaluate });
  assert.equal(c.method, "keyword");
  assert.deepEqual(c.flags, ["praise"]);
  assert.equal(c.decisionNote, "unavailable:not-configured");
});

test("decision model that hangs -> keyword fallback after the deadline", async () => {
  const evaluate: DecisionEvaluate = () => new Promise(() => undefined);
  const t0 = Date.now();
  const c = await classify("check the gateway", { ...KW, timeoutMs: 60, evaluate });
  assert.ok(Date.now() - t0 < 1000);
  assert.equal(c.method, "keyword");
  assert.deepEqual(c.flags, ["security"]);
  assert.equal(c.decisionNote, "deadline");
});

test("decision model that throws -> keyword fallback", async () => {
  const evaluate: DecisionEvaluate = async () => {
    throw new Error("boom");
  };
  const c = await classify("perfect", { ...KW, evaluate });
  assert.equal(c.method, "keyword");
  assert.match(c.decisionNote ?? "", /^error:/);
});

test("markers skip the decision model entirely", async () => {
  const calls: unknown[] = [];
  const c = await classify("[security] rotate", { ...KW, evaluate: fakeDecisions({}, calls) });
  assert.equal(c.method, "marker");
  assert.equal(calls.length, 0);
});


const LABELED =
  "Conversation info: ⟦openclaw:ctx⟧\n```json\n{\"chat_id\":\"telegram:-1\"}\n```\n\n" +
  "Conversation context (chronological, selected for current message): ⟦openclaw:ctx⟧\n#session:a 2026-10-06 OpenClaw: the deploy failed and the gateway config broke\n#26650 2026-10-06 [reply target] Pepe: security means careful\n\n" +
  "Current message:\n[Replying to: \"security means careful, the token rotated\"]\n#26652: Yes R46 and tell me how you going to use it";
const UNLABELED =
  "Conversation info: ⟦openclaw:ctx⟧\n```json\n{\"chat_id\":\"telegram:-1\"}\n```\n\n" +
  "Conversation context (chronological, selected for current message): ⟦openclaw:ctx⟧\n#session:a 2026-10-06 User: the build failed\n\n" +
  "Please make sure to complete all the work.\n\nThanks.";

test("extractCurrentMessage: labeled prompt keeps only the current message, drops the reply quote and the #id prefix", async () => {
  const ex = extractCurrentMessage(LABELED);
  assert.equal(ex.source, "prompt:current-message");
  assert.equal(ex.text, "Yes R46 and tell me how you going to use it");
  assert.deepEqual((await classify(ex.text, KW)).flags, ["praise"]);
  assert.deepEqual((await classify(LABELED, KW)).flags, ["failure", "security"]);
});

test("extractCurrentMessage: unlabeled prompt drops the envelope blocks and keeps the message paragraphs", async () => {
  const ex = extractCurrentMessage(UNLABELED);
  assert.equal(ex.source, "prompt:after-context");
  assert.equal(ex.text, "Please make sure to complete all the work.\n\nThanks.");
  assert.deepEqual((await classify(ex.text, KW)).flags, ["praise"]);
});

test("extractCurrentMessage: a bare message is returned as is; an envelope-only prompt falls back to its tail", () => {
  assert.deepEqual(extractCurrentMessage("hello there"), { text: "hello there", source: "prompt:after-context" });
  const onlyEnvelope = "Conversation info: ⟦openclaw:ctx⟧\n```json\n{}\n```";
  assert.equal(extractCurrentMessage(onlyEnvelope).source, "prompt:tail");
});
