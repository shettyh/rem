/* tslint:disable */
/* eslint-disable */

/**
 * Streaming WASM wrapper around `SnapshotReducer` for Cloudflare Workers compaction alarms.
 */
export class ReducerHandle {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Serialize the current `CompactedSnapshot` v1 state as JSON.
     */
    finish_json(): string;
    /**
     * Initialize a reducer from an optional `CompactedSnapshot` v1 JSON string.
     * Passing `null`, `undefined`, or an empty string starts from an empty snapshot.
     */
    static from_snapshot_json(snapshot_json?: string | null): ReducerHandle;
    /**
     * Fold a JSON array chunk of `{ "seq": number, "payload": SyncOp }` records.
     * Returns the number of ops processed in this chunk.
     */
    push_ops_chunk_json(ops_chunk_json: string): number;
    /**
     * Serialize the sorted unique set of `asset:<sha256>` hashes referenced by live cards as a JSON array.
     */
    referenced_assets_json(): string;
    /**
     * Current highest sequence number folded into this reducer.
     */
    snapshot_seq(): bigint;
}

/**
 * Extract all `asset:<sha256>` hashes referenced in an arbitrary markdown/JSON string as a sorted JSON array.
 */
export function extract_asset_refs_json(text: string): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_reducerhandle_free: (a: number, b: number) => void;
    readonly extract_asset_refs_json: (a: number, b: number) => [number, number, number, number];
    readonly reducerhandle_finish_json: (a: number) => [number, number, number, number];
    readonly reducerhandle_from_snapshot_json: (a: number, b: number) => [number, number, number];
    readonly reducerhandle_push_ops_chunk_json: (a: number, b: number, c: number) => [number, number, number];
    readonly reducerhandle_referenced_assets_json: (a: number) => [number, number, number, number];
    readonly reducerhandle_snapshot_seq: (a: number) => bigint;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
