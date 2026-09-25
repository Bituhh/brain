// GridWorld (Requirement 16.4): a synthetic, seeded 2-D symbol grid with a
// cursor. The agent observes only the cell under the cursor plus its own
// last action, and can move in four directions. Realism is explicitly not
// the goal; a closed causal loop is (Requirement 16.3) -- deliberately no
// location signal, no grid cells, no reference frame (that is NET-9,
// Phase 5.5's job, and IO-5 landing here is what unblocks it).

export type Action = 'up' | 'down' | 'left' | 'right';

export interface Observation {
  readonly cell: string;
  readonly lastAction: Action | undefined;
}

export interface GridWorldConfig {
  readonly width: number;
  readonly height: number;
  readonly seed: number;
  /** The symbol alphabet the grid is filled with (besides `distinguishingCell`, if given). */
  readonly symbols: ReadonlyArray<string>;
  /**
   * Places a distinguishing symbol at exactly one cell -- the mechanism
   * Requirement 16.5's ablation needs: two otherwise-identical grids
   * (same seed, same symbols) differing only here, reachable only by
   * actually moving the cursor there.
   */
  readonly distinguishingCell?: {
    readonly x: number;
    readonly y: number;
    readonly symbol: string;
  };
  /**
   * Places the *same* symbol at a second specific cell (Phase 5.5
   * Requirement 6's reference-frame disambiguation task: the same local
   * sensory pattern recurring at two different locations, distinguishable
   * only by a location signal). Deliberately a separate field from
   * `distinguishingCell` rather than a generalisation of it -- the two
   * serve different requirements (IO-5's ablation wants one unique,
   * reachable-only-by-moving cell; NET-9's wants a *repeated* one), and
   * keeping them separate means neither's existing behaviour or tests are
   * touched by the other's addition.
   */
  readonly repeatedCell?: {
    readonly x: number;
    readonly y: number;
    readonly symbol: string;
  };
  readonly startX?: number;
  readonly startY?: number;
}

// A tiny, deterministic, non-cryptographic PRNG (mulberry32) -- adequate
// for filling a small grid with symbols, not a dependency (ENG-6).
function mulberry32(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/**
 * The environment half of the sensorimotor loop (Requirement 16.1):
 * `observe`/`act` are the only two methods `runSensorimotorLoop` needs,
 * matching that module's `Environment<Obs, Act>` interface structurally.
 */
export class GridWorld {
  readonly #grid: string[][];
  readonly #width: number;
  readonly #height: number;
  #cursorX: number;
  #cursorY: number;
  #lastAction: Action | undefined;

  constructor(config: GridWorldConfig) {
    if (config.symbols.length === 0) {
      throw new RangeError('GridWorldConfig.symbols must be non-empty');
    }
    this.#width = config.width;
    this.#height = config.height;
    const rand = mulberry32(config.seed);
    this.#grid = [];
    for (let y = 0; y < config.height; y++) {
      const row: string[] = [];
      for (let x = 0; x < config.width; x++) {
        row.push(config.symbols[Math.floor(rand() * config.symbols.length)]!);
      }
      this.#grid.push(row);
    }
    if (config.distinguishingCell) {
      const { x, y, symbol } = config.distinguishingCell;
      if (x < 0 || x >= config.width || y < 0 || y >= config.height) {
        throw new RangeError(
          `distinguishingCell (${x}, ${y}) is outside the ${config.width}x${config.height} grid`,
        );
      }
      this.#grid[y]![x] = symbol;
    }
    if (config.repeatedCell) {
      const { x, y, symbol } = config.repeatedCell;
      if (x < 0 || x >= config.width || y < 0 || y >= config.height) {
        throw new RangeError(
          `repeatedCell (${x}, ${y}) is outside the ${config.width}x${config.height} grid`,
        );
      }
      this.#grid[y]![x] = symbol;
    }
    this.#cursorX = config.startX ?? Math.floor(config.width / 2);
    this.#cursorY = config.startY ?? Math.floor(config.height / 2);
  }

  get cursor(): { readonly x: number; readonly y: number } {
    return { x: this.#cursorX, y: this.#cursorY };
  }

  /** The cell under the cursor plus the agent's own last action -- nothing else is observable (Requirement 16.1). */
  observe(): Observation {
    return {
      cell: this.#grid[this.#cursorY]![this.#cursorX]!,
      lastAction: this.#lastAction,
    };
  }

  /** Moves the cursor one cell (clamped to the grid's edges), changing what the *next* `observe()` call returns (Requirement 16.3). */
  act(action: Action): void {
    this.#lastAction = action;
    switch (action) {
      case 'up':
        this.#cursorY = Math.max(0, this.#cursorY - 1);
        break;
      case 'down':
        this.#cursorY = Math.min(this.#height - 1, this.#cursorY + 1);
        break;
      case 'left':
        this.#cursorX = Math.max(0, this.#cursorX - 1);
        break;
      case 'right':
        this.#cursorX = Math.min(this.#width - 1, this.#cursorX + 1);
        break;
    }
  }
}
