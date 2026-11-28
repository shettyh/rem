use wasm_bindgen::prelude::*;

use crate::assets::asset_refs;
use crate::snapshot::SnapshotReducer;

/// Streaming WASM wrapper around `SnapshotReducer` for Cloudflare Workers compaction alarms.
#[wasm_bindgen]
pub struct ReducerHandle {
    inner: SnapshotReducer,
}

#[wasm_bindgen]
impl ReducerHandle {
    /// Initialize a reducer from an optional `CompactedSnapshot` v1 JSON string.
    /// Passing `null`, `undefined`, or an empty string starts from an empty snapshot.
    #[wasm_bindgen]
    pub fn from_snapshot_json(snapshot_json: Option<String>) -> Result<ReducerHandle, JsValue> {
        let inner = SnapshotReducer::from_snapshot_json(snapshot_json.as_deref())
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
        Ok(Self { inner })
    }

    /// Fold a JSON array chunk of `{ "seq": number, "payload": SyncOp }` records.
    /// Returns the number of ops processed in this chunk.
    #[wasm_bindgen]
    pub fn push_ops_chunk_json(&mut self, ops_chunk_json: &str) -> Result<u32, JsValue> {
        let count = self
            .inner
            .push_ops_chunk_json(ops_chunk_json)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
        Ok(count as u32)
    }

    /// Current highest sequence number folded into this reducer.
    #[wasm_bindgen]
    pub fn snapshot_seq(&self) -> u64 {
        self.inner.snapshot_seq()
    }

    /// Serialize the current `CompactedSnapshot` v1 state as JSON.
    #[wasm_bindgen]
    pub fn finish_json(&self) -> Result<String, JsValue> {
        let snapshot = self.inner.to_snapshot();
        serde_json::to_string(&snapshot).map_err(|err| JsValue::from_str(&err.to_string()))
    }

    /// Serialize the sorted unique set of `asset:<sha256>` hashes referenced by live cards as a JSON array.
    #[wasm_bindgen]
    pub fn referenced_assets_json(&self) -> Result<String, JsValue> {
        let refs: Vec<String> = self.inner.referenced_assets().into_iter().collect();
        serde_json::to_string(&refs).map_err(|err| JsValue::from_str(&err.to_string()))
    }
}

/// Extract all `asset:<sha256>` hashes referenced in an arbitrary markdown/JSON string as a sorted JSON array.
#[wasm_bindgen]
pub fn extract_asset_refs_json(text: &str) -> Result<String, JsValue> {
    let mut refs: Vec<String> = asset_refs(text).into_iter().collect();
    refs.sort();
    serde_json::to_string(&refs).map_err(|err| JsValue::from_str(&err.to_string()))
}
