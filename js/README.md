# Kourai

Kourai is named after the *Kourai Khryseai* — the golden handmaidens Hephaestus
forged, who could think, speak and work beside their maker.

This repository hosts small, self-contained building blocks published under
the `kourai` name. The first one is a strict **NDJSON line splitter**:
feed it chunks of a byte/text stream, get back one parsed JSON value per line.

- Lines may arrive split across any chunk boundary (including in the middle of
  a multi-byte UTF-8 character).
- `\n` and `\r\n` line endings are both accepted; blank lines are skipped.
- A malformed line is an **error carrying its 1-based line number** — never
  silently dropped.
- A line longer than `maxLineBytes` (default 1 MiB) is an error, so a stream
  without newlines cannot grow memory without bound.
- Trailing data without a final newline is parsed by `end()` / `finish()`.

| Package | Language | Install |
|---|---|---|
| [`kourai` on npm](https://www.npmjs.com/package/kourai) | TypeScript / JavaScript | `npm install kourai` |
| [`kourai` on crates.io](https://crates.io/crates/kourai) | Rust | `cargo add kourai` |

## JavaScript / TypeScript

```ts
import { NdjsonSplitter } from "kourai";

const splitter = new NdjsonSplitter();
for await (const chunk of stream) {
  for (const value of splitter.push(chunk)) handle(value);
}
for (const value of splitter.end()) handle(value);
```

## Rust

```rust
use kourai::Splitter;

let mut splitter = Splitter::new();
for chunk in chunks {
    for value in splitter.push(chunk)? {
        handle(value);
    }
}
for value in splitter.finish()? {
    handle(value);
}
```

## License

MIT
