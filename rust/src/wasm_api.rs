//! WASM bindings for in-browser `.nxs` → `.nxb` compilation.
use wasm_bindgen::prelude::*;

use crate::compile_source;

#[wasm_bindgen]
pub fn compile_nxs(source: &str) -> Result<js_sys::Uint8Array, JsValue> {
    let bytes = compile_source(source).map_err(|e| JsValue::from_str(&format!("{e}")))?;
    Ok(js_sys::Uint8Array::from(bytes.as_slice()))
}
