/**
 * The rules every CIRSOC 201 calculation in this application shares — asked of the engine.
 *
 * β1, φ from the net tensile strain, the axial caps and the minimum steel are computed in Rust
 * (`cirsoc201_basis`, `cirsoc201_phi`; `engine/src/postprocess/cirsoc201_check.rs` and
 * `cirsoc201_section.rs`), where they are tested once for every section shape. This module
 * converts units and holds no clause. The TypeScript that used to compute them is kept as a test
 * oracle (`__tests__/differential/oracle/cirsoc201-basis.ts`).
 *
 * The edition is part of the question. CIRSOC 201-2005 is tension-controlled at εt = 0,005 and
 * gives a spiral φ = 0,70; CIRSOC 201-2025 (Tabla 21.2.2) at εty + 0,003 with 0,75. Callers that
 * do not name one get 2005, which is what these helpers computed before (the CIRSOC_FLEX
 * workbook's edition); PRO passes the edition it designs to.
 *
 * Units here: MPa, m, m², kN, cm² — the engine's SI is converted at this boundary.
 */
import type { RegulationEdition } from '../../../codes/regulation';
import { cirsoc201Engine } from './cirsoc201-engine';

export type Cirsoc201EditionLike = '2005' | '2025' | RegulationEdition;
export type Confinement = 'ties' | 'spiral';

/** Concrete's assumed ultimate compressive strain, 22.2.2.1 [10.2.3]. */
export const EPSILON_CU = 0.003;

/** Steel's modulus, MPa. */
export const ES_MPA = 200_000;

/** φ for a tension-controlled section, Tabla 21.2.2 (e)/(f) [9.3.2.1]. */
export const PHI_TENSION = 0.90;

/** φ for a compression-controlled section with closed ties, both editions. */
export const PHI_COMPRESSION_TIED = 0.65;

/** The 2005 edition's tension-controlled strain, 10.3.4. Use `tensionControlledStrain` for 2025. */
export const EPSILON_TENSION_CONTROLLED = 0.005;

export const editionCode = (e: Cirsoc201EditionLike = '2005'): number => (e === '2005' ? 2005 : 2025);
export const confinementCode = (c: Confinement = 'ties'): number => (c === 'spiral' ? 1 : 0);

function basis(fc: number, fy: number, opts: {
  edition?: Cirsoc201EditionLike; confinement?: Confinement; Ag?: number; Ast?: number; bw?: number; d?: number;
} = {}): Float64Array {
  return cirsoc201Engine('basis')(Float64Array.of(
    editionCode(opts.edition), fc * 1e6, fy * 1e6, ES_MPA * 1e6, confinementCode(opts.confinement),
    opts.Ag ?? 0, opts.Ast ?? 0, opts.bw ?? 0, opts.d ?? 0,
  ));
}

/** β1, 22.2.2.4.3 [10.2.7.3]. `fc` MPa. */
export function beta1(fc: number): number {
  return basis(fc, 420)[0];
}

/** Yield strain of the reinforcement, fy/Es (21.2.2.1). */
export function yieldStrain(fy: number): number {
  return fy / ES_MPA;
}

/** The εt at and above which a section is tension-controlled. */
export function tensionControlledStrain(fy: number, edition: Cirsoc201EditionLike = '2005'): number {
  return basis(25, fy, { edition })[3];
}

/** The least εt a beam may have, 9.3.3.1 [10.3.5]. */
export function ductilityStrain(fy: number, edition: Cirsoc201EditionLike = '2005'): number {
  return basis(25, fy, { edition })[4];
}

/** φ for a compression-controlled section, Tabla 21.2.2 [9.3.2.2]. */
export function phiCompression(confinement: Confinement = 'ties', edition: Cirsoc201EditionLike = '2005'): number {
  return basis(25, 420, { edition, confinement })[1];
}

/** φ from the net tensile strain of the extreme tension steel, Tabla 21.2.2 [9.3.2]. */
export function phiFromStrain(
  epsT: number,
  fy: number,
  confinement: Confinement = 'ties',
  edition: Cirsoc201EditionLike = '2005',
): number {
  return cirsoc201Engine('phi')(editionCode(edition), confinementCode(confinement), fy * 1e6, ES_MPA * 1e6,
    Float64Array.of(Math.abs(epsT)))[0];
}

/** The nominal squash load Po (22.4.2.2). `Ag`, `Ast` m², result kN. */
export function squashLoad(fc: number, fy: number, Ag: number, Ast: number): number {
  return basis(fc, fy, { Ag, Ast })[5] / 1000;
}

/** φ·Pn,max, Tabla 22.4.2.1 [10.3.6], kN. */
export function axialCap(
  fc: number, fy: number, Ag: number, Ast: number,
  confinement: Confinement = 'ties',
  edition: Cirsoc201EditionLike = '2005',
): number {
  return basis(fc, fy, { Ag, Ast, confinement, edition })[6] / 1000;
}

/** Column longitudinal steel bounds as a fraction of Ag, 10.6.1.1 [10.9.1]. */
export const COLUMN_STEEL_RATIO = { min: 0.01, max: 0.08 } as const;

/** 9.6.1.2's minimum flexural steel, cm²; `bw` the web width and `d` the effective depth, m. */
export function minFlexuralSteelCm2(fc: number, fy: number, bw: number, d: number): number {
  return basis(fc, fy, { bw, d })[7] * 1e4;
}
