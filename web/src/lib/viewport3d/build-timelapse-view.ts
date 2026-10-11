/**
 * Draws the build timelapse on the 3D scene.
 *
 * Works on the meshes the viewport already keeps (one group per member, per shell, per
 * support): each frame it gives every piece the transform for how far it has come in and hides
 * what is not standing yet. While it runs it also sets the stage: background, lights, fog and
 * the members' finish for the chosen style, the glow on the pieces going in, and the camera's
 * turn. Everything else in the scene (nodes, loads, labels, results) is hidden, and all of it is
 * given back as it was on `restore`.
 */
import * as THREE from 'three';
import { modelStore } from '../store';
import { buildTimelapse, HOLD_S, LEAD_S, type TimelapseStyle } from '../store/build-timelapse.svelte';
import { projectNodeToScene } from '../geometry/coordinate-system';
import {
  buildFront, buildSchedule, dropMatrix, growMatrix, growthAt, piecesStarted,
  type BuildPiece, type BuildSchedule, type BuildStep, type P3,
} from './build-timelapse';

export interface TimelapseRefs {
  scene: THREE.Scene;
  elementGroups: Map<number, THREE.Group>;
  shellGroups: Map<string, THREE.Group>;
  supportGizmos: Map<number, THREE.Group>;
  /** Scene children the timelapse keeps drawn (members, shells, supports). */
  keep: Set<THREE.Object3D>;
  /** Scene children kept unless the background is clean (the grids). */
  background: Set<THREE.Object3D>;
  /** Whether a planar model is drawn in the XZ plane. */
  project2D: boolean;
}

/** What the picture's texts need from the frame just drawn. */
export interface TimelapseFrameInfo {
  started: number;
  pieces: number;
  progress: number;
  /** 0 to 1 during the hold at the end, for the closing card. */
  closing: number;
}

/** Radians per second while the camera turns: a full turn in a minute. */
const ORBIT_RAD_S = (2 * Math.PI) / 60;
/** Seconds a piece keeps glowing after it is in place. */
const AFTERGLOW_S = 0.7;
const ACCENT = new THREE.Color('#e5482a');
const Z = new THREE.Vector3(0, 0, 1);

interface Look {
  top: string; bottom: string; fog: string;
  steel: number; metalness: number; roughness: number;
  sky: number; ground: number; hemi: number; key: number;
}
const LOOKS: Record<Exclude<TimelapseStyle, 'model'>, Look> = {
  night: {
    top: '#1b2838', bottom: '#05080d', fog: '#0b121b',
    steel: 0xc6cfd9, metalness: 0.55, roughness: 0.32,
    sky: 0xdfe8f5, ground: 0x1a2230, hemi: 0.75, key: 0.95,
  },
  studio: {
    top: '#f7f9fb', bottom: '#d3dae3', fog: '#e4e9ef',
    steel: 0x5f6f80, metalness: 0.35, roughness: 0.45,
    sky: 0xffffff, ground: 0xaab4bf, hemi: 0.85, key: 0.75,
  },
};

function gradient(top: string, bottom: string): THREE.CanvasTexture {
  const c = document.createElement('canvas');
  c.width = 4; c.height = 512;
  const g = c.getContext('2d')!;
  const grad = g.createLinearGradient(0, 0, 0, 512);
  grad.addColorStop(0, top);
  grad.addColorStop(1, bottom);
  g.fillStyle = grad;
  g.fillRect(0, 0, 4, 512);
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

interface MatSaved { color: number; emissive: number; emissiveIntensity: number; metalness: number; roughness: number }

export function createTimelapseView() {
  let schedule: BuildSchedule | null = null;
  let scheduleSig = '';
  const stepOf = new Map<string, BuildStep>();
  const shown = new Map<THREE.Object3D, boolean>();
  const touched = new Set<THREE.Object3D>();
  const mats = new Map<THREE.MeshStandardMaterial, MatSaved>();
  const m16: number[] = new Array(16);
  const grow = new THREE.Matrix4();
  const base = new THREE.Matrix4();
  const scaleM = new THREE.Matrix4();
  const back = new THREE.Matrix4();
  const offset = new THREE.Vector3();
  let lastNow = -1;

  // The stage: saved on the first frame, given back on restore.
  let stageStyle: TimelapseStyle | null = null;
  let savedBackground: THREE.Scene['background'] = null;
  let savedFog: THREE.Scene['fog'] = null;
  let bgTexture: THREE.CanvasTexture | null = null;
  let lights: THREE.Group | null = null;
  let keyLight: THREE.DirectionalLight | null = null;
  let staged = false;

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

  function setStage(scene: THREE.Scene): void {
    const style = buildTimelapse.style;
    if (!staged) {
      savedBackground = scene.background;
      savedFog = scene.fog;
      staged = true;
    }
    if (style === stageStyle) return;
    stageStyle = style;
    bgTexture?.dispose(); bgTexture = null;
    if (lights) { scene.remove(lights); lights = null; keyLight = null; }
    if (style === 'model') {
      scene.background = savedBackground;
      scene.fog = savedFog;
      return;
    }
    const look = LOOKS[style];
    bgTexture = gradient(look.top, look.bottom);
    scene.background = bgTexture;
    scene.fog = new THREE.Fog(look.fog, 1, 1000);
    lights = new THREE.Group();
    lights.name = 'timelapseLights';
    lights.add(new THREE.HemisphereLight(look.sky, look.ground, look.hemi));
    keyLight = new THREE.DirectionalLight(0xffffff, look.key);
    lights.add(keyLight, keyLight.target);
    scene.add(lights);
  }

  /** The member's finish for the style, and its glow while it goes in. */
  function finish(group: THREE.Object3D, glow: number): void {
    const style = buildTimelapse.style;
    const look = style === 'model' ? null : LOOKS[style];
    group.traverse((o) => {
      if (o.userData?.pickingHelper) return;
      const mat = (o as THREE.Mesh).material as THREE.MeshStandardMaterial | undefined;
      if (!mat || !mat.isMeshStandardMaterial) return;
      let saved = mats.get(mat);
      if (!saved) {
        saved = { color: mat.color.getHex(), emissive: mat.emissive.getHex(), emissiveIntensity: mat.emissiveIntensity, metalness: mat.metalness, roughness: mat.roughness };
        mats.set(mat, saved);
      }
      if (look) { mat.color.setHex(look.steel); mat.metalness = look.metalness; mat.roughness = look.roughness; }
      else { mat.color.setHex(saved.color); mat.metalness = saved.metalness; mat.roughness = saved.roughness; }
      // The brand's vermilion: the colour itself turns, and a little light of its own keeps it
      // from going pink under the white lights.
      if (glow > 0.001) {
        mat.color.lerp(ACCENT, glow * 0.9);
        mat.emissive.copy(ACCENT);
        mat.emissiveIntensity = glow * 0.45;
      } else { mat.emissive.setHex(saved.emissive); mat.emissiveIntensity = saved.emissiveIntensity; }
    });
  }

  function place(obj: THREE.Object3D, step: BuildStep | null, tp: number, drop: number): void {
    touched.add(obj);
    const s = step ? growthAt(step, tp) : 1;
    if (s <= 0.002) { obj.visible = false; return; }
    obj.visible = true;
    obj.matrixAutoUpdate = false;
    base.compose(obj.position, obj.quaternion, obj.scale);
    if (s >= 1 || !step) obj.matrix.copy(base);
    else if (buildTimelapse.entry === 'drop') obj.matrix.fromArray(dropMatrix(drop, s, m16)).multiply(base);
    else obj.matrix.fromArray(growMatrix(step, s, m16)).multiply(base);
    obj.matrixWorldNeedsUpdate = true;
    let glow = 0;
    if (step && buildTimelapse.highlight) {
      glow = s < 1 ? 1 : Math.max(0, 1 - (tp - step.end) / AFTERGLOW_S);
    }
    finish(obj, glow);
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
    grow.makeTranslation(p.x, p.y, p.z).multiply(scaleM.makeScale(s, s, s)).multiply(back.makeTranslation(-p.x, -p.y, -p.z));
    obj.matrix.copy(grow).multiply(base);
    obj.matrixWorldNeedsUpdate = true;
  }

  /**
   * Draw the frame for the timelapse's current time. Returns null when it is off.
   */
  function frame(refs: TimelapseRefs, camera: THREE.Camera, target: THREE.Vector3, now: number): TimelapseFrameInfo | null {
    if (!buildTimelapse.active) { lastNow = -1; return null; }
    prepare(refs.project2D);
    setStage(refs.scene);
    const wasPlaying = buildTimelapse.playing;
    buildTimelapse.tick(now);
    const t = buildTimelapse.time;
    const sch = schedule!;

    const grid = !buildTimelapse.cleanBackground && buildTimelapse.style !== 'studio';
    for (const child of refs.scene.children) {
      if (child === lights) continue;
      if (!shown.has(child)) shown.set(child, child.visible);
      const keep = refs.keep.has(child) || (refs.background.has(child) && grid) || (child as THREE.Light).isLight === true;
      child.visible = keep;
    }

    const tp = t - LEAD_S;
    const drop = Math.max(2, (sch.zMax - sch.zMin) * 0.4 + 1);
    for (const [id, g] of refs.elementGroups) place(g, stepOf.get(`e${id}`) ?? null, tp, drop);
    for (const [key, g] of refs.shellGroups) place(g, stepOf.get(key) ?? null, tp, drop);
    const sup = [...refs.supportGizmos.values()];
    sup.forEach((g, k) => placeSupport(g, k, sup.length, t));

    // The camera: a slow turn, and on `rise` the view climbs with the build.
    const dt = wasPlaying && lastNow >= 0 ? Math.min(0.1, (now - lastNow) / 1000) : 0;
    if (dt > 0 && buildTimelapse.camera !== 'fixed') {
      offset.copy(camera.position).sub(target).applyAxisAngle(Z, ORBIT_RAD_S * dt);
      camera.position.copy(target).add(offset);
    }
    if (buildTimelapse.camera === 'rise') {
      const want = sch.zMin + (buildFront(sch, tp) - sch.zMin) * 0.6;
      const dz = (want - target.z) * Math.min(1, 3 * Math.max(dt, 1 / 60));
      target.z += dz;
      camera.position.z += dz;
    }
    if (buildTimelapse.camera !== 'fixed') camera.lookAt(target);
    lastNow = now;

    // The key light comes from over the viewer's shoulder; the fog thickens past the model.
    if (keyLight) {
      keyLight.position.copy(camera.position).add(offset.set(0, 0, camera.position.distanceTo(target) * 0.5));
      keyLight.target.position.copy(target);
    }
    if (refs.scene.fog instanceof THREE.Fog && stageStyle !== 'model') {
      const d = camera.position.distanceTo(target);
      refs.scene.fog.near = d * 1.1;
      refs.scene.fog.far = d * 3.2;
    }

    const total = buildTimelapse.total;
    return {
      started: piecesStarted(sch, tp),
      pieces: sch.steps.length,
      progress: total > LEAD_S ? Math.max(0, Math.min(1, tp / (total - LEAD_S))) : 1,
      closing: Math.max(0, Math.min(1, (t - total - 0.3) / Math.min(0.9, HOLD_S * 0.4))),
    };
  }

  /** Everything back as it was before the timelapse. */
  function restore(scene: THREE.Scene): void {
    for (const obj of touched) {
      obj.matrixAutoUpdate = true;
      obj.updateMatrix();
      obj.visible = true;
    }
    touched.clear();
    for (const [mat, s] of mats) {
      mat.color.setHex(s.color); mat.emissive.setHex(s.emissive); mat.emissiveIntensity = s.emissiveIntensity;
      mat.metalness = s.metalness; mat.roughness = s.roughness;
    }
    mats.clear();
    for (const [obj, v] of shown) obj.visible = v;
    shown.clear();
    if (staged) { scene.background = savedBackground; scene.fog = savedFog; }
    if (lights) scene.remove(lights);
    bgTexture?.dispose();
    lights = null; keyLight = null; bgTexture = null; stageStyle = null; staged = false;
    schedule = null;
    scheduleSig = '';
    lastNow = -1;
  }

  return { frame, restore };
}
