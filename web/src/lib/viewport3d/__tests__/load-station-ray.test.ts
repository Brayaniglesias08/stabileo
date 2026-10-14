import { describe, expect, it } from 'vitest';
import * as THREE from 'three';
import { stationOnRay } from '../load-handles';

describe('placing a member load from a pointer ray', () => {
  const I = new THREE.Vector3(-2, 0, -5);
  const J = new THREE.Vector3(2, 0, -10);
  const segment = { I, J, L: I.distanceTo(J) };

  for (const projection of ['perspective', 'orthographic'] as const) {
    it.each([0.1, 0.5, 0.9])(`${projection}: places a load at fraction %s of the physical length`, (fraction) => {
      const camera = projection === 'perspective'
        ? new THREE.PerspectiveCamera(50, 1, 0.1, 100)
        : new THREE.OrthographicCamera(-4, 4, 4, -4, 0.1, 100);
      camera.updateMatrixWorld();
      const screen = I.clone().lerp(J, fraction).project(camera);
      const raycaster = new THREE.Raycaster();
      raycaster.setFromCamera(new THREE.Vector2(screen.x, screen.y), camera);
      expect(stationOnRay(segment, raycaster.ray)).toBeCloseTo(fraction * segment.L, 10);
    });
  }

  it('clamps clicks beside an endpoint to the member', () => {
    const segment = { I: new THREE.Vector3(0, 0, -5), J: new THREE.Vector3(6, 0, -5), L: 6 };
    for (const [x, expected] of [[-0.1, 0], [6.1, 6]]) {
      const ray = new THREE.Ray(new THREE.Vector3(x, 0.02, 0), new THREE.Vector3(0, 0, -1));
      expect(stationOnRay(segment, ray)).toBe(expected);
    }
  });
});
