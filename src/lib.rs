//! `idna._rust` — whole-address IDNA kernel (PyO3).
//!
//! Two crossings, `encode_address` / `decode_address`. Any rejection (or
//! internal anomaly) raises `KernelFailed`; the Python shim catches it and
//! re-runs the pure-Python path, which raises the byte-exact `IDNAError`.
//! Success values are bit-identical to pure Python by construction and by
//! differential test.

mod kernel;
mod punycode;
mod tables;
mod tables_gen;

use pyo3::create_exception;
use pyo3::prelude::*;

create_exception!(_rust, KernelFailed, pyo3::exceptions::PyException);

#[pyfunction]
fn encode_address(s: &str, strict: bool, uts46: bool, std3: bool) -> PyResult<Vec<u8>> {
    kernel::encode_address(s, strict, uts46, std3)
        .map_err(|_| KernelFailed::new_err("kernel rejected input"))
}

#[pyfunction]
fn decode_address(
    s: &str,
    strict: bool,
    uts46: bool,
    std3: bool,
    display: bool,
) -> PyResult<String> {
    kernel::decode_address(s, strict, uts46, std3, display)
        .map_err(|_| KernelFailed::new_err("kernel rejected input"))
}

#[pymodule]
fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(encode_address, m)?)?;
    m.add_function(wrap_pyfunction!(decode_address, m)?)?;
    m.add("KernelFailed", m.py().get_type::<KernelFailed>())?;
    Ok(())
}
