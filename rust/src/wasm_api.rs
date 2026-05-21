//! WASM bindings for in-browser `.nxs` → `.nxb` compilation.
use wasm_bindgen::prelude::*;

use crate::compile_source;
use crate::compile_source_with_opts;
use crate::layout::{CompileOptions, Layout};

#[wasm_bindgen]
pub fn compile_nxs(source: &str) -> Result<js_sys::Uint8Array, JsValue> {
    let bytes = compile_source(source).map_err(err_js)?;
    Ok(js_sys::Uint8Array::from(bytes.as_slice()))
}

/// Compile record blocks as columnar layout (no `@layout` pragma required in source).
#[wasm_bindgen]
pub fn compile_nxs_columnar(source: &str) -> Result<js_sys::Uint8Array, JsValue> {
    let mut opts = CompileOptions::default();
    opts.layout = Layout::Columnar;
    let bytes = compile_source_with_opts(source, &opts).map_err(err_js)?;
    Ok(js_sys::Uint8Array::from(bytes.as_slice()))
}

fn err_js(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&format!("{e}"))
}
