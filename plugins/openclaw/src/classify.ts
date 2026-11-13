/** Persona flags this plugin can set (the persona's `inputs`). launch and claim_done are left to the operator. */
export type Flag = "praise" | "correction" | "failure" | "owner_upset" | "security" | "win" | "wants_energy";
export type Method = "marker" | "decision-model" | "keyword" | "none";

export type Classification = {
  flags: Flag[];
  method: Method;
  /** Decision-model probabilities, when that path answered. */
  probabilities?: Partial<Record<Flag, number>>;
  /** Why the decision-model path was skipped or failed, when it was tried. */
  decisionNote?: string;
  /** time_pressure in [0, 1] fed to the engine from the host decision model. */
  timePressure?: number;
};

const FLAG_ORDER: Flag[] = ["praise", "correction", "failure", "owner_upset", "security", "win", "wants_energy"];

const MARKERS: Record<string, Flag> = {
  praise: "praise",
  correction: "correction",
  failure: "failure",
  upset: "owner_upset",
  security: "security",
  win: "win",
  energy: "wants_energy",
};
export const MARKER_RE = /\[(praise|correction|failure|upset|security|win|energy)\]/gi;

const KEYWORDS: Array<[Flag, RegExp]> = [
  ["praise", /\b(thanks|thank you|great (work|job|stuff)|well done|nailed it|love it|perfect)\b|^\s*yes\b/i],
  ["correction", /\b(wrong|not what|why did you)\b|(^|[\s"'(])no,/i],
  ["failure", /\b(broke|broken|failed|failing|error|errors|stalled)\b/i],
  ["security", /\b(security|token|tokens|config|gateway|deploy|deployed|deployment|restart|credential|credentials)\b/i],
];

/**
 * Good news that happened to the user. Phrase-shaped on purpose: "no reply yet", "the deal fell through", "offer expired"
 * and "it didn't work" contain none of these. A negation just before the hit ("not signed yet") or a question
 * ("has it worked?") does not count.
 */
const WIN_RE =
  /\b(?:(?:she|he|they|recruiter|someone|the client|the investor) (?:replied|responded|wrote back|said yes)|got (?:the|an|a) (?:interview|offer|job|deal|contract|callback)|we got (?:the|an|a) (?:offer|deal|contract|customer)|signed(?!\s+(?:in|out)\b)|closed the (?:deal|round|sale)|it worked|went live|(?:is|are|we're|it's) live now|first (?:customer|sale|user|paying user|order)|they accepted|offer came through)\b/i;
/** A request for more energy. "no cheerleading" and "less hype" contain none of these; "don't hype me up" is negated. */
const ENERGY_RE =
  /\b(?:more (?:energy|enthusiasm|excitement|hype)|be more (?:upbeat|excited|energetic|enthusiastic|hyped)|hype me(?: up)?|pump me up|motivate me|celebrate with me|get me hyped)\b/i;
/** Up to two words between a negation (or "if"/"hope") and the hit cancel it. */
const NEGATED_BEFORE =
  /\b(?:not|no|never|don't|dont|didn't|didnt|doesn't|hasn't|hasnt|haven't|havent|hadn't|wasn't|isn't|aren't|won't|yet to|if|unless|whether|hope|hopefully)\s+(?:[\w']+\s+){0,2}$/i;

/** True when `re` hits a sentence of `text` without a negation right before it (and, with `skipQuestions`, outside questions). */
function phraseHit(text: string, re: RegExp, skipQuestions: boolean): boolean {
  for (const sentence of text.match(/[^.!?\n]+[.!?]*/g) ?? []) {
    if (skipQuestions && sentence.trimEnd().endsWith("?")) continue;
    const global = new RegExp(re.source, "gi");
    for (const m of sentence.matchAll(global)) {
      if (!NEGATED_BEFORE.test(sentence.slice(0, m.index))) return true;
    }
  }
  return false;
}

const MAX_TEXT = 20_000;

/** Drops runtime-context carriers and URLs (paths like /config/ or ?token= are not the user's topic). */
export function sanitize(text: string): string {
  return text
    .slice(0, MAX_TEXT)
    .replace(/<<<BEGIN_OPENCLAW_INTERNAL_CONTEXT>>>[\s\S]*?<<<END_OPENCLAW_INTERNAL_CONTEXT>>>/g, " ")
    .replace(/\bhttps?:\/\/\S+/gi, " ");
}

/**
 * On runners that hand the hook the whole composed prompt (claude-cli on 2026.9.7 sends no `currentUserMessage`), pull out
 * the current user message so channel context (conversation history, reply quotes) does not reach the keywords.
 * The gateway composes blocks separated by blank lines: "Conversation info:", "Conversation context (...)", then
 * "Current message:" when there is history; a bare first message has no label.
 */
export type ExtractedSource = "prompt:current-message" | "prompt:after-context" | "prompt:tail";
export function extractCurrentMessage(prompt: string): { text: string; source: ExtractedSource } {
  const label = "Current message:";
  const at = prompt.lastIndexOf(label);
  if (at >= 0) return { text: stripReplyEnvelope(prompt.slice(at + label.length)), source: "prompt:current-message" };
  const blocks = prompt.split(/\n{2,}/u);
  const envelope =
    /^(Conversation info:|Conversation context \(|⟦openclaw:ctx⟧|```|#session:|#\d+ |\[Inter-session message\]|OpenClaw resumed|Queued #\d+ context:|System: )/u;
  let i = 0;
  while (i < blocks.length && envelope.test((blocks[i] ?? "").trimStart())) i++;
  const rest = blocks.slice(i).join("\n\n").trim();
  if (rest) return { text: stripReplyEnvelope(rest), source: "prompt:after-context" };
  return { text: stripReplyEnvelope(prompt.slice(-1500)), source: "prompt:tail" };
}

function stripReplyEnvelope(s: string): string {
  let t = s.trim();
  t = t.replace(/^\[Replying to: "(?:[^"\\]|\\.)*"\]\s*/u, "");
  t = t.replace(/^#\d+:\s*/u, "");
  return t.trim();
}

function ordered(set: Set<Flag>): Flag[] {
  return FLAG_ORDER.filter((f) => set.has(f));
}

export function markerFlags(text: string): Flag[] {
  const set = new Set<Flag>();
  for (const m of text.matchAll(MARKER_RE)) {
    const flag = MARKERS[(m[1] ?? "").toLowerCase()];
    if (flag) set.add(flag);
  }
  return ordered(set);
}

/** Conservative keywords; a correction outranks a polite "thanks". `win` may sit beside praise, correction or failure. */
export function keywordFlags(text: string): Flag[] {
  const set = new Set<Flag>();
  for (const [flag, re] of KEYWORDS) if (re.test(text)) set.add(flag);
  if (phraseHit(text, WIN_RE, true)) set.add("win");
  if (phraseHit(text, ENERGY_RE, false)) set.add("wants_energy");
  if (set.has("correction")) set.delete("praise");
  return ordered(set);
}

export type DecisionAnswer = { type?: string; probabilityTrue?: number; score?: number; probabilities?: Record<string, number> };
export type DecisionOutcome = {
  status?: string;
  reason?: string;
  result?: { answers?: Record<string, DecisionAnswer> };
};
export type DecisionEvaluate = (
  request: { state: unknown; questions: Record<string, unknown> },
  options: { agentId?: string; purpose: string; rubricVersion: string; timeoutMs: number },
) => Promise<DecisionOutcome>;

const QUESTIONS: Record<Flag, { instructions: string; true: string; false: string } | undefined> = {
  praise: {
    instructions: "Does the user praise or approve the assistant's work?",
    true: "The user praises or approves",
    false: "The user neither praises nor approves",
  },
  correction: {
    instructions: "Does the user correct the assistant?",
    true: "The user corrects the assistant",
    false: "The user does not correct the assistant",
  },
  owner_upset: {
    instructions: "Is the user frustrated?",
    true: "The user is frustrated",
    false: "The user is calm",
  },
  security: {
    instructions: "Is the message about security, credentials, config or deployment?",
    true: "The message is about security, credentials, config or deployment",
    false: "The message is about something else",
  },
  failure: undefined,
  win: {
    instructions:
      "Does the user report real good news that happened to them (a reply, an offer, a deal, a launch result, a win), not praise of the assistant?",
    true: "The user reports real good news that happened to them",
    false: "The user reports no good news of their own",
  },
  wants_energy: {
    instructions: "Does the user ask the assistant for more energy, enthusiasm or motivation?",
    true: "The user asks for more energy, enthusiasm or motivation",
    false: "The user does not ask for more energy",
  },
};

/** The urgency question: five levels, zero-based; time_pressure = expected index / 4. */
export const TIME_PRESSURE_QUESTION = {
  type: "score",
  instructions: "How urgent is the user's message?",
  criteria: ["none", "low", "moderate", "high", "urgent"],
} as const;

/**
 * A score answer → time_pressure in [0, 1], 2 decimals. Uses the answer's expected index (`score`); when that is missing,
 * the expectation over `probabilities`. Undefined when neither is usable.
 */
export function timePressureFrom(answer: DecisionAnswer | undefined): number | undefined {
  if (!answer) return undefined;
  const top = TIME_PRESSURE_QUESTION.criteria.length - 1;
  let expected: number | undefined;
  if (typeof answer.score === "number" && Number.isFinite(answer.score)) expected = answer.score;
  else if (answer.probabilities) {
    let mass = 0;
    let sum = 0;
    for (const [k, p] of Object.entries(answer.probabilities)) {
      const i = Number(k);
      if (!Number.isInteger(i) || i < 0 || i > top || !Number.isFinite(p) || p < 0) continue;
      mass += p;
      sum += i * p;
    }
    if (mass > 0) expected = sum / mass;
  }
  if (expected === undefined || expected < 0 || expected > top) return undefined;
  return Math.round((expected / top) * 100) / 100;
}

export type DecisionAnswerSet = {
  flags: Flag[];
  probabilities: Partial<Record<Flag, number>>;
  timePressure?: number;
};

/** Asks the configured decision model six Boolean questions and optionally urgency. A note means the caller falls back. */
export async function decisionFlags(
  text: string,
  evaluate: DecisionEvaluate,
  opts: { agentId?: string; timeoutMs: number; threshold: number; askTimePressure?: boolean },
): Promise<DecisionAnswerSet | { note: string }> {
  const questions: Record<string, unknown> = {};
  for (const flag of FLAG_ORDER) {
    const q = QUESTIONS[flag];
    if (q) questions[flag] = { type: "boolean", instructions: q.instructions, criteria: { true: q.true, false: q.false } };
  }
  if (opts.askTimePressure) questions.time_pressure = { ...TIME_PRESSURE_QUESTION, criteria: [...TIME_PRESSURE_QUESTION.criteria] };
  let guard: ReturnType<typeof setTimeout> | undefined;
  const deadline = new Promise<{ note: string }>((resolve) => {
    guard = setTimeout(() => resolve({ note: "deadline" }), opts.timeoutMs + 50);
  });
  try {
    const call = evaluate(
      { state: { message: text.slice(0, 1_500) }, questions },
      {
        ...(opts.agentId ? { agentId: opts.agentId } : {}),
        purpose: "probbit-stance.classify",
        rubricVersion: "2",
        timeoutMs: opts.timeoutMs,
      },
    ).then(
      (outcome) => {
        if (outcome?.status !== "ok") {
          return { note: `unavailable:${outcome?.reason ?? outcome?.status ?? "unknown"}` };
        }
        const answers = outcome.result?.answers ?? {};
        const set = new Set<Flag>();
        const probabilities: Partial<Record<Flag, number>> = {};
        for (const flag of FLAG_ORDER) {
          const p = answers[flag]?.probabilityTrue;
          if (typeof p !== "number" || !Number.isFinite(p)) continue;
          probabilities[flag] = p;
          if (p >= opts.threshold) set.add(flag);
        }
        const timePressure = opts.askTimePressure ? timePressureFrom(answers.time_pressure) : undefined;
        return {
          flags: ordered(set),
          probabilities,
          ...(timePressure !== undefined ? { timePressure } : {}),
        };
      },
      (err: unknown) => ({ note: `error:${String(err).slice(0, 120)}` }),
    );
    return await Promise.race([call, deadline]);
  } finally {
    if (guard) clearTimeout(guard);
  }
}

export type ClassifyDeps = {
  evaluate?: DecisionEvaluate;
  agentId?: string;
  timeoutMs: number;
  threshold: number;
};

/**
 * Order: (a) operator markers win outright; (b) the decision model when one is configured and answers;
 * (c) keywords. `failure` comes from markers or keywords only (the decision questions do not ask it).
 */
export async function classify(rawText: string, deps: ClassifyDeps): Promise<Classification> {
  const text = sanitize(rawText);
  if (!text.trim()) return { flags: [], method: "none" };
  const markers = markerFlags(text);
  if (markers.length > 0) return { flags: markers, method: "marker" };
  const plain = text.replace(MARKER_RE, " ");
  let decisionNote: string | undefined;
  if (deps.evaluate) {
    const answer = await decisionFlags(plain, deps.evaluate, {
      ...(deps.agentId ? { agentId: deps.agentId } : {}),
      timeoutMs: deps.timeoutMs,
      threshold: deps.threshold,
      askTimePressure: true,
    });
    if ("flags" in answer) {
      const set = new Set(answer.flags);
      if (keywordFlags(plain).includes("failure")) set.add("failure");
      return {
        flags: ordered(set),
        method: "decision-model",
        probabilities: answer.probabilities,
        ...(answer.timePressure !== undefined ? { timePressure: answer.timePressure } : {}),
      };
    }
    decisionNote = answer.note;
  }
  return { flags: keywordFlags(plain), method: "keyword", ...(decisionNote ? { decisionNote } : {}) };
}
