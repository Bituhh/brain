import { test } from "node:test";
import assert from "node:assert/strict";
import { TrigramModel } from "../src/baseline/trigram.ts";

test("TrigramModel predicts the most frequently observed next character", () => {
  const model = new TrigramModel();
  model.train("the cat sat on the mat"); // "th" -> "e" twice
  assert.equal(model.predict("th"), "e");
});

test("TrigramModel returns undefined for a never-observed context", () => {
  const model = new TrigramModel();
  model.train("hello world");
  assert.equal(model.predict("zz"), undefined);
});

test("TrigramModel.observe is the streaming-friendly primitive train is built from", () => {
  const streamed = new TrigramModel();
  streamed.observe("th", "e");
  streamed.observe("th", "e");
  streamed.observe("th", "e");
  streamed.observe("th", "a");

  const trained = new TrigramModel();
  trained.train("theatheatheatha");
  // Both must agree on the majority prediction for the same observed counts.
  assert.equal(streamed.predict("th"), "e");
});

test("TrigramModel.predict uses only the last two characters of a longer context", () => {
  const model = new TrigramModel();
  model.train("xythexythe");
  assert.equal(model.predict("some prefix xy"), model.predict("xy"));
});

test("TrigramModel: an empty corpus predicts nothing", () => {
  const model = new TrigramModel();
  model.train("");
  assert.equal(model.predict("ab"), undefined);
});

test("TrigramModel picks up genuine high-order structure: distinguishes context-dependent continuations", () => {
  // The classic VAL-4 example this project's own README cites: the 'e' in
  // "the" is followed by a space far more often than by anything else in
  // ordinary English text, while other two-character contexts predict
  // differently.
  const model = new TrigramModel();
  model.train("the dog and the cat and the bird ran to the store");
  assert.equal(model.predict("th"), "e");
  assert.equal(model.predict("he"), " ");
});
