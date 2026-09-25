import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  encodeDatetime,
  timeOfDayComponent,
  dayOfWeekComponent,
  type DatetimeEncoderConfig,
} from '../src/encoders/datetime.ts';
import { overlap } from '../src/sdr.ts';

function timeOfDayOnlyConfig(): DatetimeEncoderConfig {
  return { components: [timeOfDayComponent(200, 20)] };
}

test('encodeDatetime is deterministic (Requirement 2.3)', () => {
  const config = timeOfDayOnlyConfig();
  const date = new Date(2026, 0, 15, 10, 30, 0);
  const a = encodeDatetime(config, date);
  const b = encodeDatetime(config, date);
  assert.deepEqual(a.activeBits, b.activeBits);
});

test("encodeDatetime width is the sum of every component's width", () => {
  const config: DatetimeEncoderConfig = {
    components: [timeOfDayComponent(200, 20), dayOfWeekComponent(70, 10)],
  };
  const sdr = encodeDatetime(config, new Date());
  assert.equal(sdr.width, 270);
});

test('close timestamps (same day, minutes apart) overlap substantially (Requirement 4.2)', () => {
  const config = timeOfDayOnlyConfig();
  const a = encodeDatetime(config, new Date(2026, 0, 15, 10, 30, 0));
  const b = encodeDatetime(config, new Date(2026, 0, 15, 10, 31, 0));
  assert.ok(
    overlap(a, b) > 15,
    `one minute apart must overlap substantially, got ${overlap(a, b)}`,
  );
});

test("far-apart timestamps within the same day overlap little (Requirement 4.2's converse)", () => {
  const config = timeOfDayOnlyConfig();
  const morning = encodeDatetime(config, new Date(2026, 0, 15, 2, 0, 0));
  const evening = encodeDatetime(config, new Date(2026, 0, 15, 22, 0, 0));
  assert.ok(
    overlap(morning, evening) < 5,
    `20 hours apart must overlap little, got ${overlap(morning, evening)}`,
  );
});

test('the same time-of-day on different days overlaps substantially (Requirement 4.3 -- the property a raw scalar encoder cannot provide)', () => {
  const config = timeOfDayOnlyConfig();
  const day1 = encodeDatetime(config, new Date(2026, 0, 1, 14, 0, 0));
  const day2 = encodeDatetime(config, new Date(2026, 5, 20, 14, 0, 0)); // five months later, same clock time
  assert.equal(
    overlap(day1, day2),
    20,
    'identical time-of-day must produce identical time-of-day encoding regardless of the date',
  );
});

test("time-of-day wraps at the day boundary: 23:59 and 00:01 overlap (Requirement 4.3's wraparound mechanism)", () => {
  const config = timeOfDayOnlyConfig();
  const lateNight = encodeDatetime(config, new Date(2026, 0, 15, 23, 59, 0));
  const earlyMorning = encodeDatetime(config, new Date(2026, 0, 16, 0, 1, 0));
  assert.ok(
    overlap(lateNight, earlyMorning) > 5,
    `2 minutes apart across midnight must still overlap, got ${overlap(lateNight, earlyMorning)}`,
  );
});

test('dayOfWeek wraps at the week boundary: Saturday and Sunday overlap', () => {
  const config: DatetimeEncoderConfig = {
    components: [dayOfWeekComponent(70, 20)],
  };
  // 2026-01-17 is a Saturday, 2026-01-18 is a Sunday (day indices 6 and 0).
  const saturday = encodeDatetime(config, new Date(2026, 0, 17));
  const sunday = encodeDatetime(config, new Date(2026, 0, 18));
  assert.ok(
    overlap(saturday, sunday) > 0,
    'adjacent days across the week boundary (Sat/Sun) must overlap',
  );
});

test("multiple components combine independently: differing only in one component still shares the other's bits", () => {
  const config: DatetimeEncoderConfig = {
    components: [timeOfDayComponent(200, 20), dayOfWeekComponent(70, 10)],
  };
  const mondayMorning = encodeDatetime(config, new Date(2026, 0, 12, 9, 0, 0)); // a Monday
  const tuesdayMorning = encodeDatetime(config, new Date(2026, 0, 13, 9, 0, 0)); // a Tuesday, same time
  // Same time-of-day component (20 bits) must fully overlap even though
  // the day-of-week component differs.
  assert.ok(
    overlap(mondayMorning, tuesdayMorning) >= 20,
    `shared time-of-day component must overlap fully even on different days, got ${overlap(mondayMorning, tuesdayMorning)}`,
  );
});
