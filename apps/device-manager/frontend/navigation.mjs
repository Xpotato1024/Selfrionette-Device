// Presentation navigation only. It never sends a command or grants write permissions.
export const stepKeys = Object.freeze(['baseline', 'assessment', 'preview', 'confirmation', 'record']);
export function currentStep(view) {
  if (!view) return null;
  const index = view.steps.findIndex(step => step.state === 'current');
  return index >= 0 ? stepKeys[index] : view.saved_current ? 'record' : 'baseline';
}
export function canVisit(view, key) {
  if (!view) return false;
  const index = stepKeys.indexOf(key);
  return index >= 0 && (['complete','current'].includes(view.steps[index]?.state) || (key === 'baseline' && view.phase === 'not_started'));
}
export function createStepNavigation() {
  const positions = new Map();
  return {
    position(view) {
      if (!view) return null;
      const previous = positions.get(view.id);
      return previous && canVisit(view, previous) ? previous : currentStep(view);
    },
    visit(view, key) {
      if (!canVisit(view, key)) return false;
      positions.set(view.id, key); return true;
    },
    follow(view) { if (view) positions.set(view.id, currentStep(view)); },
  };
}
