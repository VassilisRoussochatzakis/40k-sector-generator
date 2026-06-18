# seg-golden — Golden Segmentum

Stitch seed: `stitch-golden`

Generator: sectorforge v0.1.0

- **Super-grid:** 2×1
- **Children:** 2
- **Inter-sector links:** 2
- **Faction roster mode:** shared
- **Aggregate systems / worlds / routes:** 48 / 187 / 113

## Super-grid

```
[alpha       ][beta        ]
```

## Children

| ID | Slot | Sector | Seed | Systems | Worlds | Routes |
|---|---|---|---|---:|---:|---:|
| alpha | (0, 0) | m42-sector | `alpha-seed` | 24 | 94 | 53 |
| beta | (1, 0) | m42-sector | `beta-seed` | 24 | 93 | 60 |

## Inter-sector links

| ID | From | To | Orientation | Units | Type | Stability |
|---|---|---|---|---:|---|---|
| sl-0001 | alpha/sys-0023 | beta/sys-0005 | E-W | 1 | charted_passage | unstable |
| sl-0002 | alpha/sys-0024 | beta/sys-0004 | E-W | 1 | charted_passage | unstable |

## Super-manifest

- Stitch seed hash: `d9f0725e41490c20aac37d45aefb745a0b38d4bd0459a892d315efa003abf0c0`
- Settings digest: `blake3:28c740d7c4ca61be06b9425df77110b14e2d6b41420b132b664a9ba999a70ed2`
- Child digests:
  - `alpha` → `blake3:a9c8231f3992711dc3f4ef987dae74f6db8d242d89eae8fe05831c8e3ba70703`
  - `beta` → `blake3:1c2f00b9ab496130ae59e3b505463b715d935d4f2b6588b5b8959a0eee2427eb`

