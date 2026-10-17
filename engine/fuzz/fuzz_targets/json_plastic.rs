//! Plastic-collapse entry points of the WASM layer (engine/src/lib.rs) on
//! arbitrary bytes: the incremental solver and the exact-arithmetic collapse
//! certificate, 2D and 3D. Input: selector byte, parameter byte, JSON text.
#![no_main]

use dedaliano_engine::certify::{plastic as cplastic, plastic_3d as cplastic3};
use dedaliano_engine::solver::plastic;
use dedaliano_engine::types::*;
use dedaliano_engine_fuzz::{run, split};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((sel, _, json)) = split(data) else { return };
    match sel % 4 {
        0 => {
            run("solve_plastic_2d", json, |i: &PlasticInput| plastic::solve_plastic_2d(i));
        }
        1 => {
            run("solve_plastic_3d", json, |i: &PlasticInput3D| plastic::solve_plastic_3d(i));
        }
        2 => {
            run("certify_plastic_2d", json, |i: &PlasticInput| cplastic::certify_plastic_2d(i));
        }
        _ => {
            run("certify_plastic_3d", json, |i: &PlasticInput3D| cplastic3::certify_plastic_3d(i));
        }
    }
});
