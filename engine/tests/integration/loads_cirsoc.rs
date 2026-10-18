//! Load generation in the engine (`loads`): CIRSOC 101 combinations and INPRES-CIRSOC 103.
//! The clause-by-clause verification is in the web differential tests against the oracle that
//! #292 verified (docs/verification/LOADS.md); these pin the printed values directly.

use dedaliano_engine::loads::cirsoc101_combinations::*;
use dedaliano_engine::loads::cirsoc103_seismic::*;

fn labels(c: &[Combination]) -> Vec<(String, Vec<(LoadSymbol, f64)>)> {
    c.iter().map(|c| (c.id.clone(), c.terms.iter().map(|t| (t.symbol, t.factor)).collect())).collect()
}

#[test]
fn dead_and_live_give_combinations_1_and_2() {
    let i = CombinationInputs { present: Present { l: true, ..Default::default() }, ..Default::default() };
    let c = generate_combinations(&i);
    assert_eq!(labels(&c), vec![
        ("1".into(), vec![(LoadSymbol::D, 1.4)]),
        ("2".into(), vec![(LoadSymbol::D, 1.2), (LoadSymbol::L, 1.6)]),
    ]);
}

#[test]
fn wind_and_earthquake_are_never_together() {
    let i = CombinationInputs { present: Present { l: true, w: true, e: true, s: true, lr: true, ..Default::default() }, ..Default::default() };
    for c in generate_combinations(&i) {
        let has = |s| c.terms.iter().any(|t| t.symbol == s);
        assert!(!(has(LoadSymbol::W) && has(LoadSymbol::E)), "{}", c.id);
    }
}

#[test]
fn exception_1_halves_l_at_lo_5_but_not_above() {
    let mut i = CombinationInputs { present: Present { l: true, w: true, ..Default::default() }, max_lo_knm2: Some(5.0), ..Default::default() };
    assert_eq!(live_load_factor_in_companion(&i).0, 0.5);
    i.max_lo_knm2 = Some(5.01);
    assert_eq!(live_load_factor_in_companion(&i).0, 1.0);
    i.max_lo_knm2 = Some(3.0);
    i.has_garage_or_public_assembly = true;
    assert_eq!(live_load_factor_in_companion(&i).0, 1.0);
}

#[test]
fn seismic_companions_take_the_larger_of_101_and_103() {
    let i = CombinationInputs {
        present: Present { l: true, s: true, e: true, ..Default::default() },
        max_lo_knm2: Some(3.0),
        seismic_companions: Some(SeismicCompanions { f1: Some(0.25), f2: Some(0.7) }),
        ..Default::default()
    };
    let c = generate_combinations(&i);
    let five = c.iter().find(|c| c.id == "5").unwrap();
    let f = |s| five.terms.iter().find(|t| t.symbol == s).unwrap().factor;
    assert_eq!(f(LoadSymbol::L), 0.5);
    assert_eq!(f(LoadSymbol::S), 0.7);
}

#[test]
fn table_3_1_spectrum_zone_4_type_1() {
    let SpectrumOutcome::Spectrum { spectrum: s } = design_spectrum(&SpectrumInputs { zone: 4, site: SiteClass::SB, na: None, nv: None }) else { panic!() };
    // Ca = 0,37·1,0, Cv = 0,51·1,2, T2 = Cv/(2,5·Ca), T1 = 0,2·T2.
    assert_eq!(s.ca, 0.37);
    assert!((s.cv - 0.51 * 1.2).abs() < 1e-15);
    assert!((s.t2 - 0.612 / (2.5 * 0.37)).abs() < 1e-15);
    assert!((s.t1 - 0.2 * s.t2).abs() < 1e-15);
    assert!(s.fault_proximity_assumed);
    assert_eq!(s.t3, 13.0);
    // Continuity at T1 and T2.
    assert!((spectral_ordinate(s.t1, &s) - 2.5 * s.ca).abs() < 1e-12);
    assert!((spectral_ordinate(s.t2 + 1e-12, &s) - 2.5 * s.ca).abs() < 1e-9);
}

#[test]
fn zone_0_and_site_sf_are_refused() {
    assert!(matches!(design_spectrum(&SpectrumInputs { zone: 0, site: SiteClass::SB, na: None, nv: None }), SpectrumOutcome::Blocked { .. }));
    assert!(matches!(design_spectrum(&SpectrumInputs { zone: 2, site: SiteClass::SF, na: None, nv: None }), SpectrumOutcome::Blocked { .. }));
}

#[test]
fn static_method_values() {
    // Ta = 0,0466·20^0,9; Cu(0,20) = 1,60 + 0,5·(1,45 − 1,60).
    let p = design_period(20.0, "concreteMomentFrame", None, 0.20).unwrap();
    assert!((p.ta - 0.0466 * 20f64.powf(0.9)).abs() < 1e-15);
    assert!((p.cu - 1.525).abs() < 1e-12);
    // [6.11]: forces proportional to W·h, summing to V0.
    let d = distribute_in_height(&[MassLevel { h: 3.0, w: 100.0 }, MassLevel { h: 6.0, w: 100.0 }], 90.0, 0.3, Some(0.5));
    assert!(!d.top_heavy);
    assert!((d.forces[0].2 - 30.0).abs() < 1e-12 && (d.forces[1].2 - 60.0).abs() < 1e-12);
    // Tabla 2.5: group A in zone 4 above 30 m needs the dynamic method.
    let a = static_method_applicable(4, "A", 31.0, 10, "regular", None, None);
    assert!(!a.allowed && a.dynamic_required);
}
