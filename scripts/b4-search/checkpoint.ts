// The resumable run's memory: one JSON line per finished trial, appended the
// moment it finishes. Re-running the search reloads this file and skips
// every trial already in it; because the search's choices are computed from
// these results (and the trials are deterministic), a resumed run makes
// exactly the choices an uninterrupted one would.

import { appendFileSync, existsSync, readFileSync, openSync, fsyncSync, closeSync } from "node:fs";
import type { StructuralStats } from "@brain/core";
import type { ConsolidationStats } from "../../packages/io/src/milestone/charPrediction.ts";

export interface TrialRecord {
  readonly key: string;
  readonly label: string;
  readonly seed: string;
  readonly ok: boolean;
  readonly accuracy?: number;
  readonly structuralStats?: StructuralStats;
  /** PLAN.md C1: what the consolidation cadence did, when one was configured. Additive -- a pre-C1 checkpoint line simply has no such field. */
  readonly consolidationStats?: ConsolidationStats;
  readonly error?: string;
  readonly seconds: number;
  readonly finishedAt: string;
}

export class Checkpoint {
  readonly #path: string;
  readonly #records = new Map<string, TrialRecord>();
  #skippedLines = 0;

  constructor(path: string) {
    this.#path = path;
    if (!existsSync(path)) return;
    for (const line of readFileSync(path, "utf8").split("\n")) {
      if (line.trim() === "") continue;
      try {
        const record = JSON.parse(line) as TrialRecord;
        if (typeof record.key === "string" && typeof record.ok === "boolean") {
          // A later line for the same key (a retried failure) supersedes an earlier one.
          this.#records.set(record.key, record);
        } else {
          this.#skippedLines++;
        }
      } catch {
        // A line cut off by a crash or power loss mid-write: that trial is
        // simply run again.
        this.#skippedLines++;
      }
    }
  }

  get size(): number {
    return this.#records.size;
  }

  /** Lines that could not be read (e.g. cut off mid-write) and were ignored. */
  get skippedLines(): number {
    return this.#skippedLines;
  }

  get(key: string): TrialRecord | undefined {
    return this.#records.get(key);
  }

  /** Only successful trials count as done; a failed one is retried on the next run. */
  hasSucceeded(key: string): boolean {
    return this.#records.get(key)?.ok === true;
  }

  append(record: TrialRecord): void {
    // A leading newline guarantees a cut-off previous line never merges with this one.
    appendFileSync(this.#path, `\n${JSON.stringify(record)}`);
    const fd = openSync(this.#path, "r+");
    try {
      fsyncSync(fd);
    } finally {
      closeSync(fd);
    }
    this.#records.set(record.key, record);
  }
}
