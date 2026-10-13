/**
 * The kinematic report of the model that is open: the input the Kinematic panel and the
 * calculation report both read, built in one place.
 *
 * In 3D it is built on the space model and the 3D rank check (the plane wire drops z, and a
 * space frame counted on it is its projection on XY). In 2D the sliding joints, which are not
 * on the solver wire, are passed explicitly so the count includes them.
 */
import { modelStore, uiStore } from '../store';
import { generateKinematicReport, generateKinematicReport3D, type KinematicReport, type SlidingJointInput } from './kinematic-report';

export function kinematicReportOfModel(is3D = uiStore.is3DWorkspace): KinematicReport | null {
  if (is3D) {
    let input3D: ReturnType<typeof modelStore.buildSolverInput3D> = null;
    try {
      // The model's members and nodes, none cut for a load inside its span.
      input3D = modelStore.buildSolverInput3D(false, uiStore.axisConvention3D === 'leftHand', { expandMemberOffsets: false, uncut: true });
    } catch {
      input3D = null;
    }
    return input3D ? generateKinematicReport3D(input3D) : null;
  }
  const input = modelStore.buildSolverInput(false);
  if (!input) return null;
  const slidingJoints: SlidingJointInput[] = [];
  for (const e of modelStore.elements.values()) {
    if (e.releaseI?.slide) slidingJoints.push({ elemId: e.id, end: 'I', kind: e.releaseI.slide, axis: e.releaseI.slideAxis ?? 'global' });
    if (e.releaseJ?.slide) slidingJoints.push({ elemId: e.id, end: 'J', kind: e.releaseJ.slide, axis: e.releaseJ.slideAxis ?? 'global' });
  }
  return generateKinematicReport(input, slidingJoints);
}
