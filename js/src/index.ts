/** Raised for a line that is not valid JSON or exceeds `maxLineBytes`. */
export class NdjsonParseError extends Error {
  /** 1-based line number in the stream. */
  readonly line: number;

  constructor(line: number, message: string, options?: { cause?: unknown }) {
    super(`line ${line}: ${message}`, options);
    this.name = "NdjsonParseError";
    this.line = line;
  }
}

export interface NdjsonSplitterOptions {
  /** Maximum UTF-8 byte length of one line (excluding the newline). Default 1 MiB. */
  maxLineBytes?: number;
}

declare const TextDecoder: {
  new (label?: string, options?: { fatal?: boolean }): {
    decode(input?: Uint8Array, options?: { stream?: boolean }): string;
  };
};
declare const TextEncoder: { new (): { encode(input: string): Uint8Array } };

const DEFAULT_MAX_LINE_BYTES = 1024 * 1024;
const encoder = new TextEncoder();

/**
 * Splits a chunked NDJSON stream into parsed values.
 *
 * Feed chunks with `push()`; call `end()` once after the last chunk. After an
 * error the splitter is unusable: further calls throw the same error.
 */
export class NdjsonSplitter {
  private readonly maxLineBytes: number;
  private readonly decoder = new TextDecoder("utf-8", { fatal: true });
  private buffer = "";
  private line = 0;
  private failed: NdjsonParseError | null = null;
  private ended = false;

  constructor(options: NdjsonSplitterOptions = {}) {
    const max = options.maxLineBytes ?? DEFAULT_MAX_LINE_BYTES;
    if (!Number.isInteger(max) || max <= 0) {
      throw new RangeError(`maxLineBytes must be a positive integer, got ${max}`);
    }
    this.maxLineBytes = max;
  }

  /** Adds a chunk and returns the values of every line it completed. */
  push(chunk: string | Uint8Array): unknown[] {
    this.assertUsable();
    const text = typeof chunk === "string" ? chunk : this.decodeChunk(chunk, true);
    this.buffer += text;
    return this.drain(false);
  }

  /** Flushes the last line (if it had no trailing newline) and closes the splitter. */
  end(): unknown[] {
    this.assertUsable();
    this.buffer += this.decodeChunk(undefined, false);
    this.ended = true;
    return this.drain(true);
  }

  private decodeChunk(chunk: Uint8Array | undefined, stream: boolean): string {
    try {
      return this.decoder.decode(chunk, { stream });
    } catch (cause) {
      throw this.fail(new NdjsonParseError(this.line + 1, "invalid UTF-8", { cause }));
    }
  }

  private drain(final: boolean): unknown[] {
    const values: unknown[] = [];
    let start = 0;
    for (;;) {
      const nl = this.buffer.indexOf("\n", start);
      if (nl === -1) break;
      this.emit(this.buffer.slice(start, nl), values);
      start = nl + 1;
    }
    this.buffer = this.buffer.slice(start);
    if (final) {
      if (this.buffer.length > 0) this.emit(this.buffer, values);
      this.buffer = "";
    } else {
      this.checkLength(this.buffer, this.line + 1);
    }
    return values;
  }

  private emit(raw: string, values: unknown[]): void {
    this.line += 1;
    const text = raw.endsWith("\r") ? raw.slice(0, -1) : raw;
    this.checkLength(text, this.line);
    if (text.trim() === "") return;
    try {
      values.push(JSON.parse(text));
    } catch (cause) {
      throw this.fail(new NdjsonParseError(this.line, "invalid JSON", { cause }));
    }
  }

  private checkLength(text: string, line: number): void {
    // UTF-16 length is a lower bound on UTF-8 bytes; encode only when it might exceed.
    if (text.length * 3 <= this.maxLineBytes) return;
    if (encoder.encode(text).length > this.maxLineBytes) {
      throw this.fail(new NdjsonParseError(line, `line exceeds ${this.maxLineBytes} bytes`));
    }
  }

  private fail(error: NdjsonParseError): NdjsonParseError {
    this.failed = error;
    return error;
  }

  private assertUsable(): void {
    if (this.failed) throw this.failed;
    if (this.ended) throw new Error("NdjsonSplitter: already ended");
  }
}
