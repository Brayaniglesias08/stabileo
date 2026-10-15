//! Certified design-code utilization. See "Certified design checks" in
//! `docs/verification/CERTIFY.md`.
//!
//! Given demand enclosures `[F − ΔF, F + ΔF]` (from the force / combination
//! certificates), the code formulas are evaluated in outward-rounded interval
//! arithmetic through the *same generic implementation* the ordinary check
//! uses (`crate::certify::real::Real`), so the result `[lower, upper]`
//! encloses the exact real utilization for every demand in the box. Branches
//! whose condition is undecided on the box are evaluated both ways and hulled.
//!
//! Covered: AISC 360 member checks (`postprocess::steel_check`): tension D2,
//! compression E3, flexure F2/F6, shear G2 and the H1 interaction.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::combinations::{certify_multi_case_2d, certify_multi_case_3d, CertifiedMultiCase2D, CertifiedMultiCase3D};
use crate::solver::load_cases::{MultiCaseInput, MultiCaseInput3D};
use super::interval::Interval;
use super::real::{branch, Real};
use crate::postprocess::diagrams::{compute_diagram_value_at_sorted, sorted_point_loads};
use crate::postprocess::diagrams_3d::evaluate_diagram_3d_at;
use crate::postprocess::steel_check::{
    compression_capacity, flexural_capacity_y, flexural_capacity_z, interaction_h1, shear_capacity,
    tension_capacity, ElementDesignForces, SteelMemberData,
};

/// Absolute bounds |exact − computed| on one set of design forces.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DemandBound {
    pub n: f64,
    pub my: f64,
    #[serde(default)]
    pub mz: f64,
    #[serde(default)]
    pub vy: f64,
}

/// Design forces with their certified error bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundedSteelDemand {
    pub forces: ElementDesignForces,
    pub bound: DemandBound,
}

/// Members and every demand set (e.g. each station × combination) to check.
/// A member is certified over *all* demand sets given for its element id.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertifiedSteelCheckInput {
    pub members: Vec<SteelMemberData>,
    pub demands: Vec<BoundedSteelDemand>,
}

/// Enclosure of one check's ratio, maximized over the member's demand sets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RatioEnclosure {
    pub name: String,
    pub lower: f64,
    pub upper: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertifiedSteelResult {
    pub element_id: usize,
    pub certified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The exact utilization (max over checks and demand sets) lies in
    /// `[utilization_lower, utilization_upper]` (both 0 when uncertified).
    pub utilization_lower: f64,
    pub utilization_upper: f64,
    /// Certified to pass: `utilization_upper < 1`.
    pub certified_ok: bool,
    /// Certified to fail: `utilization_lower > 1`.
    pub certified_fail: bool,
    /// Check with the largest upper bound.
    pub governing_check: String,
    pub checks: Vec<RatioEnclosure>,
    pub demand_count: usize,
}

const CHECK_NAMES: [&str; 6] =
    ["Tension D2", "Compression E3", "Flexure-Y F2", "Flexure-Z F6", "Shear G2", "Interaction H1"];

/// Interval enclosures of the six AISC ratios for one demand box. Mirrors
/// `steel_check::check_single_member`; Err when the box cannot be certified
/// (non-finite input, capacity not certified positive where a demand exists).
pub fn steel_ratio_enclosures(
    m: &SteelMemberData,
    f: &ElementDesignForces,
    b: &DemandBound,
) -> Result<[Interval; 6], String> {
    let around = |x: f64, d: f64, what: &str| -> Result<Interval, String> {
        if !x.is_finite() || !d.is_finite() || d < 0.0 {
            return Err(format!("invalid {what} demand {x} ± {d}"));
        }
        if d == 0.0 {
            return Ok(Interval::point(x)); // exact
        }
        Ok(Interval::point(x) + Interval::new(-d, d))
    };
    let n = around(f.n, b.n, "axial")?;
    let my = around(f.my, b.my, "My")?.abs();
    let mz = around(f.mz.unwrap_or(0.0), b.mz, "Mz")?.abs();
    let vy = around(f.vy.unwrap_or(0.0), b.vy, "Vy")?.abs();
    let zero = Interval::ZERO;

    // Ratio for a check that applies only when loaded: an identically zero
    // demand is "not applicable" (ratio 0), exactly as `ratio_if_loaded`.
    let ratio = |name: &str, d: Interval, cap: Interval| -> Result<Interval, String> {
        if d.lo == 0.0 && d.hi == 0.0 {
            return Ok(zero);
        }
        if !cap.is_finite() || !(cap.lo > 0.0) {
            return Err(format!("{name}: capacity not certified positive ({cap:?})"));
        }
        let r = d.div(cap);
        if r.is_finite() { Ok(r) } else { Err(format!("{name}: non-finite ratio")) }
    };

    let phi_pn_t: Interval = tension_capacity(m);
    let phi_pn_c: Interval = compression_capacity(m);
    let phi_mn_y: Interval = flexural_capacity_y(m);
    let phi_mn_z: Interval = flexural_capacity_z(m);
    let phi_vn: Interval = shear_capacity(m);

    let tension = ratio(CHECK_NAMES[0], n.max(zero), phi_pn_t)?;
    let compression = ratio(CHECK_NAMES[1], (-n).max(zero), phi_pn_c)?;
    let flex_y = ratio(CHECK_NAMES[2], my, phi_mn_y)?;
    let flex_z = ratio(CHECK_NAMES[3], mz, phi_mn_z)?;
    // Shear: `if phi_vn > 0 { vy / phi_vn } else { 0 }`.
    let shear = branch(zero.lt(phi_vn), || vy.div(phi_vn), || zero);
    let axial = branch(n.lt(zero), || compression, || tension);
    let interaction = interaction_h1(axial, flex_y, flex_z);
    let out = [tension, compression, flex_y, flex_z, shear, interaction];
    for (r, name) in out.iter().zip(CHECK_NAMES) {
        if !r.is_finite() {
            return Err(format!("{name}: enclosure not finite"));
        }
    }
    Ok(out)
}

fn uncertified(element_id: usize, reason: String, demand_count: usize) -> CertifiedSteelResult {
    CertifiedSteelResult {
        element_id,
        certified: false,
        reason: Some(reason),
        utilization_lower: 0.0,
        utilization_upper: 0.0,
        certified_ok: false,
        certified_fail: false,
        governing_check: "Not certified".into(),
        checks: vec![],
        demand_count,
    }
}

/// Certified AISC 360 checks: per member, the enclosure of the utilization
/// maximized over every demand set given for it.
pub fn certify_steel_members(input: &CertifiedSteelCheckInput) -> Vec<CertifiedSteelResult> {
    let mut by_id: HashMap<usize, Vec<&BoundedSteelDemand>> = HashMap::new();
    for d in &input.demands {
        by_id.entry(d.forces.element_id).or_default().push(d);
    }
    let mut out: Vec<CertifiedSteelResult> = input
        .members
        .iter()
        .map(|m| {
            let eid = m.element_id;
            let demands = by_id.get(&eid).map(|v| v.as_slice()).unwrap_or(&[]);
            if demands.is_empty() {
                return uncertified(eid, "no demands for this member".into(), 0);
            }
            let mut acc = [Interval::ZERO; 6];
            for d in demands {
                match steel_ratio_enclosures(m, &d.forces, &d.bound) {
                    Ok(r) => {
                        for k in 0..6 {
                            // max over demand sets: componentwise max of endpoints
                            acc[k] = Interval::new(acc[k].lo.max(r[k].lo), acc[k].hi.max(r[k].hi));
                        }
                    }
                    Err(reason) => return uncertified(eid, reason, demands.len()),
                }
            }
            let lower = acc.iter().map(|r| r.lo).fold(0.0, f64::max);
            let upper = acc.iter().map(|r| r.hi).fold(0.0, f64::max);
            let gov = (0..6).max_by(|&a, &b| acc[a].hi.total_cmp(&acc[b].hi)).unwrap();
            CertifiedSteelResult {
                element_id: eid,
                certified: true,
                reason: None,
                utilization_lower: lower,
                utilization_upper: upper,
                certified_ok: upper < 1.0,
                certified_fail: lower > 1.0,
                governing_check: CHECK_NAMES[gov].into(),
                checks: CHECK_NAMES
                    .iter()
                    .zip(acc)
                    .map(|(n, r)| RatioEnclosure { name: n.to_string(), lower: r.lo, upper: r.hi })
                    .collect(),
                demand_count: demands.len(),
            }
        })
        .collect();
    out.sort_by_key(|r| r.element_id);
    out
}

/// Demand sets for every element at `stations` evenly spaced stations
/// (t = 0..1) of every combination, with the combination's certified station
/// bounds (2D: N, M about the major axis, V). Err if any combination is
/// uncertified; an element lacking a bound in some combination gets a NaN
/// bound so its check refuses rather than silently skipping that combination.
pub fn steel_demands_from_combinations_2d(
    out: &CertifiedMultiCase2D,
    stations: usize,
) -> Result<Vec<BoundedSteelDemand>, String> {
    let stations = stations.max(2);
    let mut demands = Vec::new();
    for (cr, cc) in out.results.combination_results.iter().zip(&out.combinations) {
        if !cc.certified {
            return Err(format!("combination '{}' not certified: {:?}", cc.name, cc.reason));
        }
        let sb: HashMap<usize, _> = cc.station_bounds.iter().map(|s| (s.element_id, s)).collect();
        for ef in &cr.results.element_forces {
            let pl = sorted_point_loads(ef);
            let bound = sb
                .get(&ef.element_id)
                .map(|s| DemandBound { n: s.axial, my: s.moment, mz: 0.0, vy: s.shear })
                .unwrap_or(DemandBound { n: f64::NAN, my: f64::NAN, mz: f64::NAN, vy: f64::NAN });
            for j in 0..stations {
                let t = j as f64 / (stations - 1) as f64;
                demands.push(BoundedSteelDemand {
                    forces: ElementDesignForces {
                        element_id: ef.element_id,
                        n: compute_diagram_value_at_sorted("axial", t, ef, &pl),
                        my: compute_diagram_value_at_sorted("moment", t, ef, &pl),
                        mz: None,
                        vy: Some(compute_diagram_value_at_sorted("shear", t, ef, &pl)),
                    },
                    bound,
                });
            }
        }
    }
    Ok(demands)
}

/// 3D version of `steel_demands_from_combinations_2d` (N, My, Mz, Vy).
pub fn steel_demands_from_combinations_3d(
    out: &CertifiedMultiCase3D,
    stations: usize,
) -> Result<Vec<BoundedSteelDemand>, String> {
    let stations = stations.max(2);
    let mut demands = Vec::new();
    for (cr, cc) in out.results.combination_results.iter().zip(&out.combinations) {
        if !cc.certified {
            return Err(format!("combination '{}' not certified: {:?}", cc.name, cc.reason));
        }
        let sb: HashMap<usize, _> = cc.station_bounds.iter().map(|s| (s.element_id, s)).collect();
        for ef in &cr.results.element_forces {
            let bound = sb
                .get(&ef.element_id)
                .map(|s| DemandBound { n: s.axial, my: s.moment_y, mz: s.moment_z, vy: s.shear_y })
                .unwrap_or(DemandBound { n: f64::NAN, my: f64::NAN, mz: f64::NAN, vy: f64::NAN });
            for j in 0..stations {
                let t = j as f64 / (stations - 1) as f64;
                demands.push(BoundedSteelDemand {
                    forces: ElementDesignForces {
                        element_id: ef.element_id,
                        n: evaluate_diagram_3d_at(ef, "axial", t),
                        my: evaluate_diagram_3d_at(ef, "momentY", t),
                        mz: Some(evaluate_diagram_3d_at(ef, "momentZ", t)),
                        vy: Some(evaluate_diagram_3d_at(ef, "shearY", t)),
                    },
                    bound,
                });
            }
        }
    }
    Ok(demands)
}

/// Multi-case model plus steel member data: solve, certify the combinations,
/// then certify every member over all stations × combinations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteelMultiCaseInput2D {
    pub multi_case: MultiCaseInput,
    pub members: Vec<SteelMemberData>,
    /// Stations per element (default 21, as the envelope diagrams).
    #[serde(default)]
    pub stations: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteelMultiCaseInput3D {
    pub multi_case: MultiCaseInput3D,
    pub members: Vec<SteelMemberData>,
    #[serde(default)]
    pub stations: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertifiedSteelMultiCase2D {
    pub analysis: CertifiedMultiCase2D,
    pub steel: Vec<CertifiedSteelResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertifiedSteelMultiCase3D {
    pub analysis: CertifiedMultiCase3D,
    pub steel: Vec<CertifiedSteelResult>,
}

fn members_uncertified(members: &[SteelMemberData], reason: &str) -> Vec<CertifiedSteelResult> {
    members.iter().map(|m| uncertified(m.element_id, reason.to_string(), 0)).collect()
}

/// End to end, 2D: `certify_multi_case_2d` → station demands → `certify_steel_members`.
pub fn certify_steel_multi_case_2d(input: &SteelMultiCaseInput2D) -> Result<CertifiedSteelMultiCase2D, String> {
    let analysis = certify_multi_case_2d(&input.multi_case)?;
    let steel = match steel_demands_from_combinations_2d(&analysis, input.stations.unwrap_or(21)) {
        Ok(demands) => certify_steel_members(&CertifiedSteelCheckInput { members: input.members.clone(), demands }),
        Err(reason) => members_uncertified(&input.members, &reason),
    };
    Ok(CertifiedSteelMultiCase2D { analysis, steel })
}

/// End to end, 3D.
pub fn certify_steel_multi_case_3d(input: &SteelMultiCaseInput3D) -> Result<CertifiedSteelMultiCase3D, String> {
    let analysis = certify_multi_case_3d(&input.multi_case)?;
    let steel = match steel_demands_from_combinations_3d(&analysis, input.stations.unwrap_or(21)) {
        Ok(demands) => certify_steel_members(&CertifiedSteelCheckInput { members: input.members.clone(), demands }),
        Err(reason) => members_uncertified(&input.members, &reason),
    };
    Ok(CertifiedSteelMultiCase3D { analysis, steel })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postprocess::steel_check::{check_steel_members, SteelCheckInput};

    /// W310x97-like section, SI units (N, m, Pa).
    pub(crate) fn w_member(id: usize, lb: f64) -> SteelMemberData {
        SteelMemberData {
            element_id: id,
            fy: 345e6,
            fu: Some(450e6),
            ag: 1.23e-2,
            an: Some(1.1e-2),
            u_factor: Some(0.9),
            aw: Some(3.08e-3),
            cv1: None,
            lby: lb,
            lbz: lb,
            ky: None,
            kz: None,
            iy: 2.22e-4,
            iz: 7.25e-5,
            ry: 0.134,
            rz: 0.0768,
            zy: 1.59e-3,
            zz: 7.23e-4,
            sy: 1.44e-3,
            sz: 4.77e-4,
            j: 9.1e-7,
            cw: Some(1.56e-6),
            lb: Some(lb),
            cb: Some(1.14),
            e: 200e9,
            g: None,
            depth: Some(0.308),
        }
    }

    struct Lcg(u64);
    impl Lcg {
        fn unit(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        fn range(&mut self, a: f64, b: f64) -> f64 {
            a + (b - a) * self.unit()
        }
    }

    fn random_member(g: &mut Lcg, id: usize) -> SteelMemberData {
        let mut m = w_member(id, g.range(0.5, 14.0));
        let s = g.range(0.3, 2.5);
        m.ag *= s;
        m.zy *= s * s;
        m.sy *= s * s;
        m.zz *= s * s;
        m.sz *= s * s;
        m.ry *= s.sqrt();
        m.rz *= s.sqrt();
        m.cw = if g.unit() < 0.2 { None } else { m.cw.map(|c| c * s * s * s) };
        m.fu = if g.unit() < 0.3 { None } else { m.fu };
        m.aw = if g.unit() < 0.2 { None } else { m.aw };
        m.cb = Some(g.range(1.0, 2.2));
        m.ky = Some(g.range(0.5, 2.1));
        m
    }

    fn random_forces(g: &mut Lcg, id: usize) -> ElementDesignForces {
        ElementDesignForces {
            element_id: id,
            n: g.range(-4e6, 4e6),
            my: g.range(-6e5, 6e5),
            mz: Some(g.range(-2e5, 2e5)),
            vy: Some(g.range(-8e5, 8e5)),
        }
    }

    fn f64_ratios(m: &SteelMemberData, f: &ElementDesignForces) -> [f64; 6] {
        let r = &check_steel_members(&SteelCheckInput { members: vec![m.clone()], forces: vec![f.clone()] })[0];
        [r.tension_ratio, r.compression_ratio, r.flexure_y_ratio, r.flexure_z_ratio, r.shear_ratio, r.interaction_ratio]
    }

    const ZERO_BOUND: DemandBound = DemandBound { n: 0.0, my: 0.0, mz: 0.0, vy: 0.0 };

    /// The f64 check and the interval twin agree on point inputs: the
    /// enclosure contains the f64 value and is ulp-tight.
    #[test]
    fn f64_and_interval_paths_agree_on_points() {
        let mut g = Lcg(2024);
        for i in 0..3000 {
            let m = random_member(&mut g, i);
            let f = random_forces(&mut g, i);
            let r = steel_ratio_enclosures(&m, &f, &ZERO_BOUND).unwrap();
            let p = f64_ratios(&m, &f);
            for k in 0..6 {
                let tol = 1e-12 * p[k].abs().max(1e-300);
                assert!(r[k].lo - tol <= p[k] && p[k] <= r[k].hi + tol, "member {i} check {k}: {} ∉ {:?}", p[k], r[k]);
                assert!(r[k].hi - r[k].lo <= 1e-12 * p[k].abs().max(1e-12), "member {i} check {k}: wide {:?}", r[k]);
            }
        }
    }

    /// Sampling the demand box with the f64 check never leaves the enclosure.
    #[test]
    fn enclosure_contains_sampled_demands() {
        let mut g = Lcg(77);
        for i in 0..400 {
            let m = random_member(&mut g, i);
            let f = random_forces(&mut g, i);
            let b = DemandBound {
                n: f.n.abs() * g.range(0.0, 0.3) + 1.0,
                my: f.my.abs() * g.range(0.0, 0.3),
                mz: f.mz.unwrap().abs() * g.range(0.0, 0.3),
                vy: f.vy.unwrap().abs() * g.range(0.0, 0.3),
            };
            let r = steel_ratio_enclosures(&m, &f, &b).unwrap();
            for _ in 0..50 {
                let s = ElementDesignForces {
                    element_id: i,
                    n: f.n + b.n * g.range(-1.0, 1.0),
                    my: f.my + b.my * g.range(-1.0, 1.0),
                    mz: Some(f.mz.unwrap() + b.mz * g.range(-1.0, 1.0)),
                    vy: Some(f.vy.unwrap() + b.vy * g.range(-1.0, 1.0)),
                };
                let p = f64_ratios(&m, &s);
                for k in 0..6 {
                    let tol = 1e-12 * p[k].abs();
                    assert!(r[k].lo - tol <= p[k] && p[k] <= r[k].hi + tol, "{i}/{k}: {} ∉ {:?}", p[k], r[k]);
                }
            }
        }
    }

    /// Demand box straddling the H1 switch at Pr/Pc = 0.2 (where H1-1a and
    /// H1-1b disagree): the enclosure must contain both branch values.
    #[test]
    fn straddled_h1_branch_takes_hull() {
        let m = w_member(1, 3.0);
        let pc: f64 = compression_capacity(&m);
        let mcy: f64 = flexural_capacity_y(&m);
        let my = 0.3 * mcy;
        let f = ElementDesignForces { element_id: 1, n: -0.2 * pc, my, mz: Some(0.0), vy: Some(0.0) };
        let b = DemandBound { n: 0.01 * pc, my: 0.0, mz: 0.0, vy: 0.0 };
        let r = steel_ratio_enclosures(&m, &f, &b).unwrap()[5];
        let a = 0.195 / 2.0 + 0.3; // H1-1b just below the switch
        let c = 0.205 + 8.0 / 9.0 * 0.3; // H1-1a just above
        assert!(r.lo <= a && a <= r.hi && r.lo <= c && c <= r.hi, "{r:?}");
        // the jump at the switch: 0.2 + 8/9·0.3 vs 0.1 + 0.3
        assert!(r.hi >= 0.2 + 8.0 / 9.0 * 0.3 && r.lo <= 0.1 + 0.3);
    }

    /// The certificate refuses (never under-bounds) on bad input.
    #[test]
    fn refuses_invalid_bounds_and_capacities() {
        let m = w_member(1, 3.0);
        let f = ElementDesignForces { element_id: 1, n: -1e5, my: 1e5, mz: None, vy: Some(1e4) };
        for b in [
            DemandBound { n: f64::NAN, ..ZERO_BOUND },
            DemandBound { my: -1.0, ..ZERO_BOUND },
            DemandBound { vy: f64::INFINITY, ..ZERO_BOUND },
        ] {
            assert!(steel_ratio_enclosures(&m, &f, &b).is_err(), "{b:?}");
        }
        // Zero plastic modulus with a moment demand: no capacity to certify.
        let mut bad = m.clone();
        bad.zy = 0.0;
        assert!(steel_ratio_enclosures(&bad, &f, &ZERO_BOUND).is_err());
        // ... but with no moment the flexure check does not apply.
        let f0 = ElementDesignForces { my: 0.0, ..f.clone() };
        let r0 = steel_ratio_enclosures(&bad, &f0, &ZERO_BOUND);
        assert!(r0.is_ok(), "{r0:?}");
        // Member level: uncertified, never "ok".
        let res = certify_steel_members(&CertifiedSteelCheckInput {
            members: vec![bad, w_member(2, 3.0)],
            demands: vec![BoundedSteelDemand { forces: f.clone(), bound: ZERO_BOUND }],
        });
        assert!(res.iter().all(|r| !r.certified && !r.certified_ok), "{res:?}");
    }

    /// Member results: max over demand sets; ok / fail flags are certified.
    #[test]
    fn member_aggregates_over_demands() {
        let m = w_member(1, 3.0);
        let mcy: f64 = flexural_capacity_y(&m);
        let mk = |my: f64| BoundedSteelDemand {
            forces: ElementDesignForces { element_id: 1, n: 0.0, my, mz: None, vy: None },
            bound: DemandBound { n: 0.0, my: 0.01 * mcy, mz: 0.0, vy: 0.0 },
        };
        let res = certify_steel_members(&CertifiedSteelCheckInput {
            members: vec![m.clone()],
            demands: vec![mk(0.2 * mcy), mk(-0.7 * mcy), mk(0.5 * mcy)],
        });
        let r = &res[0];
        assert!(r.certified && r.certified_ok && !r.certified_fail);
        assert!(r.utilization_lower <= 0.69 + 1e-12 && r.utilization_upper >= 0.71 - 1e-12, "{r:?}");
        assert!(r.utilization_upper < 0.72);
        let res = certify_steel_members(&CertifiedSteelCheckInput { members: vec![m], demands: vec![mk(1.5 * mcy)] });
        assert!(res[0].certified_fail && !res[0].certified_ok);
    }
}
