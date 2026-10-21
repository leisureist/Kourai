import { test } from "node:test";
import assert from "node:assert/strict";
import { NdjsonSplitter, NdjsonParseError } from "../dist/index.js";

const enc = new TextEncoder();

test("parses complete lines and keeps partial line buffered", () => {
  const s = new NdjsonSplitter();
  assert.deepEqual(s.push('{"a":1}\n{"b"'), [{ a: 1 }]);
  assert.deepEqual(s.push(':2}\n'), [{ b: 2 }]);
  assert.deepEqual(s.end(), []);
});

test("end() parses a final line without newline", () => {
  const s = new NdjsonSplitter();
  assert.deepEqual(s.push("1\n2"), [1]);
  assert.deepEqual(s.end(), [2]);
});

test("accepts CRLF and skips blank lines", () => {
  const s = new NdjsonSplitter();
  assert.deepEqual(s.push('"x"\r\n\r\n  \n"y"\n'), ["x", "y"]);
});

test("multi-byte UTF-8 split across byte chunks", () => {
  const bytes = enc.encode('"台新"\n');
  const s = new NdjsonSplitter();
  const out = [];
  for (const b of bytes) out.push(...s.push(Uint8Array.of(b)));
  out.push(...s.end());
  assert.deepEqual(out, ["台新"]);
});

test("malformed line throws with its line number and poisons the splitter", () => {
  const s = new NdjsonSplitter();
  assert.throws(() => s.push("1\n\n{bad\n"), (e) => e instanceof NdjsonParseError && e.line === 3);
  assert.throws(() => s.push("2\n"), (e) => e instanceof NdjsonParseError && e.line === 3);
});

test("malformed final line without newline throws on end()", () => {
  const s = new NdjsonSplitter();
  assert.deepEqual(s.push("1\n{bad"), [1]);
  assert.throws(() => s.end(), (e) => e instanceof NdjsonParseError && e.line === 2);
});

test("invalid UTF-8 throws", () => {
  const s = new NdjsonSplitter();
  assert.throws(() => s.push(Uint8Array.of(0x22, 0xff, 0x22, 0x0a)), NdjsonParseError);
});

test("line longer than maxLineBytes throws even before newline arrives", () => {
  const s = new NdjsonSplitter({ maxLineBytes: 8 });
  assert.deepEqual(s.push('"1234"\n'), ["1234"]);
  assert.throws(() => s.push('"123456789'), (e) => e instanceof NdjsonParseError && e.line === 2);
});

test("maxLineBytes counts UTF-8 bytes, not characters", () => {
  const s = new NdjsonSplitter({ maxLineBytes: 6 });
  assert.throws(() => s.push('"台新"\n'), NdjsonParseError); // 8 bytes
});

test("rejects invalid maxLineBytes", () => {
  assert.throws(() => new NdjsonSplitter({ maxLineBytes: 0 }), RangeError);
});

test("use after end() throws", () => {
  const s = new NdjsonSplitter();
  s.end();
  assert.throws(() => s.push("1\n"), /already ended/);
});
