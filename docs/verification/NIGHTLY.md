# Nightly verification

The checks too slow for a pull request run every night on `main`, on GitHub-hosted runners,
and on demand from the Actions tab or with `gh workflow run <file>`. A failed scheduled run
opens a GitHub issue labelled `verification-failure`, one per job, or comments on the one
already open for that job (`.github/workflows/verification-alert.yml`,
`.github/scripts/verification-alert.cjs`). These issues are never closed automatically: the
random seeds and the mutants shards change every night, so a green run the next day does not
mean the finding is fixed. Close the issue in the PR that fixes it.

| Workflow | Jobs | Schedule (UTC) | Wall clock | Runner time |
|---|---|---|---|---|
| `nightly.yml` | `certify-large` | 05:00 | ~25 min | ~25 min |
| | `opensees-diff` (OpenSees, 2D/3D static, buckling, modal, P-Delta, time history, plates, shells) | | ~15 min | ~15 min |
| | `codeaster-diff` (Code_Aster 18.1, 3D linear with releases, connectors, constraints, thermal, inclined supports) | | ~10 min | ~10 min |
| | `fuzz` (4 targets × 30 min) | | ~40 min | ~2.7 h |
| | `mutants` (8 of 128 shards) | | 1-3.5 h | 8-28 h |
| | `bench` (criterion, informational) | | ~20 min | ~20 min |
| `differential.yml` | Frame3DD, PyNite, Mystran, Kratos, CalculiX | 04:30 | ~25 min | ~2 h |
| `verify.yml` | `lean`, `symbolic`, `kani` (nightly tier) | 05:00 | ~2 h | ~2.2 h |
| `mutation-ts.yml` | Stryker over the safety-relevant TypeScript (3322 mutants) | 03:17 | ~4.5 h | ~4.5 h |
| `supply-chain.yml` | cargo deny / audit, npm audit | 06:00 | ~2 min | ~3 min |

Total: 20 to 45 runner-hours a night, most of it mutation testing; standard
GitHub-hosted runners are free for this public repository, so the cost is runner
concurrency, not money. Everything starts between 03:00 and 06:00 UTC (night in Argentina),
when pull requests are not waiting for runners.

## Mutants rotation

`scripts/mutants.sh` covers ~23,000 mutants (certificate code, the linear solver core, the
element kernels). Each one is a rebuild of the engine and its test binaries plus a test run.
On the first scheduled run that was ~4 min per mutant (a full non-incremental rebuild with debug
info, linked by the default linker, then every test binary in turn), and no shard finished in
300 minutes. With incremental builds, no debug info, mold and nextest (which stops at the first
failing test) it is ~40 s of build and a few seconds of tests, ~22 s of wall clock per mutant with
two in parallel, for a mutant the tests catch. A mutant they miss runs the whole test set (up
to ~2 min): a shard of the linear solver with most mutants missed tested only 269 of 363 in 320
minutes at 64 shards. So the `plan` job of `nightly.yml` splits the mutants into 128 shards
(~180 each) and picks 8 each night from the day number: every mutant is tested once every 16
nights. Each shard has a 320-minute budget; if it does not finish, the job fails saying how far
it got (`scripts/mutants-summary.py`), and `MUTANTS_TOTAL` in `nightly.yml` must go up.

Run particular shards by hand:

    gh workflow run nightly.yml -f only=mutants -f mutants_shards='[5,6]'
    gh workflow run nightly.yml -f only=mutants -f mutants_total=1024 -f mutants_shards='[0]'   # a quick sample (~23 mutants)

## Running a job by hand

    gh workflow run nightly.yml                          # everything
    gh workflow run nightly.yml -f only='fuzz codeaster-diff'
    gh workflow run differential.yml -f seed=744000      # reproduce a night's seeds
    gh workflow run verification-alert.yml -f run_id=<id>               # what it would report (dry run)
    gh workflow run verification-alert.yml -f run_id=<id> -f dry_run=false

## What cannot run on GitHub-hosted runners

Nothing at present. Every job above installs its reference program on the runner (pip,
conda-forge, a pinned source build, or nix) and fits in a standard runner's 16 GB and
6-hour limit; Kani runs one harness at a time under a memory watchdog for that reason.
