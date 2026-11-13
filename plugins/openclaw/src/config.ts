import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { LABEL_RULES, labelProblem, MAX_FREE_LABELS, parseLabelsConfig, RESERVED_LABELS, type Labels } from "./labels.js";

/** The persona shipped with the plugin (`personas/assistant.yaml`, next to `dist/`): the default when `personaPath` is unset. */
export function bundledPersonaPath(): string {
  return fileURLToPath(new URL("../../personas/assistant.yaml", import.meta.url));
}

export type GateMode = "shadow" | "off";
export type InjectTarget = "appendContext" | "appendSystemContext";
/** auto: the host decision role when an agent decisionModel is configured, else keywords. keyword: keywords only. */
export type ClassifierMode = "auto" | "keyword";
/** off: no readings (default). shadow: compute them and log them to strands/<lane>.senses.jsonl, feed nothing. feed: send them. */
export type SensesMode = "off" | "shadow" | "feed";
/** question: the urgency question's answer only (default). heuristic: that answer, else the text heuristic. off: never. */
export type TimePressureSource = "question" | "heuristic" | "off";

/** One entry of the destructive-action list the shadow gate checks. */
export type DestructiveRule = {
  id: string;
  /** Canonical OpenClaw tool ids the rule applies to (Claude Code's Bash arrives as `exec`). */
  tools: string[];
  /** Regex over a string param (`param`, default `command`). */
  regex?: string;
  param?: string;
  /** Path prefix for file-writing tools; `~` expands to the home directory. */
  pathPrefix?: string;
};

export type StanceConfig = {
  /** Exact session key, or a prefix ending in `*`, mapped to a lane name. Unlisted sessions are untouched. */
  lanes: Record<string, string>;
  personaPath: string;
  stateDir: string;
  binary: string;
  seed: number;
  timeoutMs: number;
  gateMode: GateMode;
  footer: boolean;
  /** Opens the footer line (default: the persona's name as the stance document reports it). */
  footerPrefix?: string;
  /** The block's glyph line (default true): tells the model to use the engine's glyph as the reply's one mood emoji. */
  glyphInReply: boolean;
  inject: InjectTarget;
  classifierTimeoutMs: number;
  decisionThreshold: number;
  classifier: ClassifierMode;
  destructive: DestructiveRule[];
  /** When non-empty, only turns whose ctx.senderId is listed produce events (group members other than the operator are skipped). */
  senderIds: string[];
  /** The readings this plugin measures (tempo, tool_fail, the heuristic time_pressure): off, shadow (logged only) or feed. */
  sensesMode: SensesMode;
  /** Where time_pressure comes from; a heuristic value reaches the engine only with sensesMode feed. */
  timePressureSource: TimePressureSource;
  /** The `agent` label of every lane (default: the agent id of the session key). */
  agentName?: string;
  /** Labels added to every lane's sidecar; a reserved key (agent, host, lane, session) overrides the computed value. */
  labels: Labels;
};

export const SHELL_TOOLS = ["exec", "bash", "shell"];
export const WRITE_TOOLS = ["write", "edit", "multi_edit", "apply_patch", "notebook_edit"];

export const DEFAULT_DESTRUCTIVE: DestructiveRule[] = [
  {
    id: "rm-rf",
    tools: SHELL_TOOLS,
    regex:
      "(^|[\\s;&|(])(sudo\\s+)?rm(?=[^;&|\\n]*\\s-[a-zA-Z]*[rR]|[^;&|\\n]*\\s--recursive)(?=[^;&|\\n]*\\s-[a-zA-Z]*f|[^;&|\\n]*\\s--force)\\s",
  },
  {
    id: "git-push-force",
    tools: SHELL_TOOLS,
    regex: "\\bgit\\b[^;&|\\n]*\\bpush\\b[^;&|\\n]*\\s(-f|--force|--force-with-lease)(?=\\s|=|$)",
  },
  {
    id: "curl-pipe-shell",
    tools: SHELL_TOOLS,
    regex: "\\b(curl|wget)\\b[^;&\\n]*\\|\\s*(sudo\\s+)?(ba|z|da|k)?sh\\b|\\b(ba|z)?sh\\s+-c\\s+[\"']?\\$\\((curl|wget)\\b",
  },
  {
    id: "openclaw-config-set",
    tools: SHELL_TOOLS,
    regex: "\\bopenclaw\\s+config\\s+(set|unset)\\b",
  },
  {
    id: "shell-write-openclaw-json",
    tools: SHELL_TOOLS,
    regex: "(>|\\btee\\b|\\bsed\\s+-i\\b|\\bmv\\b|\\bcp\\b)[^;&|\\n]*\\.openclaw/openclaw\\.json",
  },
  { id: "write-openclaw-json", tools: WRITE_TOOLS, pathPrefix: "~/.openclaw/openclaw.json" },
];

const LANE_NAME = /^[a-z0-9][a-z0-9._-]{0,63}$/;

export function expandHome(p: string, home = homedir()): string {
  return p === "~" ? home : p.startsWith("~/") ? join(home, p.slice(2)) : p;
}

/** warnings: a field fell back (the schema check fails). notices: logged at start, not a schema failure (dropped labels). */
export type ResolvedConfig = { config: StanceConfig; warnings: string[]; notices: string[] };

/** Resolves `plugins.entries.probbit-stance.config` with defaults. Never throws; bad fields fall back and warn. */
export function resolveConfig(raw: unknown, home = homedir()): ResolvedConfig {
  const warnings: string[] = [];
  const notices: string[] = [];
  const src = raw && typeof raw === "object" && !Array.isArray(raw) ? (raw as Record<string, unknown>) : {};
  const str = (key: string, fallback: string): string => {
    const v = src[key];
    if (v === undefined) return fallback;
    if (typeof v === "string" && v.trim()) return v.trim();
    warnings.push(`${key}: expected a non-empty string`);
    return fallback;
  };
  const num = (key: string, fallback: number, min: number, max: number): number => {
    const v = src[key];
    if (v === undefined) return fallback;
    if (typeof v === "number" && Number.isFinite(v) && v >= min && v <= max) return v;
    warnings.push(`${key}: expected a number in [${min}, ${max}]`);
    return fallback;
  };

  const lanes: Record<string, string> = {};
  const rawLanes = src.lanes;
  if (rawLanes && typeof rawLanes === "object" && !Array.isArray(rawLanes)) {
    for (const [key, lane] of Object.entries(rawLanes as Record<string, unknown>)) {
      if (typeof lane === "string" && LANE_NAME.test(lane) && key.length > 0) lanes[key] = lane;
      else warnings.push(`lanes.${key}: lane must match ${LANE_NAME.source}`);
    }
  } else if (rawLanes !== undefined) warnings.push("lanes: expected an object");

  const stateDir = expandHome(str("stateDir", "~/.openclaw/probbit-stance"), home);
  let footerPrefix: string | undefined;
  if (typeof src.footerPrefix === "string" && src.footerPrefix.trim() && [...src.footerPrefix.trim()].length <= 24) footerPrefix = src.footerPrefix.trim();
  else if (src.footerPrefix !== undefined) warnings.push("footerPrefix: expected a non-empty string of at most 24 characters");
  const gateMode = src.gateMode === "off" ? "off" : "shadow";
  if (src.gateMode !== undefined && src.gateMode !== "off" && src.gateMode !== "shadow") warnings.push("gateMode: expected shadow or off");
  const inject = src.inject === "appendSystemContext" ? "appendSystemContext" : "appendContext";
  if (src.inject !== undefined && src.inject !== "appendContext" && src.inject !== "appendSystemContext") {
    warnings.push("inject: expected appendContext or appendSystemContext");
  }
  let footer = true;
  if (typeof src.footer === "boolean") footer = src.footer;
  else if (src.footer !== undefined) warnings.push("footer: expected a boolean");
  let glyphInReply = true;
  if (typeof src.glyphInReply === "boolean") glyphInReply = src.glyphInReply;
  else if (src.glyphInReply !== undefined) warnings.push("glyphInReply: expected a boolean");

  let destructive = DEFAULT_DESTRUCTIVE;
  if (Array.isArray(src.destructive)) {
    destructive = [];
    for (const [i, rule] of (src.destructive as unknown[]).entries()) {
      const r = rule && typeof rule === "object" ? (rule as Record<string, unknown>) : {};
      const tools = Array.isArray(r.tools) ? r.tools.filter((t): t is string => typeof t === "string") : [];
      const ok =
        typeof r.id === "string" &&
        tools.length > 0 &&
        (typeof r.regex === "string" || typeof r.pathPrefix === "string") &&
        (typeof r.regex !== "string" || isValidRegex(r.regex));
      if (!ok) {
        warnings.push(`destructive[${i}]: needs id, tools and a valid regex or pathPrefix`);
        continue;
      }
      destructive.push({
        id: r.id as string,
        tools,
        ...(typeof r.regex === "string" ? { regex: r.regex } : {}),
        ...(typeof r.param === "string" ? { param: r.param } : {}),
        ...(typeof r.pathPrefix === "string" ? { pathPrefix: r.pathPrefix } : {}),
      });
    }
  } else if (src.destructive !== undefined) warnings.push("destructive: expected an array");

  let classifier: ClassifierMode = "auto";
  if (src.classifier === "auto" || src.classifier === "keyword") classifier = src.classifier;
  else if (src.classifier === "jev") {
    classifier = "keyword";
    notices.push('classifier "jev" was removed; using keyword');
  } else if (src.classifier !== undefined) warnings.push("classifier: expected auto or keyword");
  if (src.classifierShadow !== undefined) notices.push("classifierShadow was removed; ignored");
  // The removed adapter's options stay declared: the host rejects unknown keys before the plugin loads.
  if (src.jevModel !== undefined) notices.push("jevModel was removed; ignored");
  if (src.jevKeyFile !== undefined) notices.push("jevKeyFile was removed; ignored");

  let senderIds: string[] = [];
  if (Array.isArray(src.senderIds) && src.senderIds.every((v) => typeof v === "string" && v.length > 0)) senderIds = src.senderIds as string[];
  else if (src.senderIds !== undefined) warnings.push("senderIds: expected an array of non-empty strings");

  let sensesMode: SensesMode = "off";
  if (src.sensesMode === "off" || src.sensesMode === "shadow" || src.sensesMode === "feed") sensesMode = src.sensesMode;
  else if (src.sensesMode !== undefined) warnings.push("sensesMode: expected off, shadow or feed");
  let timePressureSource: TimePressureSource = "question";
  if (src.timePressureSource === "question" || src.timePressureSource === "heuristic" || src.timePressureSource === "off") {
    timePressureSource = src.timePressureSource;
  } else if (src.timePressureSource !== undefined) warnings.push("timePressureSource: expected question, heuristic or off");

  let agentName: string | undefined;
  if (typeof src.agentName === "string" && labelProblem("agent", src.agentName.trim()) === undefined) agentName = src.agentName.trim();
  else if (src.agentName !== undefined) warnings.push("agentName: expected 1-64 characters without control characters");
  const { labels, dropped } = parseLabelsConfig(src.labels);
  if (dropped.length > 0) {
    notices.push(
      `labels: dropped ${dropped.join(", ")} (${LABEL_RULES}; at most ${MAX_FREE_LABELS} keys besides ${RESERVED_LABELS.join(", ")})`,
    );
  }

  return {
    config: {
      lanes,
      personaPath: expandHome(str("personaPath", bundledPersonaPath()), home),
      stateDir,
      binary: expandHome(str("binary", "probbit"), home),
      seed: num("seed", 42, 0, Number.MAX_SAFE_INTEGER),
      timeoutMs: num("timeoutMs", 500, 50, 10_000),
      gateMode,
      footer,
      ...(footerPrefix !== undefined ? { footerPrefix } : {}),
      glyphInReply,
      inject,
      classifierTimeoutMs: num("classifierTimeoutMs", 300, 50, 5_000),
      decisionThreshold: num("decisionThreshold", 0.6, 0, 1),
      classifier,
      destructive,
      senderIds,
      sensesMode,
      timePressureSource,
      ...(agentName !== undefined ? { agentName } : {}),
      labels,
    },
    warnings,
    notices,
  };
}

function isValidRegex(source: string): boolean {
  try {
    new RegExp(source);
    return true;
  } catch {
    return false;
  }
}

/** Exact key first, then the longest `prefix*` entry. */
export function resolveLane(lanes: Record<string, string>, sessionKey: string | undefined): string | undefined {
  if (!sessionKey) return undefined;
  const exact = lanes[sessionKey];
  if (exact) return exact;
  let best: { len: number; lane: string } | undefined;
  for (const [key, lane] of Object.entries(lanes)) {
    if (!key.endsWith("*")) continue;
    const prefix = key.slice(0, -1);
    if (sessionKey.startsWith(prefix) && (!best || prefix.length > best.len)) best = { len: prefix.length, lane };
  }
  return best?.lane;
}

/** JSON Schema mirrored in openclaw.plugin.json (the manifest copy is what the host validates before load). */
export const CONFIG_JSON_SCHEMA = {
  type: "object",
  additionalProperties: false,
  properties: {
    lanes: {
      type: "object",
      description: "Session key (exact) or prefix ending in * → lane name. Only listed sessions get the hooks.",
      additionalProperties: { type: "string", pattern: "^[a-z0-9][a-z0-9._-]{0,63}$" },
    },
    personaPath: { type: "string" },
    stateDir: { type: "string" },
    binary: { type: "string" },
    seed: { type: "integer", minimum: 0 },
    timeoutMs: { type: "integer", minimum: 50, maximum: 10000 },
    gateMode: { type: "string", enum: ["shadow", "off"] },
    footer: { type: "boolean" },
    footerPrefix: {
      type: "string",
      minLength: 1,
      maxLength: 24,
      description: "Opens the footer line (default: the persona's name). An emoji or a short handle works.",
    },
    glyphInReply: {
      type: "boolean",
      description: "Default true: when the persona has display.glyphs, the block tells the model to use the engine's glyph as the reply's one mood emoji. The footer shows the glyph either way.",
    },
    inject: { type: "string", enum: ["appendContext", "appendSystemContext"] },
    classifierTimeoutMs: { type: "integer", minimum: 50, maximum: 5000 },
    decisionThreshold: { type: "number", minimum: 0, maximum: 1 },
    classifier: {
      type: "string",
      oneOf: [
        { enum: ["auto", "keyword"] },
        { const: "jev", deprecated: true, description: "Removed; loads with a warning and uses keywords only." },
      ],
      description: "auto: host decision role when an agent decisionModel is configured, else keywords. keyword: keywords only.",
    },
    classifierShadow: {
      type: "boolean",
      deprecated: true,
      description: "Removed; accepted as a no-op with a warning for old configurations.",
    },
    jevModel: {
      type: "string",
      deprecated: true,
      description: "Removed; accepted as a no-op with a warning for old configurations.",
    },
    jevKeyFile: {
      type: "string",
      deprecated: true,
      description: "Removed; accepted as a no-op with a warning for old configurations. The file is never read.",
    },
    senderIds: { type: "array", items: { type: "string", minLength: 1 } },
    sensesMode: {
      type: "string",
      enum: ["off", "shadow", "feed"],
      description:
        "Default off. shadow: measure tempo, tool_fail and the heuristic time_pressure each event and log them to strands/<lane>.senses.jsonl, feed nothing. feed: send the readings the persona declares.",
    },
    timePressureSource: {
      type: "string",
      enum: ["question", "heuristic", "off"],
      description:
        "Default question: time_pressure only from the urgency question (host decision model in auto mode). heuristic: that answer, else the documented text heuristic (fed only with sensesMode feed). off: never.",
    },
    agentName: {
      type: "string",
      minLength: 1,
      maxLength: 64,
      description: "The agent label of every lane's strand (default: the agent id of the session key).",
    },
    labels: {
      type: "object",
      description:
        "Labels written beside every lane's strand (strands/<lane>.labels.json) for probbit monitor. Keys 1-64 of [A-Za-z0-9_.-], values 1-64 characters; at most 4 keys besides agent, host, lane, session. A reserved key here overrides the value the plugin computes. Invalid entries are dropped with one warning.",
      additionalProperties: { type: "string" },
    },
    destructive: {
      type: "array",
      items: {
        type: "object",
        additionalProperties: false,
        required: ["id", "tools"],
        properties: {
          id: { type: "string" },
          tools: { type: "array", items: { type: "string" }, minItems: 1 },
          regex: { type: "string" },
          param: { type: "string" },
          pathPrefix: { type: "string" },
        },
      },
    },
  },
} as const;
