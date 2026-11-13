import { spawn } from "node:child_process";
import { closeSync, existsSync, fstatSync, mkdirSync, openSync, readFileSync, readSync, rmSync, statSync, writeFileSync, appendFileSync } from "node:fs";
import { join } from "node:path";

/** The part of `probbit live`'s stance document this plugin reads (persona.md §4/§5.7). */
export type StanceDoc = {
  line?: string;
  status?: string;
  turn?: number;
  stance?: Record<string, { level?: string; odds?: Record<string, number> }>;
  mood?: Record<string, { level?: string; p?: number }>;
  habits?: { active?: string[]; bound?: string[]; violations?: number };
  /** The persona the stance was made under (name, version, digest), as `probbit live` prints it. */
  persona?: { name?: string; version?: string; digest?: string };
  why?: unknown;
  engine?: { verdict?: string };
  error?: unknown;
  /** The engine's glyph for this stance (persona.md §2.18, `display.glyphs`): computed from the document, logged on the strand. */
  glyph?: string;
  /** Which rule gave the glyph: `habit <id>` or `mood <a>=<l> <b>=<l>`, then ` +surprise z=<z>` with the modifier. */
  glyph_why?: string;
};

/**
 * Input ids a persona declares: the `  - id: NAME` items of its top-level `inputs:` block (a YAML subset; flow items
 * `- {id: NAME, ...}` too). Undefined when the file has no `inputs:` block or the block declares nothing.
 */
export function parseDeclaredInputs(yaml: string): Set<string> | undefined {
  const ids = new Set<string>();
  let inside = false;
  let sawBlock = false;
  for (const line of yaml.split(/\r?\n/u)) {
    if (/^inputs:\s*(?:#.*)?$/u.test(line)) {
      inside = true;
      sawBlock = true;
      continue;
    }
    if (!inside) continue;
    if (/^[^\s#]/u.test(line)) break;
    const m = /^\s+-\s+(?:\{\s*)?id:\s*["']?([A-Za-z_][\w-]*)["']?/u.exec(line);
    if (m?.[1]) ids.add(m[1]);
  }
  return sawBlock && ids.size > 0 ? ids : undefined;
}

export type DeclaredInputsCache = Map<string, { mtimeMs: number; size: number; ids: Set<string> | undefined }>;

/** The persona's declared input ids, re-read only when the file's mtime or size changes. Undefined when unreadable. */
export function readDeclaredInputs(personaPath: string, cache: DeclaredInputsCache): Set<string> | undefined {
  let st: { mtimeMs: number; size: number };
  try {
    st = statSync(personaPath);
  } catch {
    cache.delete(personaPath);
    return undefined;
  }
  const hit = cache.get(personaPath);
  if (hit && hit.mtimeMs === st.mtimeMs && hit.size === st.size) return hit.ids;
  let ids: Set<string> | undefined;
  try {
    ids = parseDeclaredInputs(readFileSync(personaPath, "utf8"));
  } catch {
    ids = undefined;
  }
  cache.set(personaPath, { mtimeMs: st.mtimeMs, size: st.size, ids });
  return ids;
}

export type LanePaths = { state: string; strand: string; last: string; log: string; gateLog: string };

/** Stage-0 layout: state/<lane>.json, strands/<lane>.strand, last/<lane>.ts, log/<lane>.jsonl. */
export function lanePaths(stateDir: string, lane: string): LanePaths {
  return {
    state: join(stateDir, "state", `${lane}.json`),
    strand: join(stateDir, "strands", `${lane}.strand`),
    last: join(stateDir, "last", `${lane}.ts`),
    log: join(stateDir, "log", `${lane}.jsonl`),
    gateLog: join(stateDir, "log", `${lane}-gate.jsonl`),
  };
}

export function ensureLaneDirs(stateDir: string): void {
  for (const d of ["state", "strands", "last", "log"]) mkdirSync(join(stateDir, d), { recursive: true });
}

/** Hours since the lane's previous event, as stage 0 stamps it: max(0, Δs / 3600) rounded to 1e-6 h; 0 without a previous event. */
export function elapsedHours(nowSec: number, lastSec: number | undefined): number {
  if (lastSec === undefined || !Number.isFinite(lastSec) || !Number.isFinite(nowSec)) return 0;
  const h = Math.max(0, (nowSec - lastSec) / 3600);
  return Math.round(h * 1e6) / 1e6;
}

export function readLastEpoch(path: string): number | undefined {
  try {
    const raw = readFileSync(path, "utf8").trim();
    if (!/^\d{1,12}$/.test(raw)) return undefined;
    return Number(raw);
  } catch {
    return undefined;
  }
}

/** Reads the last `maxBytes` of a file as text (whole lines only, except possibly the first). */
export function readTail(path: string, maxBytes: number): string {
  let fd: number | undefined;
  try {
    fd = openSync(path, "r");
    const size = fstatSync(fd).size;
    const len = Math.min(size, maxBytes);
    const buf = Buffer.alloc(len);
    readSync(fd, buf, 0, len, size - len);
    return buf.toString("utf8");
  } catch {
    return "";
  } finally {
    if (fd !== undefined) closeSync(fd);
  }
}

/** Event number of the strand's last line (`n`); 0 when only the header exists. */
export function lastStrandEventNumber(strandPath: string): number | undefined {
  const tail = readTail(strandPath, 64 * 1024);
  const lines = tail.split("\n").filter((l) => l.trim());
  const last = lines[lines.length - 1];
  if (!last) return undefined;
  try {
    const obj = JSON.parse(last) as { n?: unknown; probbit_strand?: unknown };
    if (typeof obj.n === "number") return obj.n;
    if (obj.probbit_strand !== undefined) return 0;
  } catch {
    // a header longer than the tail window: the first line is cut, so the strand has no events yet
    if (lines.length === 1) return 0;
  }
  return undefined;
}

/** A sense reading with its own source (docs/persona.md §2.11: `{v, src}`). */
export type Reading = { v: number | boolean; src: string };

/**
 * Canonical-enough event line: sorted keys, true flags only, number inputs (time_pressure) when given, elapsed_hours always
 * present, and `senses` (sorted, each `{src, v}`) only when there is a reading to send.
 */
export function buildEventLine(
  flags: string[],
  hours: number,
  numbers: Record<string, number> = {},
  senses: Record<string, Reading> = {},
): string {
  const ev: Record<string, unknown> = {};
  for (const f of flags) ev[f] = true;
  for (const [k, v] of Object.entries(numbers)) if (Number.isFinite(v)) ev[k] = v;
  ev.elapsed_hours = hours;
  const ids = Object.keys(senses).sort();
  if (ids.length > 0) {
    const out: Record<string, { src: string; v: number | boolean }> = {};
    for (const id of ids) {
      const r = senses[id] as Reading;
      out[id] = { src: r.src, v: r.v };
    }
    ev.senses = out;
  }
  const sorted: Record<string, unknown> = {};
  for (const k of Object.keys(ev).sort()) sorted[k] = ev[k];
  return JSON.stringify(sorted);
}

export type SpawnResult = { code: number | null; signal: NodeJS.Signals | null; stdout: string; stderr: string; spawnError?: string };

/** Runs the binary with stdin input; resolves when it exits (SIGKILL after killAfterMs). Never rejects. */
export function runBinary(binary: string, args: string[], input: string, killAfterMs: number): Promise<SpawnResult> {
  return new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    let settled = false;
    const finish = (r: SpawnResult) => {
      if (settled) return;
      settled = true;
      clearTimeout(killer);
      resolve(r);
    };
    let child: ReturnType<typeof spawn>;
    try {
      child = spawn(binary, args, { stdio: ["pipe", "pipe", "pipe"], env: { PATH: process.env.PATH ?? "/usr/bin:/bin", HOME: process.env.HOME ?? "" } });
    } catch (err) {
      resolve({ code: null, signal: null, stdout: "", stderr: "", spawnError: String(err) });
      return;
    }
    const killer = setTimeout(() => {
      try {
        child.kill("SIGKILL");
      } catch {
        // already gone
      }
    }, killAfterMs);
    child.stdout?.on("data", (d: Buffer) => {
      if (stdout.length < 4_000_000) stdout += d.toString("utf8");
    });
    child.stderr?.on("data", (d: Buffer) => {
      if (stderr.length < 64_000) stderr += d.toString("utf8");
    });
    child.on("error", (err) => finish({ code: null, signal: null, stdout, stderr, spawnError: String(err) }));
    child.on("close", (code, signal) => finish({ code, signal, stdout, stderr }));
    child.stdin?.on("error", () => undefined);
    child.stdin?.end(input);
  });
}

export type LiveRun =
  | { ok: true; doc: StanceDoc; n: number; eventLine: string; outLine: string; ms: number; labeled: boolean }
  | { ok: false; reason: string; ms: number };

/**
 * One event through `probbit live PERSONA --state STATE --strand STRAND --clock fixed` (persona.md §5.7).
 * A missing state is created with `persona init` only when the strand is missing too (stage-0 rule); a state
 * without its strand, or the reverse, is refused rather than forked. `newStrandArgs` is asked only when the strand is
 * about to be created (the `--label k=v` pairs, when the binary takes them); `labeled` says whether any were passed.
 */
export async function runLiveEvent(opts: {
  binary: string;
  personaPath: string;
  seed: number;
  paths: LanePaths;
  eventLine: string;
  killAfterMs: number;
  newStrandArgs?: () => Promise<string[]>;
  /** A refusal of the binary itself (its version); undefined to go ahead. A refused binary never runs an event. */
  binaryCheck?: () => Promise<string | undefined>;
}): Promise<LiveRun> {
  const t0 = Date.now();
  const { paths } = opts;
  // Asked beside `persona init` too (cached per binary, so usually already settled at load).
  const check = opts.binaryCheck ? opts.binaryCheck().catch(() => undefined) : Promise.resolve(undefined);
  const hasState = existsSync(paths.state);
  const hasStrand = existsSync(paths.strand);
  if (!hasState && hasStrand) return { ok: false, reason: "strand exists without its state; refusing to fork the life", ms: Date.now() - t0 };
  // Asked beside `persona init`, so a probe of the binary adds no wait of its own.
  const extraArgs = !hasStrand && opts.newStrandArgs ? opts.newStrandArgs().catch(() => [] as string[]) : Promise.resolve([] as string[]);
  if (!hasState) {
    const init = await runBinary(
      opts.binary,
      ["persona", "init", opts.personaPath, "--seed", String(opts.seed), "--out", paths.state],
      "",
      opts.killAfterMs,
    );
    const refused = await check;
    if (refused) {
      // The state this call just made belongs to no strand: remove it, so a refused binary leaves nothing behind.
      if (init.code === 0) rmSync(paths.state, { force: true });
      return { ok: false, reason: refused, ms: Date.now() - t0 };
    }
    if (init.spawnError || init.code !== 0) {
      return { ok: false, reason: `persona init failed: ${init.spawnError ?? `exit ${init.code ?? init.signal}`} ${init.stderr.slice(0, 200)}`.trim(), ms: Date.now() - t0 };
    }
  }
  const refused = await check;
  if (refused) return { ok: false, reason: refused, ms: Date.now() - t0 };
  const extra = await extraArgs;
  const res = await runBinary(
    opts.binary,
    ["live", opts.personaPath, "--state", paths.state, "--strand", paths.strand, "--clock", "fixed", ...extra],
    `${opts.eventLine}\n`,
    opts.killAfterMs,
  );
  const ms = Date.now() - t0;
  if (res.spawnError) return { ok: false, reason: `spawn failed: ${res.spawnError}`, ms };
  const outLine = res.stdout.split("\n").find((l) => l.trim()) ?? "";
  let doc: StanceDoc | undefined;
  try {
    doc = outLine ? (JSON.parse(outLine) as StanceDoc) : undefined;
  } catch {
    doc = undefined;
  }
  if (res.code !== 0 || !doc || doc.error !== undefined || typeof doc.line !== "string") {
    const detail = doc?.error !== undefined ? JSON.stringify(doc.error).slice(0, 200) : res.stderr.slice(0, 200);
    return { ok: false, reason: `probbit live exit ${res.code ?? res.signal}: ${detail}`.trim(), ms };
  }
  const n = lastStrandEventNumber(paths.strand);
  if (n === undefined) return { ok: false, reason: "strand unreadable after the event", ms };
  return { ok: true, doc, n, eventLine: opts.eventLine, outLine: outLine.trim(), ms, labeled: extra.length > 0 };
}

/** Plugin metadata kept in the 4th TSV column of log/<lane>.jsonl (stage-0 readers see the first three). */
export type LogMeta = {
  src: "probbit-stance";
  n: number;
  key: string;
  keyBasis: string;
  method: string;
  flags: string[];
  textSource: string;
  ms: number;
  late?: boolean;
  probabilities?: Record<string, number>;
  decisionNote?: string;
  /** Classified flags the persona does not declare (dropped before the event). */
  dropped?: string[];
  /** time_pressure fed to the engine this event (the question's answer, or the heuristic with sensesMode feed). */
  timePressure?: number;
  /** Where the fed time_pressure came from, when it was not the question (heuristic). */
  timePressureSrc?: "heuristic";
  /** Sense readings fed this event (sensesMode feed, declared senses only). */
  senses?: Record<string, number | boolean>;
  /** Footer facts, kept so a restart-time dedupe renders the same footer. */
  inputs?: string[];
  moved?: string[];
};

export function appendEventLog(path: string, iso: string, eventLine: string, outLine: string, meta: LogMeta): void {
  appendFileSync(path, `${iso}\t${eventLine}\t${outLine}\t${JSON.stringify(meta)}\n`);
}

export function writeLastEpoch(path: string, epochSec: number): void {
  writeFileSync(path, `${epochSec}\n`);
}

/** Finds a logged event by idempotency key in the log tail (restart-safe dedupe). */
export function findLoggedEvent(
  logPath: string,
  key: string,
): { n: number; doc: StanceDoc; inputs?: string[]; moved?: string[] } | undefined {
  const tail = readTail(logPath, 512 * 1024);
  const lines = tail.split("\n");
  for (let i = lines.length - 1; i >= 0; i--) {
    const line = lines[i];
    if (!line || !line.includes(key)) continue;
    const cols = line.split("\t");
    if (cols.length < 4) continue;
    try {
      const meta = JSON.parse(cols[3] ?? "") as Partial<LogMeta>;
      if (meta.key !== key || typeof meta.n !== "number") continue;
      const strings = (v: unknown): string[] | undefined =>
        Array.isArray(v) && v.every((x) => typeof x === "string") ? (v as string[]) : undefined;
      const inputs = strings(meta.inputs);
      const moved = strings(meta.moved);
      return {
        n: meta.n,
        doc: JSON.parse(cols[2] ?? "") as StanceDoc,
        ...(inputs ? { inputs } : {}),
        ...(moved ? { moved } : {}),
      };
    } catch {
      continue;
    }
  }
  return undefined;
}
