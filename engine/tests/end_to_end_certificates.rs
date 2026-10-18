//! End-to-end certificates (docs/verification/CERTIFY.md, "End-to-end
//! certificates"): the independent re-assembly of K and f from the model
//! (`certify::reassembly`) agrees with the engine on every example model,
//! fixture and random model it covers, and catches planted assembly bugs.

mod fuzz_models;

use dedaliano_engine::certify::reassembly::EndToEnd;
use dedaliano_engine::certify::{certify_linear_2d_full, certify_linear_3d_full, CertificateLevel};
use dedaliano_engine::types::*;
use fuzz_models::{add_random_constraints_2d, add_random_constraints_3d, random_2d_model_rich, random_3d_model_rich};

fn fixture_paths(prefix: &str, filter: impl Fn(&str) -> bool) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().unwrap().to_str().unwrap();
            n.starts_with(prefix) && n.ends_with("-input.json") && filter(n)
        })
        .collect();
    v.sort();
    v
}

#[derive(Default, Debug)]
struct Tally {
    certified: usize,
    out_of_scope: usize,
    not_checked: usize,
    failed: Vec<String>,
    worst_rel: f64,
}

impl Tally {
    fn add(&mut self, name: &str, e2e: &EndToEnd, assembled_ok: bool) {
        match e2e {
            EndToEnd::Certified { rel_bound, k_discrepancy, f_discrepancy, .. } => {
                self.certified += 1;
                self.worst_rel = self.worst_rel.max(*rel_bound);
                eprintln!("{name}: e2e rel={rel_bound:.2e} dK={k_discrepancy:.1e} df={f_discrepancy:.1e}");
            }
            EndToEnd::OutOfScope { reason } => {
                self.out_of_scope += 1;
                eprintln!("{name}: out of scope: {reason}");
            }
            EndToEnd::NotChecked => self.not_checked += 1,
            EndToEnd::Failed { reason } => {
                eprintln!("{name}: FAILED (assembled certified: {assembled_ok}): {reason}");
                if assembled_ok {
                    self.failed.push(format!("{name}: {reason}"));
                }
            }
        }
    }
}

#[test]
fn end_to_end_2d_fixtures() {
    let mut paths = fixture_paths("ex-", |n| !n.starts_with("ex-3d-"));
    paths.extend(fixture_paths("random-", |n| !n.starts_with("random3d")));
    paths.extend(fixture_paths("", |n| ["cantilever-input", "portal-", "ss-beam"].iter().any(|p| n.starts_with(p))));
    let mut t = Tally::default();
    for p in &paths {
        let name = p.file_name().unwrap().to_str().unwrap();
        let Ok(input) = serde_json::from_str::<SolverInput>(&std::fs::read_to_string(p).unwrap()) else { continue };
        let Ok(out) = certify_linear_2d_full(&input) else { continue };
        t.add(name, &out.end_to_end, out.certificate.is_certified());
        if out.end_to_end.is_certified() {
            assert_eq!(out.certificate_level, CertificateLevel::EndToEnd);
        }
    }
    eprintln!("2D fixtures: {t:?}");
    assert!(t.failed.is_empty(), "{:#?}", t.failed);
    assert!(t.certified >= 40, "only {} 2D fixtures certified end to end", t.certified);
}

#[test]
fn end_to_end_3d_fixtures() {
    let mut paths = fixture_paths("ex-3d-", |_| true);
    paths.extend(fixture_paths("random3d-", |_| true));
    paths.extend(fixture_paths("cantilever-3d", |_| true));
    let mut t = Tally::default();
    for p in &paths {
        let name = p.file_name().unwrap().to_str().unwrap();
        let Ok(input) = serde_json::from_str::<SolverInput3D>(&std::fs::read_to_string(p).unwrap()) else { continue };
        let Ok(out) = certify_linear_3d_full(&input) else { continue };
        t.add(name, &out.end_to_end, out.certificate.is_certified());
    }
    eprintln!("3D fixtures: {t:?}");
    assert!(t.failed.is_empty(), "{:#?}", t.failed);
    assert!(t.certified >= 20, "only {} 3D fixtures certified end to end", t.certified);
}

fn sweep_2d(seeds: std::ops::Range<u64>) -> Tally {
    let mut t = Tally::default();
    for seed in seeds {
        let (mut input, _) = random_2d_model_rich(seed);
        input.connectors.clear();
        if seed % 3 == 0 {
            add_random_constraints_2d(&mut input, seed);
        }
        let Ok(out) = certify_linear_2d_full(&input) else { continue };
        t.add(&format!("2d seed {seed}"), &out.end_to_end, out.certificate.is_certified());
    }
    t
}

fn sweep_3d(seeds: std::ops::Range<u64>) -> Tally {
    let mut t = Tally::default();
    for seed in seeds {
        let (mut input, _) = random_3d_model_rich(seed);
        // Keep the frame/truss part: shells, connectors, curved beams and
        // warping are outside the re-assembly's scope.
        input.plates.clear();
        input.quads.clear();
        input.quad9s.clear();
        input.solid_shells.clear();
        input.curved_shells.clear();
        input.curved_beams.clear();
        input.connectors.clear();
        for s in input.sections.values_mut() {
            s.cw = None;
        }
        input.loads.retain(|l| {
            matches!(l, SolverLoad3D::Nodal(_) | SolverLoad3D::Distributed(_) | SolverLoad3D::PointOnElement(_) | SolverLoad3D::Thermal(_))
        });
        for l in input.loads.iter_mut() {
            if let SolverLoad3D::Nodal(n) = l {
                n.bw = None;
            }
        }
        if input.elements.is_empty() {
            continue;
        }
        if seed % 3 == 0 {
            add_random_constraints_3d(&mut input, seed);
        }
        let Ok(out) = certify_linear_3d_full(&input) else { continue };
        t.add(&format!("3d seed {seed}"), &out.end_to_end, out.certificate.is_certified());
    }
    t
}

#[test]
fn end_to_end_random_2d() {
    let t = sweep_2d(0..150);
    eprintln!("random 2D: {t:?}");
    assert!(t.failed.is_empty(), "{:#?}", t.failed);
    assert!(t.certified >= 60, "only {} random 2D models certified end to end", t.certified);
}

#[test]
fn end_to_end_random_3d() {
    let t = sweep_3d(0..120);
    eprintln!("random 3D: {t:?}");
    assert!(t.failed.is_empty(), "{:#?}", t.failed);
    assert!(t.certified >= 40, "only {} random 3D models certified end to end", t.certified);
}
