// Interactive control panel (Phase 6 Requirement 8, client side): wires
// DOM controls to the callbacks `main.ts` turns into control-channel
// messages. No simulation-mutating capability is invented here beyond
// what the FFI already exposes (Requirement 8.1) -- every callback below
// maps to exactly one `ClientMessage` variant.

export interface ControlsCallbacks {
  onPause(): void;
  onResume(): void;
  onStepOnce(): void;
  onStimulate(index: number, current: number): void;
  onReward(amount: number): void;
  onSetStateStride(stride: number): void;
  onRequestRaster(): void;
  onRequestMetricsSnapshot(): void;
  onToggleShowPotentialSynapses(show: boolean): void;
}

function byData<T extends Element>(root: ParentNode, attr: string, value: string): T | null {
  return root.querySelector<T>(`[${attr}="${value}"]`);
}

export function wireControls(root: ParentNode, callbacks: ControlsCallbacks): void {
  byData<HTMLButtonElement>(root, "data-action", "pause")?.addEventListener("click", () => callbacks.onPause());
  byData<HTMLButtonElement>(root, "data-action", "resume")?.addEventListener("click", () => callbacks.onResume());
  byData<HTMLButtonElement>(root, "data-action", "step")?.addEventListener("click", () => callbacks.onStepOnce());
  byData<HTMLButtonElement>(root, "data-action", "request-raster")?.addEventListener("click", () => callbacks.onRequestRaster());
  byData<HTMLButtonElement>(root, "data-action", "request-metrics")?.addEventListener("click", () => callbacks.onRequestMetricsSnapshot());

  const stimulateForm = byData<HTMLFormElement>(root, "data-form", "stimulate");
  stimulateForm?.addEventListener("submit", (event) => {
    event.preventDefault();
    const data = new FormData(stimulateForm);
    const index = Number(data.get("index"));
    const current = Number(data.get("current"));
    if (Number.isFinite(index) && Number.isFinite(current)) callbacks.onStimulate(index, current);
  });

  const rewardForm = byData<HTMLFormElement>(root, "data-form", "reward");
  rewardForm?.addEventListener("submit", (event) => {
    event.preventDefault();
    const data = new FormData(rewardForm);
    const amount = Number(data.get("amount"));
    if (Number.isFinite(amount)) callbacks.onReward(amount);
  });

  const strideInput = byData<HTMLInputElement>(root, "data-input", "stride");
  strideInput?.addEventListener("change", () => {
    const value = Number(strideInput.value);
    if (Number.isFinite(value) && value >= 1) callbacks.onSetStateStride(Math.floor(value));
  });

  const potentialToggle = byData<HTMLInputElement>(root, "data-input", "show-potential");
  potentialToggle?.addEventListener("change", () => callbacks.onToggleShowPotentialSynapses(potentialToggle.checked));
}
