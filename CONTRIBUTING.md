# Contributing

Thanks for looking at probbit. This page covers how to build, test and measure it, and the few
rules that keep its claims honest.

## Build and test

```
cargo build --release --workspace          # offline once the dependencies below are cached
cargo test --release --workspace           # unit, integration, regression and exact-oracle tests
cargo run --release -p probbit-cli -- demo --tasks 12 | cargo run --release -p probbit-cli -- decide --pretty
```

`acc1` in `probbit-decide/tests/acceptance.rs` has a wall-clock bound and can fail on a heavily
loaded machine; re-run it alone before reading anything into a failure.
`decide_fallback_enumerates_a_program_no_chain_can_start` in `probbit-cli/tests/cli.rs` is wall-clock-bound too
(a 4 s budget sized for a fast desktop) and skips itself when the `CI` environment variable is set; run it locally.

The default build is portable (no CPU pin). `RUSTFLAGS="-C target-cpu=native"` gives the last
bit of speed on your own machine; that binary may not run elsewhere.

Release-contract checks also run on every pull request: the Python and MCP suites, npm platform/signal tests,
and clean-prefix shell, PowerShell 5.1/7 and npm installs using locally packaged candidate binaries.
`node --test npm/test-wrapper.cjs` needs no npm dependencies. Set `PROBBIT_BIN` to the release executable and run
`python3 -m unittest discover -s python -p 'test_*.py'` for the integration contracts.

The command table in `probbit-cli/src/main.rs` (`COMMANDS`) is the one source of every `--help`, `probbit help`, the shell
completions and the man page; the parser reads its flags from it. After changing it or bumping the version, regenerate the
committed man page: `cargo run -p probbit-cli -- man > docs/probbit.1` (test `committed_man_page_is_generated`).

## Ground rules

- **Keep dependencies small.** The default build uses one third-party crate, `libm`, for amplitude-register math.
  The `nostd` build adds `hashbrown` and `portable-atomic` (and their transitive dependencies).
  `cargo build --offline` works with dependencies cached.
- **Every number in the docs points at something runnable**: a test, an example under
  `examples/`, or a script under `bench/`. A change that moves a number updates the table it
  lives in (`BENCHMARKS.md`, `USE-CASES.md`, `docs/probbit-ir-json.md`) with the machine, the load
  and `N` stated.
- **Fixed-work modes stay bit-identical.** With `--sweeps` and `--polish-sweeps` the whole
  output must not depend on the thread count or the run, except the fields that report the run itself: `ms`, the timings and
  rates in `telemetry` (`*_ms`, `site_updates_per_s`, `process_cpu_ms`, `peak_rss_mb`) and the echoed `threads` / `nice`. Remove
  those before comparing two runs or two binaries (a background job in zsh runs at nice 5, which alone changes the bytes). Anything that changes the RNG stream
  or the sampler's move order is a reproducibility break and needs a `CHANGELOG.md` entry.
- **Negative results stay runnable.** Examples that show an accelerator not helping, or a
  classical baseline winning, are kept so the comparison can be repeated.
- **`diagnostics_passed` keeps its definition** (it replaced "certified" in 0.2.0). It means every released item passed
  the versioned heuristic gate (a 3-sigma Monte-Carlo error bound plus mode diagnostics, calibrated on exact oracles); it
  is not a proof. Only the `exact` and `infeasible` verdicts are proof-grade (under the score model).
- **Stress corpus and calibration.** The inputs in `probbit-cli/tests/stress/` are frozen: never edit or
  regenerate them. Gate thresholds (`GATE`, `ITEM_RHAT_MAX`, `PARTIAL_RHAT`, `BATCH_RATIO_MAX`, `GATE_BS_POW` in
  `probbit-ir/src/lib.rs`, recorded by `bench/calibrate.py` in its header) are FROZEN before a holdout run and never retuned
  on its results: a holdout failure is reported in `BENCHMARKS.md` ("Known failure modes") and fixed by a new mechanism
  (a move, a diagnostic, an exact tier) validated on a NEW holdout, with `GATE_VERSION` bumped. `bench/calibrate.py
  --holdout` generates the holdout families from a seed; pick a new `--holdout-seed` for each new holdout.

## Benchmarks

The scripts in `bench/` drive the release binary through a subprocess. `router_ilp.py` and
`schedule_ilp.py` need `numpy` and `scipy >= 1.9` for the HiGHS baseline; the others need only
Python 3. Build first with `cargo build --release -p probbit-cli`.

## Reporting a problem

Please include the problem JSON (or the `probbit demo` seed and flags that produce it), the exact
command, the output of `probbit stats --pretty`, and the operating system. If the sampler returned
`refused` or `partial` where you expected `diagnostics_passed`, include the `gate` object from the
decision: it says which check failed.

## License

By contributing you agree that your contributions are licensed under the Apache License 2.0,
the same license as the project.
