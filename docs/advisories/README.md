# Safety advisories

Advisories for released versions that produced wrong or unconservative results. The process,
severity scale, notification channels, timelines and template are in
[../SAFETY_ADVISORIES.md](../SAFETY_ADVISORIES.md).

**All advisories below are drafts.** The first four are retroactive, for defects fixed in 0.2.0;
the rest are for defects being fixed now. A draft is published (GitHub issue labelled
`safety-advisory`, release notes, in-app notice through the re-check registry) when its fix is
released.

| ID | Summary | Severity | Issue | Fix | Fixed in | Status |
|---|---|---|---|---|---|---|
| [SA-2026-001](SA-2026-001-loads-combinations.md) | Load combinations and generated loads (CIRSOC 101/102/103) unconservative | High | — | #292 | 0.2.0 | Draft |
| [SA-2026-002](SA-2026-002-concrete-checks.md) | CIRSOC 201 / ACI 318 checks overstated capacity (φMn up to 18 %) | Critical | — | #298, #301 | 0.2.0 | Draft |
| [SA-2026-003](SA-2026-003-steel-cfs-connections.md) | Steel, cold-formed and connection checks overstated capacity (up to 60 %) | Critical | — | #286, #291, #293, #295, #301, #303, #308 | 0.2.0 | Draft |
| [SA-2026-004](SA-2026-004-analysis-results.md) | Analyses ignored part of the model (ground motion up to 50 % low, plastic collapse up to 2.3×) | Critical | — | safety wave #275–#302 | 0.2.0 | Draft |
| [SA-2026-005](SA-2026-005-inclined-member-loads.md) | 2D inclined members: axial force missed the along-member load; member couples dropped in 3D | High | [#343](https://github.com/lambdaclass/stabileo/issues/343) | #274 | — | Draft |
| [SA-2026-006](SA-2026-006-mirrored-moments-2d-in-3d.md) | 2D model solved in 3D: nodal moments and imposed rotations mirrored | High | [#435](https://github.com/lambdaclass/stabileo/issues/435) | #436 | — | Draft |
| [SA-2026-007](SA-2026-007-torsional-buckling.md) | 3D buckling missed torsional / flexural-torsional modes (up to 96 % high) | High | [#404](https://github.com/lambdaclass/stabileo/issues/404) | #434 | — | Draft |
| [SA-2026-008](SA-2026-008-spectral-combination.md) | Spectral base shear SRSS under CQC; participation omitted support-coupled mass | High | [#405](https://github.com/lambdaclass/stabileo/issues/405) | #434 | — | Draft |
| [SA-2026-009](SA-2026-009-shb8-thickness-locking.md) | Solid shell (SHB8) ~18 % too stiff in thin-plate bending | Medium | [#400](https://github.com/lambdaclass/stabileo/issues/400) | #401 | — | Draft |
| [SA-2026-010](SA-2026-010-spectral-certificate-nonfinite.md) | Spectral certificate passed on non-finite results | Medium | [#335](https://github.com/lambdaclass/stabileo/issues/335) | #398 | — | Draft |

Other open `safety` issues (the derivation-review findings #406–#433, #338, #439) get an advisory
or a recorded "no advisory needed" when they are triaged; the list is on the
[verification dashboard](../verification/DASHBOARD.md).
