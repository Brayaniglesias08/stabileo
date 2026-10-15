//! Certificate bundles (docs/verification/CERTIFY.md, "Independent checking:
//! certificate bundles"): export bundles for fixture models, plus tampered copies
//! whose claims are false, into `target/cert-bundles/` for
//! `verify/scripts/check-bundles.sh` (`lake exe certcheck`).

use dedaliano_engine::certify::bundle::{export_linear_bundle_3d, from_dyadic, to_dyadic, LinearBundle};
use dedaliano_engine::types::*;
use std::path::PathBuf;
use std::time::Instant;

const FIXTURES: [&str; 3] = ["ex-3d-portal-frame", "ex-3d-tower", "ex-3d-nave-industrial"];

fn out_dir() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../target")));
    let d = target.join("cert-bundles");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn load(name: &str) -> SolverInput3D {
    let p = format!("{}/tests/fixtures/{name}-input.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn write(dir: &PathBuf, name: &str, b: &LinearBundle) {
    std::fs::write(dir.join(format!("{name}.json")), b.to_json()).unwrap();
}

/// Tampered copies whose claims are false (or, for the bound, far tighter than the
/// residual allows): `certcheck` must reject every one of them.
fn write_tampered(dir: &PathBuf, name: &str, b: &LinearBundle) {
    let bound = from_dyadic(b.bound).unwrap();

    // Bound shrunk 1024×: ρ² > (b μ)² for any realistic residual.
    let mut t = b.clone();
    t.bound = to_dyadic(bound / 1024.0).unwrap();
    write(dir, &format!("{name}.tampered-bound"), &t);

    // ũ moved by 4b in one component, bound kept: ‖u* − ũ'‖ ≥ 4b − b > b, so the
    // claim is false and a sound checker must reject it.
    let mut t = b.clone();
    let u: Vec<f64> = b.u.iter().map(|&d| from_dyadic(d).unwrap()).collect();
    let k = (0..u.len()).max_by(|&i, &j| u[i].abs().total_cmp(&u[j].abs())).unwrap();
    let moved = u[k] + 4.0 * bound;
    assert!((moved - u[k]).abs() >= 3.9 * bound, "{name}: perturbation lost to rounding");
    t.u[k] = to_dyadic(moved).unwrap();
    write(dir, &format!("{name}.tampered-u"), &t);

    // μ = 2·min Kᵢᵢ > λ_min(K) (Rayleigh quotient of eᵢ): claim false.
    let mut t = b.clone();
    let mut dmin = f64::INFINITY;
    for j in 0..b.n {
        for p in b.k.col_ptr[j]..b.k.col_ptr[j + 1] {
            if b.k.row_idx[p] == j {
                dmin = dmin.min(from_dyadic(b.k.values[p]).unwrap());
            }
        }
    }
    t.mu = to_dyadic(2.0 * dmin).unwrap();
    write(dir, &format!("{name}.tampered-mu"), &t);
}

#[test]
fn export_bundles() {
    let dir = out_dir();
    for name in FIXTURES {
        let input = load(name);
        let t = Instant::now();
        let b = export_linear_bundle_3d(&input).unwrap_or_else(|e| panic!("{name}: {e}"));
        let json = b.to_json();
        // Exact data must round-trip bit for bit (`info` is display-only f64 text).
        let mut back = LinearBundle::from_json(&json).unwrap();
        back.info = b.info.clone();
        assert_eq!(back, b, "{name}: JSON round trip");
        eprintln!(
            "{name}: n={} nnz(K)={} nnz(L)={} μ={:.3e} bound={:.3e} rel={:.2e} {} bytes, {:.0} ms",
            b.n,
            b.k.values.len(),
            b.l.values.len(),
            b.info.mu,
            b.info.bound,
            b.info.rel_bound,
            json.len(),
            t.elapsed().as_secs_f64() * 1e3
        );
        std::fs::write(dir.join(format!("{name}.json")), json).unwrap();
        write_tampered(&dir, name, &b);
    }
    eprintln!("bundles written to {}", dir.display());
}

