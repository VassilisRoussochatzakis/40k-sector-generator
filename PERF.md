# PERF.md — Serialization format benchmark

## Prompt for Claude

> Run the sector serialization-format benchmark against my sample sector, then
> revert all temporary changes and report the results table.
>
> 1. Generate (or locate) the sample sector JSON. Default: generate
>    `examples/<SAMPLE>` into `/tmp/perfout` with
>    `cargo run --release --bin sectorforge -- generate --project examples/<SAMPLE> --out /tmp/perfout`,
>    producing `/tmp/perfout/sector.json`. If a sector JSON path is given, use it directly.
> 2. Temporarily add these dev-dependencies to `Cargo.toml`:
>    ```toml
>    postcard = { version = "1", features = ["use-std"] }
>    bincode = { version = "1" }
>    ciborium = "0.2"
>    rmp-serde = "1"
>    ```
> 3. Create `examples/postcard_bench.rs` (see "Bench harness" below).
> 4. Run `cargo run --release --example postcard_bench -- /tmp/perfout/sector.json`.
> 5. Report the table: bytes(KB), ser(ms), de(ms), round-trips? for json / postcard /
>    bincode / cbor / msgpack(array) / msgpack-map. Note which formats FAIL decode and why.
> 6. **Clean up**: `rm examples/postcard_bench.rs` and `git checkout Cargo.toml Cargo.lock`.
>    Confirm `git status` shows no leftover bench files.

## Context (last run, examples/big_test, 200 systems)

- json 5651 KB / ser 4.0 ms / de 10.4 ms — baseline, round-trips.
- postcard 2.43x smaller, bincode 1.66x, msgpack-array 2.12x — **all FAIL decode**.
- cbor 1.20x, msgpack-map 1.19x — round-trip OK (self-describing).
- Root cause of binary decode failures: internally-tagged enums
  `#[serde(tag = "...")]` in `src/analysis/{personae,hooks,search}.rs` and
  `src/analysis/history/model.rs`. Non-self-describing formats (postcard, bincode,
  msgpack-array) can't decode them.
- Verdict: switching not worth it at this scale. Re-run on a bigger sample to confirm.

## Bench harness (`examples/postcard_bench.rs`)

```rust
//! Throwaway format bench: JSON vs postcard/bincode/cbor/msgpack on a generated sector.
//! Run: cargo run --release --example postcard_bench -- /tmp/perfout/sector.json
use std::time::Instant;

use camino::Utf8PathBuf;

fn time<T>(iters: u32, mut f: impl FnMut() -> T) -> (T, f64) {
    let mut last = f(); // warmup
    let start = Instant::now();
    for _ in 0..iters {
        last = f();
    }
    let per = start.elapsed().as_secs_f64() * 1000.0 / iters as f64;
    (last, per)
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .map(Utf8PathBuf::from)
        .unwrap_or_else(|| Utf8PathBuf::from("/tmp/perfout/sector.json"));

    let sector = sectorforge::load_sector_json(&path).expect("load sector json");
    let iters = 50u32;

    let (json, json_ser) = time(iters, || serde_json::to_vec(&sector).unwrap());
    let (pc, pc_ser) = time(iters, || postcard::to_allocvec(&sector).unwrap());
    let (bc, bc_ser) = time(iters, || bincode::serialize(&sector).unwrap());
    let (cbor, cbor_ser) = time(iters, || {
        let mut v = Vec::new();
        ciborium::into_writer(&sector, &mut v).unwrap();
        v
    });
    let (mp, mp_ser) = time(iters, || rmp_serde::to_vec(&sector).unwrap());
    let (mpm, mpm_ser) = time(iters, || {
        use serde::Serialize;
        let mut buf = Vec::new();
        let mut s = rmp_serde::Serializer::new(&mut buf).with_struct_map();
        sector.serialize(&mut s).unwrap();
        buf
    });

    type S = sectorforge::GeneratedSector;
    let json_de = serde_json::from_slice::<S>(&json)
        .map(|_| time(iters, || serde_json::from_slice::<S>(&json).unwrap()).1)
        .map_err(|e| e.to_string());
    let pc_de = postcard::from_bytes::<S>(&pc)
        .map(|_| time(iters, || postcard::from_bytes::<S>(&pc).unwrap()).1)
        .map_err(|e| e.to_string());
    let bc_de = bincode::deserialize::<S>(&bc)
        .map(|_| time(iters, || bincode::deserialize::<S>(&bc).unwrap()).1)
        .map_err(|e| e.to_string());
    let cbor_de = ciborium::from_reader::<S, _>(cbor.as_slice())
        .map(|_| time(iters, || ciborium::from_reader::<S, _>(cbor.as_slice()).unwrap()).1)
        .map_err(|e| e.to_string());
    let mp_de = rmp_serde::from_slice::<S>(&mp)
        .map(|_| time(iters, || rmp_serde::from_slice::<S>(&mp).unwrap()).1)
        .map_err(|e| e.to_string());
    let mpm_de = rmp_serde::from_slice::<S>(&mpm)
        .map(|_| time(iters, || rmp_serde::from_slice::<S>(&mpm).unwrap()).1)
        .map_err(|e| e.to_string());

    let kb = |n: usize| n as f64 / 1024.0;
    let de = |r: &Result<f64, String>| match r {
        Ok(ms) => format!("{ms:.3}"),
        Err(e) => format!("FAIL: {e}"),
    };
    println!("systems: {}", sector.systems.len());
    println!("{:<10} {:>12} {:>10} {:>30}", "format", "bytes(KB)", "ser(ms)", "de(ms)");
    println!("{:<10} {:>12.1} {:>10.3} {:>30}", "json", kb(json.len()), json_ser, de(&json_de));
    println!("{:<10} {:>12.1} {:>10.3} {:>30}", "postcard", kb(pc.len()), pc_ser, de(&pc_de));
    println!("{:<10} {:>12.1} {:>10.3} {:>30}", "bincode", kb(bc.len()), bc_ser, de(&bc_de));
    println!("{:<10} {:>12.1} {:>10.3} {:>30}", "cbor", kb(cbor.len()), cbor_ser, de(&cbor_de));
    println!("{:<10} {:>12.1} {:>10.3} {:>30}", "msgpack", kb(mp.len()), mp_ser, de(&mp_de));
    println!("{:<10} {:>12.1} {:>10.3} {:>30}", "msgpack-m", kb(mpm.len()), mpm_ser, de(&mpm_de));
    println!(
        "size vs json: postcard {:.2}x, bincode {:.2}x, cbor {:.2}x, msgpack {:.2}x smaller",
        json.len() as f64 / pc.len() as f64,
        json.len() as f64 / bc.len() as f64,
        json.len() as f64 / cbor.len() as f64,
        json.len() as f64 / mp.len() as f64,
    );
}
```
