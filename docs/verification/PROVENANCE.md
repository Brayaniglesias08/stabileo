# Re-check prompt, provenance and reproducibility

Roadmap: "Trust and verification backlog", items 8 and 9 (`docs/roadmap/PRODUCT_ROADMAP.md`).

## Registry of result-affecting changes

`web/src/lib/provenance/result-changes.json` lists every change that makes the same saved model
give different numbers. Each entry has:

| field | meaning |
|---|---|
| `id` | stable identifier |
| `resultsVersion` | the results version that introduced it (an integer, not the app version) |
| `date`, `prs` | when it reached main, and the pull requests |
| `area` | `analysis`, `design` or `loads` |
| `when` | which models it concerns, in feature ids (`any`: one of; `all`: each of; `allOf`: groups that must all hold) |
| `title`, `explanation` | what to tell the engineer, in `en`, `es` and `pt` |
| `changelog` | a file and one of its headings (checked by `registry.test.ts`) |

Feature ids are defined in `web/src/lib/provenance/model-features.ts`, read from the saved
snapshot. When in doubt a feature counts as present: a false "present" shows one notice too many,
a false "absent" hides a change that moved the engineer's numbers. In particular, a steel code
not bound to another code counts as CIRSOC 301, an unbound concrete code as CIRSOC 201, and
analyses a project does not record count as run.

**Adding a change**: add an entry with `resultsVersion` one above the current
`currentResultsVersion` and bump `currentResultsVersion`. `registry.test.ts` fails on unknown
features, missing translations or a heading that does not exist.

The registry is seeded with every behaviour change of the October 2026 safety wave
(`docs/MERGE_PLAN_SAFETY_WAVE.md` section 3) and the earlier BREAKING entries of `CHANGELOG.md`,
all at results version 1.

## Solve stamps and the re-check prompt

Every project snapshot carries `solvedWith` (`provenance/solve-stamp.ts`): results version,
results schema version, app version, web commit, engine version, WASM hash, solve time and the
analyses run. It rides on the snapshot, so `.ded`, session files, autosave and tabs all carry it.
It is adopted on open (never by undo) and replaced by every solve. A new project gets the current
version. A project without a stamp (saved before stamps existed) reads as version 0.

On opening a project (`.ded`, tab, autosave, share link: `onProjectOpened` in `store/file.ts`),
`store/recheck.svelte.ts` lists the registered changes after its version that concern its features.
If there are any:

- a notice lists them (title, explanation in the reader's language, link to the write-up). It has
  no close button, ignores Escape and outside clicks, and is dismissed only by ticking
  "I have read these changes" and choosing "Continue" or "Re-run analysis and design";
- the results are **stale** until the model is solved again in this build: a banner says so, and
  every report, workbook and CSV prints a STALE warning in its provenance;
- "Re-run analysis and design" runs the manual solve, then (PRO) the RC code check over the
  provided reinforcement. It does not auto-design, which would overwrite reinforcement the
  engineer chose.

Acknowledgement is per stamp and per session: reopening the file in a new session asks again.
A project solved with a newer version than the build opening it is flagged as `newer`.

## Provenance of results and reports

`provenance/result-provenance.ts` (pure) and `store/result-provenance.ts` (gathered from the live
stores) build the record printed by the calc report, the PRO report, the workbook cover and the
CSV export:

| field | |
|---|---|
| `appVersion`, `commit` | web build (`vite.config.ts`) |
| `engineVersion`, `engineCommit` | the engine's `engine_build_info()` |
| `wasmHash` | SHA-256 of the WASM bytes the app initialised (the worker pool runs the same bytes) |
| `resultsVersion`, `resultsSchemaVersion` | registry version |
| `modelHash` | SHA-256 of the exact canonical JSON of the model, without what cannot move a number (name, title block, notes, views, grid, export record, stamps, revision counters, detailing drawings) |
| `settings`, `settingsHash` | workspace, self-weight and its case, axis convention, drawing plane |
| `scope`, `resultsHash`, `baseResultsHash` | the result shown and the base solve, timings excluded |
| `engineInputSha256`, `engineResultsSha256` | the engine's own hashes of the base linear solve (`engine/src/reproduce.rs`) |
| `certificates` | status and hash of the result certificate, where certified |
| `stale`, `solvedWith` | see above |

Hashes are SHA-256 of canonical JSON that keeps every bit (`provenance/exact-hash.ts`): keys sorted,
numbers in their shortest round-trip form, `-0`, `NaN` and infinities spelled out.

The HTML reports embed the record as `<script type="application/json" id="stabileo-provenance">`.

## Reproducing a report

- **App**: Project → "Reproduce from provenance…" (Básico project panel, PRO Project tab). Paste
  the report's block or load the saved report. The open project is solved again and its model,
  settings and results hashes are compared; a differing build is named, since a difference may be
  a registered change rather than a defect. "Download solver input" saves the base solve's engine
  input.
- **CLI**: `cargo run --release --example reproduce -- solver-input-3d.json --expect-input <engineInputSha256> --expect-results <engineResultsSha256>`
  solves twice natively, checks the run is deterministic and that both hashes match; exit 1 otherwise.

## Determinism

The same model solved twice, and on the worker vs the main thread, must give bit-identical
results. Tests: `engine/tests/determinism.rs` (random 2D/3D models, with constraints, and modal,
buckling and P-Delta: each JSON parsed six times and solved, results compared as text),
`web/src/lib/store/__tests__/result-provenance.test.ts` (two solves in the app, engine hashes),
`web/e2e/recheck-provenance.spec.ts` (worker vs main thread, reproduce dialog).

Findings:

- **Fixed: the equilibrium summary differed in the last bit between two solves.** Solver inputs
  keep their entities in `HashMap`s, and every `HashMap` has its own random hash keys, so the same
  JSON parsed twice iterates in different orders. Assembly already sorts elements by id, so
  displacements and forces were identical, but `compute_equilibrium_summary_2d/3d` summed applied
  forces and reactions by iterating `DofNumbering::map` (a `HashMap`), and those sums
  (`equilibrium.appliedForceSum`, `reactionForceSum`) changed in the last bit from solve to solve.
  They now iterate `DofNumbering::entries_by_dof()`, in increasing DOF index. Any new loop that
  accumulates floats over a `HashMap` must do the same.
- Parallel assembly (`rayon`, the `parallel` feature) is not compiled into the WASM build, so the
  app has no parallel reductions; the worker and the main thread run the same single-threaded
  binary.
- Native vs WASM: the CLI computes the same algorithm natively. Transcendental functions come
  from the platform's math library natively and from Rust's own in WASM, so the native result hash
  may differ in the last bit from the browser's for models whose assembly uses them (inclined or
  rotated members). The in-app reproduce compares WASM against WASM and is bit for bit; the CLI is
  bit for bit against itself and against the input hash.
