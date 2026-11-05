import { runBinary } from "./probbit.js";

/** This plugin's version (package.json's; a unit test keeps the two equal). */
export const PLUGIN_VERSION = "0.3.0";
/**
 * The oldest probbit this plugin runs: 0.13.0 is the first release whose engine reads the bundled persona's `display`
 * block (the glyph) and takes `live --label`; older binaries refuse that persona or write strands the monitor cannot label.
 */
export const MIN_PROBBIT = "0.13.0";

type Version = { core: [number, number, number]; pre: boolean };

/** `probbit X.Y.Z[-pre]` (as `probbit --version` prints it) -> the version string; undefined for anything else. */
export function parseProbbitVersion(text: string): string | undefined {
  const m = /^probbit (\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)\s*$/mu.exec(text);
  return m?.[1];
}

function parse(v: string): Version | undefined {
  const m = /^(\d+)\.(\d+)\.(\d+)(-.+)?$/u.exec(v);
  if (!m) return undefined;
  return { core: [Number(m[1]), Number(m[2]), Number(m[3])], pre: m[4] !== undefined };
}

/** Semver order on X.Y.Z with a pre-release below its release (`0.13.0-dev` < `0.13.0` < `0.13.1-dev`). */
export function versionBelow(version: string, minimum: string): boolean {
  const a = parse(version);
  const b = parse(minimum);
  if (!a || !b) return false;
  for (let i = 0; i < 3; i++) {
    const x = a.core[i] as number;
    const y = b.core[i] as number;
    if (x !== y) return x < y;
  }
  return a.pre && !b.pre;
}

/**
 * The binary's version against MIN_PROBBIT: `ok: false` with a message naming both versions when the binary says it is
 * older. A binary that cannot be started or does not print `probbit X.Y.Z` is not judged here (`ok: true`, no version):
 * the event itself then fails or succeeds on its own, as before.
 */
export type VersionCheck = { ok: true; version?: string } | { ok: false; version: string; reason: string };

export function versionRefusal(binary: string, version: string): string {
  return `probbit-stance ${PLUGIN_VERSION} needs probbit ${MIN_PROBBIT} or later; ${binary} is probbit ${version}: upgrade it (npm i -g probbit@latest, or install.sh), or keep probbit-stance 0.1.0 with probbit 0.12.x`;
}

const versionProbes = new Map<string, Promise<VersionCheck>>();

/** `probbit --version`, once per binary per process (a cached promise; never rejects). */
export function checkBinaryVersion(binary: string, killAfterMs: number): Promise<VersionCheck> {
  let probe = versionProbes.get(binary);
  if (!probe) {
    probe = runBinary(binary, ["--version"], "", killAfterMs).then((r): VersionCheck => {
      if (r.spawnError || r.code !== 0) return { ok: true };
      const version = parseProbbitVersion(r.stdout);
      if (!version) return { ok: true };
      if (versionBelow(version, MIN_PROBBIT)) return { ok: false, version, reason: versionRefusal(binary, version) };
      return { ok: true, version };
    });
    versionProbes.set(binary, probe);
  }
  return probe;
}
