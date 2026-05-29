declare module "/sdk/nxs.js" {
  export class NxsReader {
    constructor(buf: ArrayBuffer);
    bytes: ArrayBuffer;
    recordCount: number;
    getField(record: number, key: string): unknown;
    [key: string]: unknown;
  }
  export class NxsStreamReader {
    [key: string]: unknown;
  }
  export class NxsObject {
    [key: string]: unknown;
  }
  export const WIRE_SIGILS: Record<string, unknown>;
}

declare module "/sdk/nxs_writer.js" {
  export class NxsWriter {
    [key: string]: unknown;
  }
  export class NxsSchema {
    [key: string]: unknown;
  }
}

declare module "/sdk/nxs_compile.js" {
  export function compileNxsText(...args: unknown[]): Promise<unknown>;
  export function compileNxsColumnar(...args: unknown[]): Promise<unknown>;
  export function loadNxsDataset(...args: unknown[]): Promise<unknown>;
}

declare module "/sdk/wasm.js" {
  export function loadWasm(): Promise<unknown>;
  export class WasmSpanWriter {
    [key: string]: unknown;
  }
}

declare module "@bench/bench-run.js" {
  export function runBenchmarks(opts: Record<string, unknown>): Promise<number>;
  export function parseCsv(str: string): unknown[];
}
