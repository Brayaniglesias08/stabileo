//! A posteriori certification of solver results. See `docs/verification/CERTIFY.md`.

pub mod buckling;
pub mod constrained;
pub mod forces;
pub mod interval;
pub mod linear;
pub mod modal;
pub mod solver;
pub mod sparse;

pub use linear::{
    certify_linear_system, certify_linear_system_auto, Certificate, MAX_CERTIFY_N,
};
pub use solver::{
    certify_linear_2d, certify_linear_2d_full, certify_linear_3d, certify_linear_3d_full, certify_results_2d,
    certify_results_3d, CertifiedResults2D, CertifiedResults3D,
};
