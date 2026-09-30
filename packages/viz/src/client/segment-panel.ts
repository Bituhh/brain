// Dendritic segment drill-down (VIZ-3, Phase 6 Requirement 12).
//
// Segment membership is derived from the already-held synapse topology
// (grouped by `targetSegment` for synapses targeting the selected
// neuron) rather than a separate request -- the client already has this
// data. Per-tick segment *activity* comes from the live `probeData`
// broadcasts the server sends once a probe is attached (Requirement 6),
// wired here via `onOpen`/`onClose` callbacks that the caller (`main.ts`)
// turns into `attachProbe`/`detachProbe` control messages.

export interface SegmentMember {
  readonly synapseId: number;
  readonly source: number;
  readonly permanence: number;
  /** docs/prior-art.md §2.5's efficacy -- docs/decisions.md's weight/permanence split (2026-09-13). */
  readonly weight: number;
}

export interface SegmentActivitySample {
  readonly tick: number;
  readonly segment: number;
  /** Signed and fractional: negative is a veto by inhibitory deliveries (see `SegmentSampleWire.active`). */
  readonly active: number;
  readonly depolarisation: number;
}

/**
 * One drill-down line. A negative count is labelled VETOED so it cannot be
 * mistaken for an untouched segment (PLAN.md C10). Counts print to two
 * decimals, because weighted votes (B5) make them fractional. Exported for
 * `segment-panel.test.ts`: the rest of this panel is DOM code with no test
 * harness (VIZ-3's deferral in `check-requirement-coverage.mjs`), but this
 * part is a pure function.
 */
export function formatSegmentActivity(s: SegmentActivitySample): string {
  const count = Number.isInteger(s.active)
    ? String(s.active)
    : s.active.toFixed(2);
  const state =
    s.active < 0 ? '  VETOED' : s.depolarisation > 0 ? '  FIRED' : '';
  return `tick ${s.tick}  segment ${s.segment}  active=${count}${state}`;
}

export interface SegmentPanelCallbacks {
  onOpen(neuron: number): void;
  onClose(neuron: number): void;
}

export class SegmentPanel {
  readonly #root: HTMLElement;
  readonly #callbacks: SegmentPanelCallbacks;
  #currentNeuron: number | undefined;

  constructor(root: HTMLElement, callbacks: SegmentPanelCallbacks) {
    this.#root = root;
    this.#callbacks = callbacks;
    this.#root.hidden = true;
  }

  isOpenFor(neuron: number): boolean {
    return this.#currentNeuron === neuron;
  }

  open(
    neuron: number,
    membersBySegment: ReadonlyMap<number, readonly SegmentMember[]>,
  ): void {
    if (this.#currentNeuron !== undefined && this.#currentNeuron !== neuron) {
      this.#callbacks.onClose(this.#currentNeuron);
    }
    this.#currentNeuron = neuron;
    this.#root.hidden = false;
    this.#renderStructure(neuron, membersBySegment);
    this.#callbacks.onOpen(neuron);
  }

  close(): void {
    if (this.#currentNeuron !== undefined) {
      this.#callbacks.onClose(this.#currentNeuron);
    }
    this.#currentNeuron = undefined;
    this.#root.hidden = true;
    this.#root.replaceChildren();
  }

  /** Fed by `main.ts` on every `probeData` message for the currently-open neuron. */
  updateActivity(
    neuron: number,
    samples: readonly SegmentActivitySample[] | undefined,
  ): void {
    if (neuron !== this.#currentNeuron) return;
    const activityEl = this.#root.querySelector<HTMLElement>(
      "[data-role='activity']",
    );
    if (!activityEl) return;
    if (!samples || samples.length === 0) {
      activityEl.textContent = '(no segment activity recorded yet)';
      return;
    }
    const lines = samples.slice(-20).map(formatSegmentActivity);
    activityEl.textContent = lines.join('\n');
  }

  #renderStructure(
    neuron: number,
    membersBySegment: ReadonlyMap<number, readonly SegmentMember[]>,
  ): void {
    this.#root.replaceChildren();

    const heading = document.createElement('h3');
    heading.textContent = `Neuron ${neuron}`;
    this.#root.appendChild(heading);

    const closeButton = document.createElement('button');
    closeButton.type = 'button';
    closeButton.textContent = 'Close';
    closeButton.addEventListener('click', () => this.close());
    this.#root.appendChild(closeButton);

    if (membersBySegment.size === 0) {
      const note = document.createElement('p');
      note.textContent = 'This neuron has no dendritic segments configured.';
      this.#root.appendChild(note);
      return;
    }

    const segments = [...membersBySegment.keys()].sort((a, b) => a - b);
    for (const segment of segments) {
      const members = membersBySegment.get(segment) ?? [];
      const segHeading = document.createElement('h4');
      segHeading.textContent = `Segment ${segment} (${members.length} synapse${members.length === 1 ? '' : 's'})`;
      this.#root.appendChild(segHeading);

      const list = document.createElement('ul');
      for (const member of members) {
        const item = document.createElement('li');
        item.textContent = `from neuron ${member.source}, permanence ${member.permanence.toFixed(2)}, weight ${member.weight.toFixed(2)}`;
        list.appendChild(item);
      }
      this.#root.appendChild(list);
    }

    const activityHeading = document.createElement('h4');
    activityHeading.textContent = 'Recent activity';
    this.#root.appendChild(activityHeading);

    const activity = document.createElement('pre');
    activity.dataset['role'] = 'activity';
    activity.textContent = '(waiting for activity...)';
    this.#root.appendChild(activity);
  }
}
