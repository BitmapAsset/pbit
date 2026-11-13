import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";
import { registerStance } from "../src/index.js";
import { lastStrandEventNumber } from "../src/probbit.js";
import { cliEvent, createHost, GROUP_KEY, groupCtx, PERSONA_SRC, PROBBIT, tempStateDir } from "./host.js";

const LANE = "team-room";
const HEADER = '{"probbit_strand":1,"persona":{"name":"Assistant","version":"1.0.0"},"state":{"seed":42}}';
const event = (n: number) => `{"inputs":{"elapsed_hours":0.5},"n":${n},"prev":"sha256:${"a".repeat(64)}","stance":"sha256:${"b".repeat(64)}"}`;
/** A checkpoint line padded to about `bytes` (a real one grows with the state and the expectation's memory). */
const checkpoint = (n: number, bytes = 0) =>
  `{"checkpoint":${n},"prev":"sha256:${"c".repeat(64)}","stance":{"line":"Stance: steady."},"state":{"pad":"${"x".repeat(bytes)}"}}`;

function strandFile(text: string): string {
  const path = join(tempStateDir(), "lane.strand");
  writeFileSync(path, text);
  return path;
}

test("strand tail: an event line last -> its n; the header alone -> 0", () => {
  assert.equal(lastStrandEventNumber(strandFile(`${HEADER}\n${event(1)}\n${event(2)}\n`)), 2);
  assert.equal(lastStrandEventNumber(strandFile(`${HEADER}\n`)), 0);
});

test("strand tail: a checkpoint line last -> its checkpoint number (the event count)", () => {
  assert.equal(lastStrandEventNumber(strandFile(`${HEADER}\n${event(999)}\n${event(1000)}\n${checkpoint(1000)}\n`)), 1000);
});

test("strand tail: a header longer than 64 KiB and nothing else -> 0", () => {
  const big = `{"probbit_strand":1,"persona":{"name":"Assistant"},"state":{"pad":"${"h".repeat(200 * 1024)}"}}`;
  assert.equal(lastStrandEventNumber(strandFile(`${big}\n`)), 0);
});

test("strand tail: events, then a checkpoint line longer than 64 KiB -> its checkpoint number, not 0", () => {
  const lines = [HEADER, ...Array.from({ length: 50 }, (_, i) => event(i + 1)), checkpoint(50, 300 * 1024)];
  assert.equal(lastStrandEventNumber(strandFile(`${lines.join("\n")}\n`)), 50);
  // and with an event after it the short last line is read from the first window as before
  assert.equal(lastStrandEventNumber(strandFile(`${[...lines, event(51)].join("\n")}\n`)), 51);
});

test("strand tail: a torn last line, an empty file or no file -> undefined", () => {
  assert.equal(lastStrandEventNumber(strandFile(`${HEADER}\n${event(1)}\n{"inputs":{"elapsed_hours":0.5},"n":2,"pr`)), undefined);
  assert.equal(lastStrandEventNumber(strandFile(`${HEADER}\n${checkpoint(3, 100 * 1024).slice(0, 90 * 1024)}`)), undefined);
  assert.equal(lastStrandEventNumber(strandFile("")), undefined);
  assert.equal(lastStrandEventNumber(join(tempStateDir(), "missing.strand")), undefined);
});

test("hook: the event that writes a checkpoint (the 1,000th) still gets its stance, and the next event works", async () => {
  const dir = tempStateDir();
  for (const d of ["state", "strands", "last", "log"]) mkdirSync(join(dir, d), { recursive: true });
  const state = join(dir, "state", `${LANE}.json`);
  const strand = join(dir, "strands", `${LANE}.strand`);
  // 999 events written by the engine itself, in one call; the hook's event is then the 1,000th, where the engine's
  // default (`--checkpoint-every 1000`) appends a checkpoint line after the event line.
  const events = join(dir, "events.jsonl");
  writeFileSync(events, '{"elapsed_hours":0.5}\n'.repeat(999));
  execFileSync(PROBBIT, ["persona", "init", PERSONA_SRC, "--seed", "42", "--out", state], { stdio: "ignore" });
  execFileSync(PROBBIT, ["live", PERSONA_SRC, "--state", state, "--strand", strand, "--clock", "fixed", "--events", events], { stdio: "ignore" });
  const host = createHost({ lanes: { [GROUP_KEY]: LANE }, stateDir: dir, binary: PROBBIT });
  registerStance(host.api);

  const first = await host.promptBuild(cliEvent("thanks"), groupCtx("cp-1"));
  assert.match((first.result as { appendContext?: string } | undefined)?.appendContext ?? "", /^## Assistant stance \(probbit, event 1000\)/, JSON.stringify(host.logs));
  const tail = readFileSync(strand, "utf8").trimEnd().split("\n");
  assert.equal((JSON.parse(tail[tail.length - 1]!) as { checkpoint?: number }).checkpoint, 1000, "the strand ends on the checkpoint line");
  assert.equal((JSON.parse(tail[tail.length - 2]!) as { n?: number }).n, 1000);

  const second = await host.promptBuild(cliEvent("no, that's wrong"), groupCtx("cp-2"));
  assert.match((second.result as { appendContext?: string } | undefined)?.appendContext ?? "", /^## Assistant stance \(probbit, event 1001\)/);
  assert.equal(host.logs.some((l) => l.message.includes("strand unreadable")), false);
  const ns = readFileSync(join(dir, "log", `${LANE}.jsonl`), "utf8").trim().split("\n").map((l) => (JSON.parse(l.split("\t")[3]!) as { n: number }).n);
  assert.deepEqual(ns, [1000, 1001]);
});
