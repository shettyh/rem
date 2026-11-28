import wasmModule from './wasm/rem_sync_reduce_bg.wasm'
import {
  extract_asset_refs_json,
  initSync,
  ReducerHandle,
} from './wasm/rem_sync_reduce.js'

let wasmInitialized = false

function ensureWasmInitialized(): void {
  if (wasmInitialized) return
  initSync({ module: wasmModule })
  wasmInitialized = true
}

export function createSnapshotReducer(prevSnapshotJson: string | null): ReducerHandle {
  ensureWasmInitialized()
  return ReducerHandle.from_snapshot_json(prevSnapshotJson)
}

export function extractAssetRefsFromText(text: string): string[] {
  ensureWasmInitialized()
  return JSON.parse(extract_asset_refs_json(text)) as string[]
}

export type { ReducerHandle }
