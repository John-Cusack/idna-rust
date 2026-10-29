# idna-rust

A faster `idna` (Internationalized Domain Names in Applications) for Python,
API-compatible with [kjd/idna](https://github.com/kjd/idna) 3.20.

- PyPI distribution name: `idna-rust`. The installed module keeps the
  original name `idna`, so swapping the install requires zero import changes.
- The UTS-46 map/normalize/validate/encode kernel is implemented in Rust
  (`idna._rust`, PyO3); every public function keeps its exact return values
  and exact `IDNAError` types/codes/messages, raised from Python.
- Without the compiled extension, the bundled pure-Python implementation
  (upstream 3.20 plus the tier-1 fast paths) is used automatically —
  behavior is identical either way.

## Performance

Frozen workload: `idna.encode(d, uts46=True)` + `idna.decode(d, uts46=True)`
per domain (see `data/rs-port-idna/WORKLOAD.md` in
[rustsmith](https://github.com/John-Cusack/rustsmith)).
Measured on Ryzen 9 5950X, CPython 3.12.3; min-of-7 × 3 sessions,
per-cell best; wall + peak RSS both arms. `stock` = upstream 3.20,
`tier-1` = our pure-Python fast paths (no extension), `idna-rust` =
this repo (Rust kernel + tier-1 fallback).

| Track | upstream 3.20 | tier-1 (pure) | idna-rust | vs upstream | vs tier-1 |
|---|---|---|---|---|---|
| mixed-8000 (representative) | 0.2228 s | 0.0856 s | 0.0325 s | 6.85x | 2.63x |
| ascii-16000 | 0.4264 s | 0.0567 s | 0.0423 s | 10.08x | 1.34x |
| idn-4000 | 0.1398 s | 0.1248 s | 0.0147 s | 9.51x | 8.49x |
| x1-valid / x1-idn (crossing floor) | 24 / 31 µs | 4 / 27 µs | 2 / 3 µs | 12x / 10.3x | 2x / 9x |
| x1-invalid (failure-path toll) | 13 µs | 6 µs | 8 µs | 1.6x | 0.75x |

Peak RSS: stock ~14.8–15.3 MB, tier-1 ~15.1–16.0 MB, idna-rust
~15.6–16.7 MB (extension pages, disclosed — same class as dateutil's
+1.2 MB). The x1-invalid row documents the failure-path toll honestly:
the kernel rejects fast, then the pure-Python path raises the exact
error; errors are rare in real traffic and the representative track
still wins 2.63x with 10% invalid mixed in.

## Layout

- `idna/` — the Python package (upstream 3.20 + tier-1 + Rust dispatch).
- `src/` — the Rust kernel (`idna._rust` extension via maturin/PyO3).
- `tools/gen_tables.py` — regenerates `src/tables_gen.rs` from the
  package's own data files plus the build interpreter's `unicodedata`
  (Unicode 15.0 on CPython 3.12.3 — matches the oracle exactly).
  Vendored output is committed; rerun on upstream bumps.
- `tests/` — upstream suite copy + kernel-vs-pure differential pytest.
- `bench/` — frozen-workload benches (`bench_tier1.py`, `bench_port.py`,
  `ceiling.py`; lane paths overridable via `IDNA_LANE_SCRATCH`).
- `tier1/` — the pure-Python tier-1 patch + upstream patch note +
  `gen_tier1_patch.py` (regenerates the patch mechanically from anchors).

## License

BSD-3-Clause (same as upstream). See `LICENSE.md` and `NOTICE`.
