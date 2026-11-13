import { appendFileSync, readFileSync, statSync } from "node:fs";
import { readTail, STRAND_TAIL_MAX } from "./probbit.js";

/** The source label the plugin puts on every reading it sends (docs/persona.md §2.11, rule S2: the kind must be in `src`). */
export const READING_SRC = "env:probbit-stance";
/** Sense ids this plugin can measure. A persona's other senses are left to other hosts. */
export const FED_SENSES = ["tempo", "tool_fail"] as const;
export type FedSense = (typeof FED_SENSES)[number];

export type DeclaredSense = { kind: "flag" | "number"; range?: [number, number]; src: string[] };

/**
 * Senses a persona declares: the items of its top-level `senses:` block (a YAML subset: `  - id: NAME` items with
 * `kind`, `range: [lo, hi]` and `src: [env]` / `src: env` on their own lines; `src` defaults to [env]).
 * Undefined when the file has no `senses:` block.
 */
export function parseDeclaredSenses(yaml: string): Map<string, DeclaredSense> | undefined {
  const senses = new Map<string, DeclaredSense>();
  let inside = false;
  let sawBlock = false;
  let current: { id: string; kind?: string; range?: [number, number]; src?: string[] } | undefined;
  const flush = () => {
    if (current && (current.kind === "flag" || current.kind === "number")) {
      senses.set(current.id, { kind: current.kind, ...(current.range ? { range: current.range } : {}), src: current.src ?? ["env"] });
    }
    current = undefined;
  };
  for (const line of yaml.split(/\r?\n/u)) {
    if (/^senses:\s*(?:#.*)?$/u.test(line)) {
      inside = true;
      sawBlock = true;
      continue;
    }
    if (!inside) continue;
    if (/^[^\s#]/u.test(line)) break;
    const item = /^\s+-\s+id:\s*["']?([A-Za-z_][\w-]*)["']?/u.exec(line);
    if (item?.[1]) {
      flush();
      current = { id: item[1] };
      continue;
    }
    if (!current) continue;
    const kind = /^\s+kind:\s*["']?(\w+)["']?/u.exec(line);
    if (kind?.[1]) current.kind = kind[1];
    const range = /^\s+range:\s*\[\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*\]/u.exec(line);
    if (range?.[1] && range[2]) current.range = [Number(range[1]), Number(range[2])];
    const src = /^\s+src:\s*(?:\[([^\]]*)\]|(\w+))/u.exec(line);
    if (src) {
      const list = src[1] !== undefined ? src[1].split(",") : [src[2] ?? ""];
      current.src = list.map((s) => s.trim().replace(/^["']|["']$/gu, "")).filter(Boolean);
    }
  }
  flush();
  return sawBlock ? senses : undefined;
}

export type DeclaredSensesCache = Map<string, { mtimeMs: number; size: number; senses: Map<string, DeclaredSense> | undefined }>;

/** The persona's declared senses, re-read only when the file's mtime or size changes. Undefined when unreadable or absent. */
export function readDeclaredSenses(personaPath: string, cache: DeclaredSensesCache): Map<string, DeclaredSense> | undefined {
  let st: { mtimeMs: number; size: number };
  try {
    st = statSync(personaPath);
  } catch {
    cache.delete(personaPath);
    return undefined;
  }
  const hit = cache.get(personaPath);
  if (hit && hit.mtimeMs === st.mtimeMs && hit.size === st.size) return hit.senses;
  let senses: Map<string, DeclaredSense> | undefined;
  try {
    senses = parseDeclaredSenses(readFileSync(personaPath, "utf8"));
  } catch {
    senses = undefined;
  }
  cache.set(personaPath, { mtimeMs: st.mtimeMs, size: st.size, senses });
  return senses;
}

/** Why a sense the plugin measures is or is not fed under this persona (undefined = it is fed). */
export function feedRefusal(id: FedSense, declared: Map<string, DeclaredSense> | undefined): string | undefined {
  const s = declared?.get(id);
  if (!s) return "not declared";
  const want = id === "tempo" ? "number" : "flag";
  if (s.kind !== want) return `declared as ${s.kind}, the plugin measures a ${want}`;
  if (!s.src.includes("env")) return "its src does not include env";
  return undefined;
}

/** One hour in the strand's unit: elapsed_hours is stamped to 1e-6 h, so whole micro-hours add up exactly. */
const WINDOW_MICRO_HOURS = 1_000_000;

export function microHours(hours: number): number {
  return Number.isFinite(hours) && hours > 0 ? Math.round(hours * 1e6) : 0;
}

/**
 * The tempo rule. With u_k = round(1e6 × elapsed_hours_k) for the strand's events 1..n (the current event last):
 * tempo_n = #{ m ≤ n : u_(m+1) + … + u_n ≤ 1 000 000 }, i.e. this event plus every earlier event at most 60 minutes before it.
 * Pure: a replay recomputes it from the strand's own `inputs.elapsed_hours`.
 */
export function tempoFromGaps(elapsedHours: number[]): number {
  let count = 0;
  let since = 0;
  for (let i = elapsedHours.length - 1; i >= 0; i--) {
    count += 1;
    since += microHours(elapsedHours[i] ?? 0);
    if (since > WINDOW_MICRO_HOURS) break;
  }
  return count;
}

/**
 * `inputs.elapsed_hours` of the strand's events, oldest first, from the last 512 KiB (about 1000 events). A large
 * checkpoint line can take most of that window; then the read widens (x4, up to 16 MiB) until it holds 1,000 events,
 * an hour of gaps, or the whole file, so the events before the checkpoint still count.
 */
export function strandGaps(strandPath: string): number[] {
  let size = 0;
  try {
    size = statSync(strandPath).size;
  } catch {
    return [];
  }
  for (let window = 512 * 1024; ; window = Math.min(window * 4, STRAND_TAIL_MAX)) {
    const gaps = gapsIn(readTail(strandPath, window));
    if (enough(gaps) || window >= size || window >= STRAND_TAIL_MAX) return gaps;
  }
}

/** 1,000 events, or more than an hour of gaps: enough history for the tempo rule. */
function enough(gaps: number[]): boolean {
  return gaps.length >= 1_000 || gaps.reduce((s, h) => s + microHours(h), 0) > WINDOW_MICRO_HOURS;
}

function gapsIn(tail: string): number[] {
  const gaps: number[] = [];
  for (const line of tail.split("\n")) {
    if (!line.startsWith('{"')) continue;
    try {
      const obj = JSON.parse(line) as { n?: unknown; inputs?: { elapsed_hours?: unknown } };
      if (typeof obj.n !== "number") continue;
      const h = obj.inputs?.elapsed_hours;
      gaps.push(typeof h === "number" ? h : 0);
    } catch {
      // a line cut by the tail window, or the header
    }
  }
  return gaps;
}

/** This event's tempo: the strand's past events plus the one about to be written (elapsed `hours`). */
export function tempoNow(strandPath: string, hours: number): number {
  return tempoFromGaps([...strandGaps(strandPath), hours]);
}

export type TimePressureFeatures = { deadline?: "strong" | "soft"; imperative?: boolean; emphasis?: boolean; short?: boolean };

const STRONG_DEADLINE =
  /\b(asap|urgent(?:ly)?|immediately|right (?:now|away)|hurry|deadline|eod|by (?:tonight|today|tomorrow|noon|midnight|\d{1,2}(?::\d{2})?\s*(?:am|pm)?)\b|in \d+\s*(?:min(?:ute)?s?|h(?:ou)?rs?)\b|before (?:the )?(?:launch|demo|call|meeting|deploy|release))/iu;
const SOFT_DEADLINE = /\b(quick(?:ly)?|soon|fast|today|tonight|now)\b/iu;
const IMPERATIVES = new Set(
  (
    "add build cancel check delete deploy do finish fix kill make merge move push remove restart revert rollback roll rotate " +
    "run send ship start stop update write"
  ).split(" "),
);

/**
 * The time-pressure heuristic: a fixed mapping from features of the message text to [0, 1], 2 decimals. A mechanism, not a
 * judgement of the person: strong deadline words +0.4 (else soft ones +0.2); an imperative first word (after an optional
 * "please") +0.2; emphasis ("!" or a fully capitalised word of 3+ letters) +0.1; at most 8 words +0.1, only when another
 * feature fired. Highest possible value 0.8; a plain message reads 0.
 */
export function heuristicTimePressure(text: string): { value: number; features: TimePressureFeatures } {
  const features: TimePressureFeatures = {};
  const words = text.split(/\s+/u).filter((w) => /[\p{L}\p{N}]/u.test(w));
  let v = 0;
  if (STRONG_DEADLINE.test(text)) {
    features.deadline = "strong";
    v += 0.4;
  } else if (SOFT_DEADLINE.test(text)) {
    features.deadline = "soft";
    v += 0.2;
  }
  const lead = words.map((w) => w.toLowerCase().replace(/[^\p{L}]/gu, "")).filter(Boolean);
  const first = lead[0] === "please" ? lead[1] : lead[0];
  if (first && IMPERATIVES.has(first)) {
    features.imperative = true;
    v += 0.2;
  }
  if (text.includes("!") || words.some((w) => /^[^\p{L}]*\p{Lu}{3,}[^\p{L}]*$/u.test(w))) {
    features.emphasis = true;
    v += 0.1;
  }
  if (v > 0 && words.length <= 8) {
    features.short = true;
    v += 0.1;
  }
  return { value: Math.round(Math.min(1, v) * 100) / 100, features };
}

/** One line of `strands/<lane>.senses.jsonl`: what the plugin measured at event `n`, where each value came from, what it fed. */
export type SensesLine = {
  n: number;
  ts: string;
  mode: "shadow" | "feed";
  tempo: number;
  tempo_src: "strand";
  time_pressure: number | null;
  time_pressure_src: "question" | "heuristic" | "none";
  time_pressure_heuristic: number;
  time_pressure_features: TimePressureFeatures;
  tool_fail: boolean;
  tool_fail_src: "after_tool_call";
  tool_calls: number;
  tool_errors: number;
  /** Readings this persona would take (declared, right kind, src env); the rest are logged only. */
  declared: string[];
  /** What this event's line carried beyond the flags and the question's time_pressure: nothing in shadow. */
  fed: string[];
};

export function appendSensesLine(path: string, line: SensesLine): void {
  appendFileSync(path, `${JSON.stringify(line)}\n`);
}
