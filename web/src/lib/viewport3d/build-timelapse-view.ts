/**
 * Draws the build timelapse on the 3D scene.
 *
 * Works on the meshes the viewport already keeps (one group per member, per shell, per
 * support): each frame it gives every piece the transform for how far it has grown and hides
 * what is not standing yet. Everything else in the scene (nodes, loads, labels, results) is
 * hidden while it runs, and all of it is given back as it was on `restore`.
 */
import * as THREE from 'three';
import { modelStore } from '../store';
import { buildTimelapse, LEAD_S } from '../store/build-timelapse.svelte';
import { projectNodeToScene } from '../geometry/coordinate-system';
import { buildSchedule, growMatrix, growthAt, type BuildPiece, type BuildSchedule, type BuildStep, type P3 } from './build-timelapse';

export interface TimelapseRefs {
  scene: THREE.Scene;
  elementGroups: Map<number, THREE.Group>;
  shellGroups: Map<string, THREE.Group>;
  supportGizmos: Map<number, THREE.Group>;
  /** Scene children the timelapse keeps drawn (members, shells, supports, lights). */
  keep: Set<THREE.Object3D>;
  /** Scene children kept unless the background is clean (the grids). */
  background: Set<THREE.Object3D>;
  /** Whether a planar model is drawn in the XZ plane. */
  project2D: boolean;
}

/** Turns per second while the camera orbits: a full turn in a minute. */
const ORBIT_RAD_S = (2 * Math.PI) / 60;
const Z = new THREE.Vector3(0, 0, 1);

export function createTimelapseView() {
  let schedule: BuildSchedule | null = null;
  let scheduleSig = '';
  const stepOf = new Map<string, BuildStep>();
  const shown = new Map<THREE.Object3D, boolean>();
  const touched = new Set<THREE.Object3D>();
  const m16: number[] = new Array(16);
  const grow = new THREE.Matrix4();
  const base = new THREE.Matrix4();
  const offset = new THREE.Vector3();
  let lastNow = -1;

  function pieces(project2D: boolean): BuildPiece[] {
    const at = (id: number) => {
      const n = modelStore.nodes.get(id);
      return n ? projectNodeToScene(n, project2D) : null;
    };
    const out: BuildPiece[] = [];
    for (const [id, e] of modelStore.elements) {
      const a = at(e.nodeI), b = at(e.nodeJ);
      if (a && b) out.push({ key: `e${id}`, kind: 'member', a, b, nodes: [e.nodeI, e.nodeJ] });
    }
    for (const [prefix, shells] of [['p', modelStore.plates], ['q', modelStore.quads]] as const) {
      for (const [id, s] of shells as Map<number, { nodes: number[] }>) {
        const pts = s.nodes.map(at);
        if (pts.every(Boolean)) out.push({ key: `${prefix}${id}`, kind: 'shell', pts: pts as P3[], nodes: [...s.nodes] });
      }
    }
    return out;
  }

  /** (Re)builds the schedule when the model or the options changed. */
  function prepare(project2D: boolean): void {
    const sig = `${modelStore.modelVersion}|${modelStore.elements.size}|${modelStore.plates.size}|${modelStore.quads.size}|${buildTimelapse.order}|${buildTimelapse.duration}|${project2D}`;
    if (sig === scheduleSig && schedule) return;
    scheduleSig = sig;
    const supported = new Set<number>();
    for (const s of modelStore.supports.values()) supported.add(s.nodeId);
    schedule = buildSchedule(pieces(project2D), buildTimelapse.order, { duration: buildTimelapse.duration, supportedNodes: supported });
    stepOf.clear();
    for (const s of schedule.steps) stepOf.set(s.key, s);
    buildTimelapse.total = LEAD_S + schedule.total;
  }

  function place(obj: THREE.Object3D, step: BuildStep | null, s: number): void {
    touched.add(obj);
    if (s <= 0.002) { obj.visible = false; return; }
    obj.visible = true;
    obj.matrixAutoUpdate = false;
    base.compose(obj.position, obj.quaternion, obj.scale);
    if (s >= 1 || !step) obj.matrix.copy(base);
    else obj.matrix.fromArray(growMatrix(step, s, m16)).multiply(base);
    obj.matrixWorldNeedsUpdate = true;
  }

  /** A support pops up at its node during the lead, a little after the one before it. */
  function placeSupport(obj: THREE.Object3D, k: number, n: number, t: number): void {
    touched.add(obj);
    const start = n > 1 ? (k / (n - 1)) * LEAD_S * 0.5 : 0;
    const u = Math.max(0, Math.min(1, (t - start) / (LEAD_S * 0.5)));
    const s = 1 - Math.pow(1 - u, 3);
    if (s <= 0.002) { obj.visible = false; return; }
    obj.visible = true;
    obj.matrixAutoUpdate = false;
    base.compose(obj.position, obj.quaternion, obj.scale);
    // About its own origin, which is the node.
    const p = obj.position;
    grow.makeTranslation(p.x, p.y, p.z).multiply(new THREE.Matrix4().makeScale(s, s, s)).multiply(new THREE.Matrix4().makeTranslation(-p.x, -p.y, -p.z));
    obj.matrix.copy(grow).multiply(base);
    obj.matrixWorldNeedsUpdate = true;
  }

  /**
   * Draw the frame for the timelapse's current time. Returns false when it is off.
   */
  function frame(refs: TimelapseRefs, camera: THREE.Camera, target: THREE.Vector3, now: number): boolean {
    if (!buildTimelapse.active) { lastNow = -1; return false; }
    prepare(refs.project2D);
    const wasPlaying = buildTimelapse.playing;
    buildTimelapse.tick(now);
    const t = buildTimelapse.time;

    for (const child of refs.scene.children) {
      if (!shown.has(child)) shown.set(child, child.visible);
      const keep = refs.keep.has(child) || (refs.background.has(child) && !buildTimelapse.cleanBackground) || (child as THREE.Light).isLight === true;
      child.visible = keep;
    }

    const tp = t - LEAD_S;
    for (const [id, g] of refs.elementGroups) {
      const step = stepOf.get(`e${id}`) ?? null;
      place(g, step, step ? growthAt(step, tp) : 1);
    }
    for (const [key, g] of refs.shellGroups) {
      const step = stepOf.get(key) ?? null;
      place(g, step, step ? growthAt(step, tp) : 1);
    }
    const sup = [...refs.supportGizmos.values()];
    sup.forEach((g, k) => placeSupport(g, k, sup.length, t));

    // A slow turn around the model while it plays.
    if (wasPlaying && buildTimelapse.orbit && lastNow >= 0) {
      const dt = Math.min(0.1, (now - lastNow) / 1000);
      offset.copy(camera.position).sub(target).applyAxisAngle(Z, ORBIT_RAD_S * dt);
      camera.position.copy(target).add(offset);
      camera.lookAt(target);
    }
    lastNow = now;
    return true;
  }

  /** Everything back as it was before the timelapse. */
  function restore(): void {
    for (const obj of touched) {
      obj.matrixAutoUpdate = true;
      obj.updateMatrix();
      obj.visible = true;
    }
    touched.clear();
    for (const [obj, v] of shown) obj.visible = v;
    shown.clear();
    schedule = null;
    scheduleSig = '';
    lastNow = -1;
  }

  return { frame, restore };
}
