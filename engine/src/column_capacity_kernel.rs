//! Batched rectangular-column capacity for station verification: the design moment at the
//! design axial force, through the CIRSOC 201 section engine (`postprocess::cirsoc201_section`),
//! so the station verifier, the legacy designer's gate and the calculator read one
//! implementation. Decimal presentation rounding stays in TS.
use wasm_bindgen::prelude::*;

use crate::postprocess::cirsoc201_check::Cirsoc201Edition;
use crate::postprocess::cirsoc201_section::{Confinement, Section, SectionCode};

fn solve_batch(constants: &[f64], bars: &[f64], loads: &[f64]) -> Result<Vec<f64>, &'static str> {
    if !(constants.len() == 8 || constants.len() == 9)
        || bars.len() % 2 != 0
        || constants.iter().chain(bars).chain(loads).any(|v| !v.is_finite())
    {
        return Err("Invalid column capacity buffers");
    }
    let (depth, width, fc_kpa, fy_kpa, fy, beta, axial_cap, alpha) =
        (constants[0], constants[1], constants[2], constants[3], constants[4], constants[5], constants[6], constants[7]);
    if depth <= 0. || width <= 0. || fc_kpa <= 0. || fy_kpa <= 0. || fy <= 0. || beta <= 0. || alpha <= 0.
        || bars.chunks_exact(2).any(|b| b[1] < 0.)
    {
        return Err("Invalid column capacity section");
    }
    // The ninth constant names the edition (2005 | 2025); absent, 2025.
    let edition = match constants.get(8) {
        None => Cirsoc201Edition::E2025,
        Some(&e) => crate::rc_section_kernel::edition_of(e)?,
    };
    // One implementation: the CIRSOC 201 section engine, on the rectangle with the bars at their
    // depths from the compressed face (x is irrelevant to bending about this axis).
    let (hx, hy) = (width / 2.0, depth / 2.0);
    let rings = vec![vec![[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]]];
    let section_bars = bars.chunks_exact(2).map(|b| [0.0, hy - b[0], b[1]]).collect();
    let code = SectionCode {
        fc: fc_kpa * 1e3,
        fy: fy_kpa * 1e3,
        es: 200e9,
        edition,
        confinement: Confinement::Ties,
        deduct_displaced_concrete: true,
    };
    let section = Section::new(rings, section_bars, code)?;
    // The caller's cap (kN) is the engine's own for ties; the smaller one binds.
    let cap = (axial_cap * 1e3).min(section.axial_cap());
    // φ·Pnt,max = 0,90·fy·Ast (22.4.3.1), kN.
    let tension_cap = -0.9 * fy_kpa * bars.chunks_exact(2).map(|b| b[1]).sum::<f64>();
    let mut out = Vec::with_capacity(loads.len() * 3);
    for &load in loads {
        // At or above the cap the plateau's widest point, where φ·Pn first reaches it.
        let target = (load * 1e3).min(cap * (1.0 - 1e-12));
        match section.at_axial(0.0, 1.0, target) {
            Some(p) => out.extend_from_slice(&[p.s.phi_mnx.abs() / 1e3, tension_cap, p.c]),
            None => out.extend_from_slice(&[0.0, tension_cap, f64::NAN]),
        }
    }
    Ok(out)
}

/// constants: [depth, width, fc_kPa, fy_kPa, fy_MPa, beta1, phiPn, alpha1, (edition)].
/// bars: [depth_from_compression_face, area_m2] per bar; loads: signed kN,
/// compression positive. Returns [phiMn, phiPtMax, unrounded_c] per load.
#[wasm_bindgen]
pub fn solve_column_capacity_batch(
    constants: &[f64],
    bars: &[f64],
    loads: &[f64],
) -> Result<Vec<f64>, JsValue> {
    solve_batch(constants, bars, loads).map_err(|e| JsValue::from_str(e))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> ([f64; 8], Vec<f64>) {
        let area = 8. * 2.01e-4;
        (
            [
                0.3,
                0.3,
                25000.,
                420000.,
                420.,
                0.85,
                0.65 * 0.80 * (0.85 * 25000. * (0.09 - area) + 420000. * area),
                0.85,
            ],
            (0..8)
                .flat_map(|i| [if i < 4 { 0.045 } else { 0.255 }, 2.01e-4])
                .collect(),
        )
    }
    #[test]
    fn batches_preserve_signed_loads_and_order() {
        let (c, b) = fixture();
        let loads = [1200., -400., 0., 400., 1200.];
        let batch = solve_batch(&c, &b, &loads).unwrap();
        for (i, &load) in loads.iter().enumerate() {
            assert_eq!(
                &batch[i * 3..i * 3 + 3],
                solve_batch(&c, &b, &[load]).unwrap()
            );
            assert_eq!(
                batch[i * 3 + 1],
                -0.9 * 420000. * b.chunks_exact(2).map(|b| b[1]).sum::<f64>()
            );
        }
        assert_ne!(batch[3], batch[9]);
        assert!(batch[2] > batch[11]);
        assert!(solve_batch(&c, &b, &[]).unwrap().is_empty());
    }
    #[test]
    fn rejects_malformed_and_nonfinite_buffers() {
        let (c, b) = fixture();
        assert!(solve_batch(&c[..7], &b, &[0.]).is_err());
        assert!(solve_batch(&c, &b[..3], &[0.]).is_err());
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(solve_batch(&c, &b, &[value]).is_err());
            let mut invalid = c;
            invalid[0] = value;
            assert!(solve_batch(&invalid, &b, &[0.]).is_err());
            let mut invalid = b.clone();
            invalid[0] = value;
            assert!(solve_batch(&c, &invalid, &[0.]).is_err());
        }
        let mut invalid = c;
        invalid[0] = 0.;
        assert!(solve_batch(&invalid, &b, &[0.]).is_err());
    }
}
