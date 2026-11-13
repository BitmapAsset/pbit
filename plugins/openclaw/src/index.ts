import { createHash } from "node:crypto";
import { appendFileSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import type { OpenClawPluginApi, OpenClawPluginDefinition } from "openclaw/plugin-sdk/plugin-entry";
import { buildBlock, footerInputs, movedNames } from "./block.js";
import {
  extractCurrentMessage,
  classify,
  MARKER_RE,
  sanitize,
  type Classification,
  type DecisionEvaluate,
} from "./classify.js";
import { CONFIG_JSON_SCHEMA, resolveConfig, resolveLane, type StanceConfig } from "./config.js";
import { carefulFromDoc, carefulFromState, detectDestructive, digest, redactPreview } from "./gate.js";
import { labelArgs, laneLabels, liveTakesLabels, syncSidecar } from "./labels.js";
import {
  appendEventLog,
  buildEventLine,
  elapsedHours,
  ensureLaneDirs,
  findLoggedEvent,
  lanePaths,
  readDeclaredInputs,
  readLastEpoch,
  runLiveEvent,
  writeLastEpoch,
  type DeclaredInputsCache,
  type LogMeta,
  type Reading,
  type StanceDoc,
} from "./probbit.js";
import {
  appendSensesLine,
  feedRefusal,
  FED_SENSES,
  heuristicTimePressure,
  READING_SRC,
  readDeclaredSenses,
  tempoNow,
  type DeclaredSensesCache,
} from "./senses.js";
import { checkBinaryVersion } from "./version.js";

export const PLUGIN_ID = "probbit-stance";

type Entry = { n: number; doc: StanceDoc; inputs?: string[]; moved?: string[] };
type Flight = { promise: Promise<Entry | undefined>; state: { late: boolean; warned: boolean } };

export type StanceDeps = {
  /** Wall clock in ms (tests pin it). */
  now?: () => number;
  home?: string;
};

const SEEN_LIMIT = 256;
/** Runs per lane remembered as event turns, so after_tool_call counts only tool calls made while answering a user message. */
const EVENT_RUNS_LIMIT = 16;
const TIMED_OUT = Symbol("timed-out");

function sha(text: string): string {
  return createHash("sha256").update(text).digest("hex");
}

function isoSeconds(ms: number): string {
  return new Date(ms).toISOString().replace(/\.\d{3}Z$/, "Z");
}

function latestUserTimestamp(messages: unknown): number | undefined {
  if (!Array.isArray(messages)) return undefined;
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i] as { role?: unknown; timestamp?: unknown } | null;
    if (m && m.role === "user" && typeof m.timestamp === "number") return m.timestamp;
  }
  return undefined;
}

/** One event per admitted user message: the native admission id when the harness gives one, else text + an anchor. */
export function idempotencyKey(input: {
  lane: string;
  sessionKey: string;
  text: string;
  messageId?: string | undefined;
  runId?: string | undefined;
  messages?: unknown;
  nowMs: number;
}): { key: string; basis: string } {
  if (input.messageId) return { key: `mid:${sha(`${input.lane}\n${input.messageId}`).slice(0, 32)}`, basis: "message-id" };
  let anchor: string;
  let basis: string;
  if (input.runId) {
    anchor = `run:${input.runId}`;
    basis = "text+run-id";
  } else {
    const ts = latestUserTimestamp(input.messages);
    anchor = ts !== undefined ? `ts:${ts}` : `min:${Math.floor(input.nowMs / 60_000)}`;
    basis = ts !== undefined ? "text+message-timestamp" : "text+minute";
  }
  return { key: `h:${sha(`${input.lane}\n${input.sessionKey}\n${anchor}\n${input.text}`).slice(0, 32)}`, basis };
}

function decisionEvaluateFor(api: OpenClawPluginApi, agentId: string | undefined): DecisionEvaluate | undefined {
  const cfg = api.config as unknown as {
    agents?: { defaults?: { decisionModel?: unknown }; entries?: Record<string, { decisionModel?: unknown } | undefined> };
  };
  const perAgent = agentId ? cfg?.agents?.entries?.[agentId]?.decisionModel : undefined;
  const model = perAgent !== undefined ? perAgent : cfg?.agents?.defaults?.decisionModel;
  if (typeof model !== "string" || !model.trim()) return undefined;
  const decisions = (api as unknown as { runtime?: { decisions?: { evaluate?: unknown } } }).runtime?.decisions;
  const evaluate = decisions?.evaluate;
  if (typeof evaluate !== "function") return undefined;
  return (request, options) => (evaluate as DecisionEvaluate).call(decisions, request, options);
}

function raceTimeout<T>(promise: Promise<T>, ms: number): Promise<T | typeof TIMED_OUT> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<typeof TIMED_OUT>((resolve) => {
    timer = setTimeout(() => resolve(TIMED_OUT), ms);
  });
  return Promise.race([promise, timeout]).finally(() => {
    if (timer) clearTimeout(timer);
  });
}

export function registerStance(api: OpenClawPluginApi, deps: StanceDeps = {}): StanceConfig {
  const home = deps.home ?? homedir();
  const now = deps.now ?? (() => Date.now());
  const { config: cfg, warnings, notices } = resolveConfig(api.pluginConfig, home);
  const warn = (message: string): void => {
    try {
      api.logger.warn(`[${PLUGIN_ID}] ${message}`);
    } catch {
      // logging must never break a hook
    }
  };
  for (const w of [...warnings, ...notices]) warn(`config: ${w}`);
  if (Object.keys(cfg.lanes).length === 0) {
    warn("no lanes configured; no hooks registered");
    return cfg;
  }
  // The binary's version, checked at load: an older probbit is named here once, and every event refuses it.
  void checkBinaryVersion(cfg.binary, Math.max(5_000, cfg.timeoutMs * 10)).then((check) => {
    if (!check.ok) warn(check.reason);
  });

  const classifierFor = (agentId: string | undefined): DecisionEvaluate | undefined =>
    cfg.classifier === "auto" ? decisionEvaluateFor(api, agentId) : undefined;

  const seen = new Map<string, Entry>();
  const inflight = new Map<string, Flight>();
  const laneChains = new Map<string, Promise<unknown>>();
  const lastByLane = new Map<string, Entry>();
  const declaredCache: DeclaredInputsCache = new Map();
  const warnedFlags = new Set<string>();
  let warnedUnreadable = false;
  const sensesCache: DeclaredSensesCache = new Map();
  const warnedSenses = new Set<string>();
  /** Tool calls (and failed ones) seen by after_tool_call per lane since the lane's previous event. */
  const toolTally = new Map<string, { calls: number; errors: number }>();
  const eventRuns = new Map<string, string[]>();
  /** The host's version for the `host` label (`OpenClaw <version>`), when the runtime exposes it. */
  let hostVersion: string | undefined;
  try {
    const v = (api as unknown as { runtime?: { version?: unknown } }).runtime?.version;
    if (typeof v === "string") hostVersion = v;
  } catch {
    hostVersion = undefined;
  }
  const warnedLabels = new Set<string>();
  const warnedSealed = new Set<string>();
  // The persona's senses are read once here; the cache re-reads the file only when it changes.
  if (cfg.sensesMode !== "off") readDeclaredSenses(cfg.personaPath, sensesCache);

  /** Version skew, both ways: only flags the persona declares reach probbit; one warn per unknown flag per process. */
  const declaredOnly = (flags: string[]): { kept: string[]; dropped: string[] } => {
    const ids = readDeclaredInputs(cfg.personaPath, declaredCache);
    if (!ids) {
      if (!warnedUnreadable) {
        warnedUnreadable = true;
        warn(`persona inputs unreadable at ${cfg.personaPath}; flags passed through unfiltered`);
      }
      return { kept: flags, dropped: [] };
    }
    const kept: string[] = [];
    const dropped: string[] = [];
    for (const f of flags) (ids.has(f) ? kept : dropped).push(f);
    for (const f of dropped) {
      if (warnedFlags.has(f)) continue;
      warnedFlags.add(f);
      warn(`persona does not declare input "${f}"; flag dropped (warned once per process)`);
    }
    return { kept, dropped };
  };

  const remember = (lane: string, key: string, entry: Entry): void => {
    seen.set(key, entry);
    if (seen.size > SEEN_LIMIT) {
      const oldest = seen.keys().next().value;
      if (oldest !== undefined) seen.delete(oldest);
    }
    const prev = lastByLane.get(lane);
    if (!prev || entry.n >= prev.n) lastByLane.set(lane, entry);
  };

  const withLaneLock = <T>(lane: string, work: () => Promise<T>): Promise<T> => {
    const prev = laneChains.get(lane) ?? Promise.resolve();
    const next = prev.then(work, work);
    laneChains.set(
      lane,
      next.catch(() => undefined),
    );
    return next;
  };

  const inject = (entry: Entry) => {
    const block = buildBlock(
      entry.n,
      entry.doc,
      cfg.footer,
      {
        ...(entry.inputs ? { inputs: entry.inputs } : {}),
        ...(entry.moved ? { moved: entry.moved } : {}),
      },
      cfg.footerPrefix ? { prefix: cfg.footerPrefix } : {},
      cfg.glyphInReply,
    );
    return cfg.inject === "appendSystemContext" ? { appendSystemContext: block } : { appendContext: block };
  };

  const takeEvent = async (
    lane: string,
    sessionKey: string,
    key: string,
    keyBasis: string,
    text: string,
    textSource: string,
    agentId: string | undefined,
    state: Flight["state"],
  ): Promise<Entry | undefined> => {
    const paths = lanePaths(cfg.stateDir, lane);
    ensureLaneDirs(cfg.stateDir);
    const logged = findLoggedEvent(paths.log, key);
    if (logged) return logged;
    const evaluate = classifierFor(agentId);
    const fed: Classification = await classify(text, {
      ...(evaluate ? { evaluate } : {}),
      ...(agentId ? { agentId } : {}),
      timeoutMs: cfg.classifierTimeoutMs,
      threshold: cfg.decisionThreshold,
    });
    const nowMs = now();
    const nowSec = Math.floor(nowMs / 1000);
    const hours = elapsedHours(nowSec, readLastEpoch(paths.last));
    const { kept: flags, dropped } = declaredOnly(fed.flags);
    const declared = readDeclaredInputs(cfg.personaPath, declaredCache);
    const tpDeclared = declared?.has("time_pressure") === true;
    const questionTp = fed.timePressure !== undefined && fed.timePressure >= 0.05 ? fed.timePressure : undefined;
    let timePressure = cfg.timePressureSource !== "off" && tpDeclared ? questionTp : undefined;
    let timePressureSrc: "heuristic" | undefined;
    const fedSenses: Record<string, Reading> = {};
    let sensesLine: Omit<Parameters<typeof appendSensesLine>[1], "n"> | undefined;
    let tally: { calls: number; errors: number } | undefined;
    if (cfg.sensesMode !== "off") {
      tally = toolTally.get(lane) ?? { calls: 0, errors: 0 };
      toolTally.delete(lane);
      const heur = heuristicTimePressure(sanitize(text).replace(MARKER_RE, " "));
      const chosen =
        cfg.timePressureSource === "off"
          ? undefined
          : questionTp !== undefined
            ? { v: questionTp, src: "question" as const }
            : cfg.timePressureSource === "heuristic" && heur.value >= 0.05
              ? { v: heur.value, src: "heuristic" as const }
              : undefined;
      const tempo = tempoNow(paths.strand, hours);
      const toolFail = tally.errors > 0;
      const senses = readDeclaredSenses(cfg.personaPath, sensesCache);
      const takes: string[] = [];
      for (const id of FED_SENSES) {
        const refusal = feedRefusal(id, senses);
        if (refusal === undefined) takes.push(id);
        else if (cfg.sensesMode === "feed" && !warnedSenses.has(id)) {
          warnedSenses.add(id);
          warn(`sense "${id}": ${refusal} in the persona; not fed (warned once per process)`);
        }
      }
      if (tpDeclared) takes.push("time_pressure");
      const fedNow: string[] = [];
      if (cfg.sensesMode === "feed") {
        if (takes.includes("tempo")) {
          const range = senses?.get("tempo")?.range;
          fedSenses.tempo = { v: range ? Math.min(range[1], Math.max(range[0], tempo)) : tempo, src: READING_SRC };
          fedNow.push("tempo");
        }
        if (takes.includes("tool_fail")) {
          fedSenses.tool_fail = { v: toolFail, src: READING_SRC };
          fedNow.push("tool_fail");
        }
        if (chosen?.src === "heuristic" && tpDeclared) {
          timePressure = chosen.v;
          timePressureSrc = "heuristic";
          fedNow.push("time_pressure");
        }
      }
      sensesLine = {
        ts: isoSeconds(now()),
        mode: cfg.sensesMode,
        tempo,
        tempo_src: "strand",
        time_pressure: chosen?.v ?? null,
        time_pressure_src: chosen?.src ?? "none",
        time_pressure_heuristic: heur.value,
        time_pressure_features: heur.features,
        tool_fail: toolFail,
        tool_fail_src: "after_tool_call",
        tool_calls: tally.calls,
        tool_errors: tally.errors,
        declared: takes,
        fed: fedNow,
      };
    }
    const eventLine = buildEventLine(flags, hours, timePressure !== undefined ? { time_pressure: timePressure } : {}, fedSenses);
    const labels = laneLabels({ lane, sessionKey, hostVersion, agentName: cfg.agentName, extra: cfg.labels });
    const killAfterMs = Math.max(5_000, cfg.timeoutMs * 10);
    const run = await runLiveEvent({
      binary: cfg.binary,
      personaPath: cfg.personaPath,
      seed: cfg.seed,
      paths,
      eventLine,
      killAfterMs,
      // A new strand: the CLI writes the sidecar when it takes --label (probed once per process); else the plugin does below.
      newStrandArgs: async () => ((await liveTakesLabels(cfg.binary, killAfterMs)) ? labelArgs(labels) : []),
      // probbit older than MIN_PROBBIT: no event at all (probed once per process, at load).
      binaryCheck: async () => {
        const check = await checkBinaryVersion(cfg.binary, killAfterMs);
        return check.ok ? undefined : check.reason;
      },
    });
    if (!run.ok) {
      if (tally && (tally.calls > 0 || tally.errors > 0)) {
        // The event did not happen: its tool calls count toward the next one.
        const next = toolTally.get(lane) ?? { calls: 0, errors: 0 };
        toolTally.set(lane, { calls: next.calls + tally.calls, errors: next.errors + tally.errors });
      }
      if (run.sealed) {
        // Paused until the operator acts: one warning per lane, not one per turn.
        if (!warnedSealed.has(lane)) {
          warnedSealed.add(lane);
          warn(`lane ${lane}: no stance this turn (${run.reason}; warned once per lane)`);
        }
        return undefined;
      }
      if (!state.warned) {
        state.warned = true;
        warn(`lane ${lane}: no stance this turn (${run.reason})`);
      }
      return undefined;
    }
    writeLastEpoch(paths.last, nowSec);
    if (!run.labeled) {
      try {
        syncSidecar(paths.strand, labels);
      } catch (err) {
        if (!warnedLabels.has(lane)) {
          warnedLabels.add(lane);
          warn(`lane ${lane}: labels not written (${String(err).slice(0, 200)}; warned once per lane)`);
        }
      }
    }
    if (sensesLine) {
      try {
        appendSensesLine(join(cfg.stateDir, "strands", `${lane}.senses.jsonl`), { n: run.n, ...sensesLine });
      } catch (err) {
        warn(`lane ${lane}: senses line not written (${String(err).slice(0, 200)})`);
      }
    }
    const inputs = footerInputs(flags, timePressure);
    const moved = movedNames(lastByLane.get(lane)?.doc, run.doc);
    const meta: LogMeta = {
      src: PLUGIN_ID,
      n: run.n,
      key,
      keyBasis,
      method: fed.method,
      flags,
      textSource,
      ms: run.ms,
      ...(state.late ? { late: true } : {}),
      ...(fed.probabilities ? { probabilities: fed.probabilities as Record<string, number> } : {}),
      ...(fed.decisionNote ? { decisionNote: fed.decisionNote } : {}),
      ...(dropped.length > 0 ? { dropped } : {}),
      ...(timePressure !== undefined ? { timePressure } : {}),
      ...(timePressureSrc ? { timePressureSrc } : {}),
      ...(Object.keys(fedSenses).length > 0
        ? { senses: Object.fromEntries(Object.entries(fedSenses).map(([id, r]) => [id, r.v])) }
        : {}),
      inputs,
      ...(moved ? { moved } : {}),
    };
    appendEventLog(paths.log, isoSeconds(nowMs), run.eventLine, run.outLine, meta);
    return { n: run.n, doc: run.doc, inputs, ...(moved ? { moved } : {}) };
  };

  api.on("before_prompt_build", async (event, ctx) => {
    try {
      const sessionKey = ctx?.sessionKey;
      const lane = resolveLane(cfg.lanes, sessionKey);
      if (!lane || !sessionKey) return;
      if (ctx.trigger === "heartbeat" || ctx.trigger === "cron") return;
      if (ctx.inputProvenance && ctx.inputProvenance.kind !== "external_user") return;
      if (cfg.senderIds.length > 0 && !(typeof ctx.senderId === "string" && cfg.senderIds.includes(ctx.senderId))) return;
      if (cfg.sensesMode !== "off" && typeof ctx.runId === "string") {
        const runs = eventRuns.get(lane) ?? [];
        if (!runs.includes(ctx.runId)) {
          runs.push(ctx.runId);
          if (runs.length > EVENT_RUNS_LIMIT) runs.shift();
          eventRuns.set(lane, runs);
        }
      }
      const hasCurrent = typeof event?.currentUserMessage === "string";
      let text: string;
      let textSource: string;
      if (hasCurrent) {
        text = event.currentUserMessage as string;
        textSource = "currentUserMessage";
      } else {
        const extracted = extractCurrentMessage(typeof event?.prompt === "string" ? event.prompt : "");
        text = extracted.text;
        textSource = extracted.source;
      }
      const { key, basis } = idempotencyKey({
        lane,
        sessionKey,
        text,
        messageId: event?.currentUserMessageId,
        runId: ctx.runId,
        messages: event?.messages,
        nowMs: now(),
      });
      const cached = seen.get(key);
      if (cached) return inject(cached);

      let flight = inflight.get(key);
      const joined = flight !== undefined;
      if (!flight) {
        try {
          ctx.hookInvocation?.assertActive();
        } catch {
          return;
        }
        const state = { late: false, warned: false };
        const promise = withLaneLock(lane, () =>
          takeEvent(lane, sessionKey, key, basis, text, textSource, ctx.agentId, state),
        )
          .then(
            (entry) => {
              if (entry) remember(lane, key, entry);
              return entry;
            },
            (err: unknown) => {
              if (!state.warned) {
                state.warned = true;
                warn(`lane ${lane}: no stance this turn (${String(err).slice(0, 200)})`);
              }
              return undefined;
            },
          )
          .finally(() => inflight.delete(key));
        flight = { promise, state };
        inflight.set(key, flight);
      }
      const budget = cfg.timeoutMs + (classifierFor(ctx.agentId) ? cfg.classifierTimeoutMs + 50 : 0);
      const entry = await raceTimeout(flight.promise, budget);
      if (entry === TIMED_OUT) {
        flight.state.late = true;
        if (!joined && !flight.state.warned) {
          flight.state.warned = true;
          warn(`lane ${lane}: probbit did not answer within ${budget} ms; no stance this turn`);
        }
        return;
      }
      return entry ? inject(entry) : undefined;
    } catch (err) {
      warn(`before_prompt_build skipped: ${String(err).slice(0, 200)}`);
      return;
    }
  });

  if (cfg.gateMode === "shadow") {
    let gateDirReady = false;
    api.on("before_tool_call", (event, ctx) => {
      try {
        const lane = resolveLane(cfg.lanes, ctx?.sessionKey);
        if (!lane) return;
        const params = event?.params && typeof event.params === "object" ? (event.params as Record<string, unknown>) : {};
        const cwdRaw = (ctx as unknown as { cwd?: unknown }).cwd;
        const toolName = typeof event?.toolName === "string" ? event.toolName : typeof ctx?.toolName === "string" ? ctx.toolName : "";
        const hit = detectDestructive(toolName, params, event?.derivedPaths, cfg.destructive, {
          home,
          ...(typeof cwdRaw === "string" ? { cwd: cwdRaw } : {}),
        });
        if (!hit) return;
        const paths = lanePaths(cfg.stateDir, lane);
        const fromState = carefulFromState(paths.state);
        const mem = lastByLane.get(lane);
        const check = mem && (fromState.source !== "state" || fromState.n === undefined || fromState.n === mem.n) ? carefulFromDoc(mem.doc, mem.n) : fromState;
        if (!check.careful) return;
        if (!gateDirReady) {
          mkdirSync(dirname(paths.gateLog), { recursive: true });
          gateDirReady = true;
        }
        const line = {
          ts: isoSeconds(now()),
          mode: "shadow",
          would: "deny",
          decided: "allow",
          lane,
          class: "destructive",
          policy: "actions.classes.destructive.deny_when[0]",
          rule: hit.rule,
          tool: toolName,
          ...(event?.toolCallId ? { toolCallId: event.toolCallId } : {}),
          ...(ctx?.runId ? { runId: ctx.runId } : {}),
          stance: {
            source: check.source,
            ...(check.n !== undefined ? { n: check.n } : {}),
            ...(check.caution ? { caution: check.caution } : {}),
            securityCareful: check.securityCareful,
          },
          detail: { kind: hit.detailKind, digest: digest(hit.detail), preview: redactPreview(hit.detail) },
        };
        appendFileSync(paths.gateLog, `${JSON.stringify(line)}\n`);
      } catch (err) {
        warn(`before_tool_call shadow gate skipped: ${String(err).slice(0, 200)}`);
      }
      return;
    });
  }
  if (cfg.sensesMode !== "off") {
    // tool_fail: the reading at event n is true when a tool call made in the lane since event n-1 reported an error. The
    // hook observes; it never changes a result. Calls from runs that produced no event (cron, heartbeat, an unlisted
    // sender) are not counted; a call without a run id is.
    api.on("after_tool_call", (event, ctx) => {
      try {
        const lane = resolveLane(cfg.lanes, ctx?.sessionKey);
        if (!lane) return;
        const runId = ctx?.runId ?? event?.runId;
        if (runId !== undefined && !(eventRuns.get(lane) ?? []).includes(runId)) return;
        const t = toolTally.get(lane) ?? { calls: 0, errors: 0 };
        t.calls += 1;
        if (typeof event?.error === "string" && event.error.trim()) t.errors += 1;
        toolTally.set(lane, t);
      } catch (err) {
        warn(`after_tool_call skipped: ${String(err).slice(0, 200)}`);
      }
      return;
    });
  }
  return cfg;
}

const plugin = {
  id: PLUGIN_ID,
  name: "probbit stance",
  description: "Injects the lane's probbit stance into each turn and shadow-logs destructive tool calls a careful stance would deny.",
  configSchema: {
    safeParse(value: unknown) {
      const { warnings } = resolveConfig(value);
      if (warnings.length > 0) {
        return { success: false, error: { issues: warnings.map((message) => ({ path: [] as Array<string | number>, message })) } };
      }
      return { success: true, data: value };
    },
    jsonSchema: CONFIG_JSON_SCHEMA as unknown as NonNullable<NonNullable<OpenClawPluginDefinition["configSchema"]>["jsonSchema"]>,
  },
  register(api: OpenClawPluginApi) {
    registerStance(api);
  },
} satisfies OpenClawPluginDefinition;

export default plugin;
