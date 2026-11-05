import assert from "node:assert/strict";
import { test } from "node:test";
import { footerLine, FOOTER_MAX, movedNames } from "../src/block.js";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import plugin, { registerStance } from "../src/index.js";
import { resolveConfig } from "../src/config.js";
import { cliEvent, createHost, GROUP_KEY, groupCtx, PROBBIT, tempStateDir } from "./host.js";
import { timePressureFrom } from "../src/classify.js";

test("time_pressure: expected index / 4, 2 decimals; probabilities when score is missing; out of range -> undefined", () => {
  assert.equal(timePressureFrom({ score: 3.99 }), 1);
  assert.equal(timePressureFrom({ score: 2 }), 0.5);
  assert.equal(timePressureFrom({ score: 0.4 }), 0.1);
  assert.equal(timePressureFrom({ score: 0 }), 0);
  assert.equal(timePressureFrom({ probabilities: { "0": 0.5, "2": 0.5 } }), 0.25);
  assert.equal(timePressureFrom({ score: 5 }), undefined);
  assert.equal(timePressureFrom({}), undefined);
  assert.equal(timePressureFrom(undefined), undefined);
});

const DOC = {
  line: "Stance: say it plainly; warm; short and complete; celebrate with the numbers.",
  mood: { valence: { level: "even" }, pressure: { level: "steady" } },
  stance: { caution: { level: "measured" }, hype: { level: "grounded" } },
  persona: { name: "Assistant" },
};

test("footer: valence, pressure and line words; inputs and moves only when present", () => {
  assert.equal(footerLine(32, DOC), "Assistant stance #32 · valence even · pressure steady · say it plainly");
  assert.equal(
    footerLine(33, DOC, { inputs: ["praise", "time 0.75"], moved: ["valence", "caution"] }),
    "Assistant stance #33 · valence even · pressure steady · say it plainly · in: praise, time 0.75 · moved: valence, caution",
  );
  assert.equal(footerLine(34, DOC, { inputs: [], moved: [] }), "Assistant stance #34 · valence even · pressure steady · say it plainly");
});

test("footer: never longer than 120 characters; long lists shorten to +k", () => {
  const long = footerLine(1234, DOC, {
    inputs: ["praise", "correction", "failure", "owner_upset", "security", "win", "wants_energy", "time 0.75"],
    moved: ["valence", "pressure", "caution", "directness", "hype", "humour", "verbosity", "warmth", "emoji"],
  });
  assert.ok([...long].length <= FOOTER_MAX, `${[...long].length} chars: ${long}`);
  assert.match(long, /\+\d/);
  assert.match(long, / · in: .+ · moved: /u, "both parts stay visible");
  const tight = footerLine(2, { ...DOC, line: "Stance: no jokes about a miss; own it plainly." }, {
    inputs: ["correction", "failure", "owner_upset", "time 0.75"],
    moved: ["valence", "caution", "directness", "hype", "humour"],
  });
  assert.ok([...tight].length <= FOOTER_MAX, tight);
  assert.match(tight, / · in: correction, failure, .*\+\d · moved: .*\+\d$/u);
});

test("moved: mood then trait names whose level changed; undefined without a previous event", () => {
  const next = {
    ...DOC,
    mood: { valence: { level: "up" }, pressure: { level: "steady" } },
    stance: { caution: { level: "careful" }, hype: { level: "grounded" } },
  };
  assert.deepEqual(movedNames(DOC, next), ["valence", "caution"]);
  assert.deepEqual(movedNames(DOC, DOC), []);
  assert.equal(movedNames(undefined, DOC), undefined);
});


test("config: active classifiers use a 300 ms budget; removed classifier loads as keyword", () => {
  for (const classifier of [undefined, "auto", "keyword", "jev"]) {
    const raw = classifier === undefined ? {} : { classifier };
    assert.equal(plugin.configSchema.safeParse(raw).success, true);
    const { config, warnings, notices } = resolveConfig(raw);
    assert.equal(config.classifier, classifier === "jev" ? "keyword" : classifier ?? "auto");
    assert.equal(config.classifierTimeoutMs, 300);
    assert.deepEqual(warnings, []);
    assert.deepEqual(notices, classifier === "jev" ? ['classifier "jev" was removed; using keyword'] : []);
  }
  assert.equal(resolveConfig({ classifier: "jev", classifierTimeoutMs: 400 }).config.classifierTimeoutMs, 400);
  assert.equal(plugin.configSchema.safeParse({ classifier: "unknown" }).success, false);
});

test("config: classifierShadow is a nonfatal no-op for either boolean value", () => {
  for (const classifierShadow of [false, true]) {
    const raw = { classifierShadow };
    assert.equal(plugin.configSchema.safeParse(raw).success, true);
    const { config, warnings, notices } = resolveConfig(raw);
    assert.equal("classifierShadow" in config, false);
    assert.deepEqual(warnings, []);
    assert.deepEqual(notices, ["classifierShadow was removed; ignored"]);
  }
});

function logs(dir: string): Array<{ event: Record<string, unknown>; meta: Record<string, unknown> }> {
  return readFileSync(join(dir, "log", "team-room.jsonl"), "utf8").trim().split("\n").map((line) => {
    const cols = line.split("\t");
    return { event: JSON.parse(cols[1]!), meta: JSON.parse(cols[3]!) };
  });
}

test("hook: removed classifier warns once across turns and never calls fetch or the host model", async (t) => {
  const fetch = t.mock.method(globalThis, "fetch", async () => { throw new Error("unexpected network call"); });
  let modelCalls = 0;
  const dir = tempStateDir();
  const host = createHost(
    { lanes: { [GROUP_KEY]: "team-room" }, stateDir: dir, binary: PROBBIT, classifier: "jev", classifierShadow: true },
    {
      config: { agents: { defaults: { decisionModel: "example-judge" } } },
      runtime: { decisions: { evaluate: async () => { modelCalls++; throw new Error("unexpected host model call"); } } },
    },
  );
  const cfg = registerStance(host.api);
  assert.equal(cfg.classifier, "keyword");
  for (const [i, message] of ["thanks", "no, that's wrong", "[upset] fix it"].entries()) {
    const turn = await host.promptBuild(cliEvent(message), groupCtx(`legacy-${i}`));
    assert.equal(turn.threw, false);
    assert.ok(turn.result, JSON.stringify(host.logs));
  }
  assert.equal(modelCalls, 0);
  assert.equal(fetch.mock.callCount(), 0);
  const rows = logs(dir);
  assert.deepEqual(rows.map((r) => r.meta.method), ["keyword", "keyword", "marker"]);
  assert.deepEqual(rows.map((r) => r.meta.flags), [["praise"], ["correction"], ["owner_upset"]]);
  assert.ok(rows.every((r) => !("classifier" in r.meta) && !("time_pressure" in r.event)));
  const warnings = host.logs.filter((l) => l.level === "warn").map((l) => l.message);
  assert.deepEqual(warnings, [
    '[probbit-stance] config: classifier "jev" was removed; using keyword',
    '[probbit-stance] config: classifierShadow was removed; ignored',
  ]);
});

test("hook: keyword skips a configured host model; auto calls it only when configured", async () => {
  for (const [classifier, configured, expectedCalls] of [["keyword", true, 0], ["auto", false, 0], ["auto", true, 1]] as const) {
    let calls = 0;
    const dir = tempStateDir();
    const host = createHost(
      { lanes: { [GROUP_KEY]: "team-room" }, stateDir: dir, binary: PROBBIT, classifier },
      {
        config: configured ? { agents: { defaults: { decisionModel: "example-judge" } } } : {},
        runtime: { decisions: { evaluate: async () => {
          calls++;
          return { status: "ok", result: { answers: { correction: { probabilityTrue: 0.9 } } } };
        } } },
      },
    );
    registerStance(host.api);
    assert.ok((await host.promptBuild(cliEvent("thanks"), groupCtx("one"))).result);
    assert.equal(calls, expectedCalls);
    assert.deepEqual(logs(dir)[0]!.meta.flags, [expectedCalls ? "correction" : "praise"]);
  }
});

test("hook: auto host urgency feeds time pressure; off drops it; question wins over heuristic", async () => {
  for (const timePressureSource of ["question", "off", "heuristic"] as const) {
    const dir = tempStateDir();
    const host = createHost(
      { lanes: { [GROUP_KEY]: "team-room" }, stateDir: dir, binary: PROBBIT, timePressureSource, sensesMode: "feed" },
      {
        config: { agents: { entries: { main: { decisionModel: "example-judge" } } } },
        runtime: { decisions: { evaluate: async (request: { questions: Record<string, unknown> }) => {
          assert.deepEqual(request.questions.time_pressure, {
            type: "score", instructions: "How urgent is the user's message?", criteria: ["none", "low", "moderate", "high", "urgent"],
          });
          return { status: "ok", result: { answers: { time_pressure: { score: 3 } } } };
        } } },
      },
    );
    registerStance(host.api);
    assert.ok((await host.promptBuild(cliEvent("fix it now"), groupCtx("urgency"))).result);
    const { event, meta } = logs(dir)[0]!;
    assert.equal(event.time_pressure, timePressureSource === "off" ? undefined : 0.75);
    assert.equal(meta.timePressure, event.time_pressure);
    assert.equal(meta.timePressureSrc, undefined);
    assert.equal(meta.method, "decision-model");
    assert.equal(meta.classifier, undefined);
  }
});
