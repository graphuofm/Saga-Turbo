# Installing Saga Turbo

Saga Turbo is self-contained: a native Rust core with no runtime services. It runs
identically on a 16 GB laptop and an 80 GB H100 node — only the *feasible
scale* differs, and `saga doctor` tells you what that is (§13).

## Platforms

| OS | CLI binary | Python wheel | notes |
|---|:--:|:--:|---|
| Linux x86-64 | ✅ | ✅ | primary target; tested |
| macOS (Intel / Apple Silicon) | ✅ | ✅ | GPU probe returns none (no CUDA); CPU path |
| Windows x86-64 | ✅ | ✅ | NUMA reported as 1 node; everything else probed |

The host probe degrades gracefully off Linux: NUMA topology (read from sysfs)
falls back to a single node, and the GPU probe (which shells out to
`nvidia-smi`) reports no GPU when the driver isn't present. CPU, RAM and disk
come from the cross-platform `sysinfo` crate. Nothing platform-specific is on
the generation hot path.

## Option 1 — Python (`pip install`)

The Python harness wraps the same native engine via PyO3 (abi3 wheels, one
wheel per platform works across Python 3.9+):

```bash
pip install saga          # from a built wheel
# from source checkout:
pip install maturin
maturin build --release -m crates/saga-py/Cargo.toml
pip install target/wheels/saga-*.whl
```
```python
import saga
saga.doctor()                      # host capabilities
saga.generate("finance", nodes=1_000_000, edges=100_000_000,
              time_span_days=180, anomaly_rate=0.02, out="out")
# or get arrays straight into PyG/DGL, no disk:
a = saga.generate_arrays("finance", nodes=100_000, edges=10_000_000,
                         time_span_days=90)
# staleness knobs, same semantics as --block-len-days / --guards /
# --calibrate-under-guards; guarded sharded runs stay CLI-only:
s = saga.generate("finance", nodes=20_000, edges=200_000, time_span_days=20,
                  guards=True, block_len_days=0.01, out="out")
s["overdraft_violations"]
```

## Option 2 — prebuilt CLI binary

A single static binary, no dependencies:

```bash
# download the release binary for your platform, then:
chmod +x saga && ./saga doctor
```

## Option 3 — build from source (Rust)

```bash
git clone https://github.com/graphuofm/SAGA2.0 && cd SAGA2.0
cargo build --release            # binary at target/release/saga
./target/release/saga doctor
# or install onto PATH:
cargo install --path crates/saga-cli
```

Requires a recent stable Rust (1.80+). No CMake, no system libraries — `cargo`
fetches everything (arrow/parquet, rayon, etc.).

## First run

```bash
saga doctor                                  # what scale fits this machine
saga generate --domain finance --preset small --out out
saga stats out                               # introspection summary
```

`saga generate --dry-run` estimates time, RAM and disk *before* running so you
commit with eyes open; if a request can't fit the host, Saga Turbo refuses with an
actionable message and a suggested feasible scale rather than OOM-ing.
