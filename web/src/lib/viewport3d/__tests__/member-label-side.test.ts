/**
 * A member's number and length in the 3D view go beside the member as the camera shows it:
 * judged by the world's Z alone, a beam along Y seen in plan runs up the screen and had its
 * labels written across its line.
 */
import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { keepLabelBeside } from '../results-sync';

const BESIDE: [number, number] = [1.12, 0.5];
const ACROSS: [number, number] = [0.5, -0.1];

function seenFrom(sprite: THREE.Sprite, position: THREE.Vector3, up = new THREE.Vector3(0, 0, 1)) {
  const camera = new THREE.OrthographicCamera(-10, 10, 10, -10, 0.1, 100);
  camera.up.copy(up);
  camera.position.copy(position);
  camera.lookAt(0, 0, 0);
  camera.updateMatrixWorld();
  sprite.onBeforeRender(null as never, null as never, camera, null as never, null as never, null as never);
  return [sprite.center.x, sprite.center.y];
}

describe('the side of a member label in 3D', () => {
  it('a beam along Y seen in plan runs up the screen: its label goes beside it', () => {
    const sprite = new THREE.Sprite();
    keepLabelBeside(sprite, { x: 0, y: 0, z: 0 }, { x: 0, y: 4, z: 0 }, BESIDE, ACROSS);
    // Plan: looking down, Y up the screen.
    expect(seenFrom(sprite, new THREE.Vector3(0, 0, 20), new THREE.Vector3(0, 1, 0))).toEqual(BESIDE);
    // From the side (looking along X), the same beam runs across the screen: over it.
    expect(seenFrom(sprite, new THREE.Vector3(20, 0, 0))).toEqual(ACROSS);
  });

  it('a column stays beside, from the front and seen end-on in plan', () => {
    const sprite = new THREE.Sprite();
    keepLabelBeside(sprite, { x: 0, y: 0, z: 0 }, { x: 0, y: 0, z: 3 }, BESIDE, ACROSS);
    expect([sprite.center.x, sprite.center.y]).toEqual(BESIDE);
    expect(seenFrom(sprite, new THREE.Vector3(0, -20, 0))).toEqual(BESIDE);
    expect(seenFrom(sprite, new THREE.Vector3(0, 0, 20), new THREE.Vector3(0, 1, 0))).toEqual(BESIDE);
  });

  it('a beam along X in plan runs across: over it', () => {
    const sprite = new THREE.Sprite();
    keepLabelBeside(sprite, { x: 0, y: 0, z: 0 }, { x: 4, y: 0, z: 0 }, BESIDE, ACROSS);
    expect(seenFrom(sprite, new THREE.Vector3(0, 0, 20), new THREE.Vector3(0, 1, 0))).toEqual(ACROSS);
  });
});
