# seg-pacificus-demo — Segmentum Pacificus (demo)

Stitch seed: `stitch-001`

Generator: sectorforge v0.1.0

- **Super-grid:** 2×2
- **Children:** 4
- **Inter-sector links:** 8
- **Faction roster mode:** shared
- **Aggregate systems / worlds / routes:** 96 / 366 / 273

## Super-grid

```
[alpha       ][beta        ]
[gamma       ][delta       ]
```

## Children

| ID | Slot | Sector | Seed | Systems | Worlds | Routes |
|---|---|---|---|---:|---:|---:|
| alpha | (0, 0) | m42-sector | `alpha-seed` | 24 | 94 | 70 |
| beta | (1, 0) | m42-sector | `beta-seed` | 24 | 93 | 74 |
| gamma | (0, 1) | m42-sector | `gamma-seed` | 24 | 92 | 66 |
| delta | (1, 1) | m42-sector | `delta-seed` | 24 | 87 | 63 |

## Inter-sector links

| ID | From | To | Orientation | Units | Type | Stability |
|---|---|---|---|---:|---|---|
| sl-0001 | alpha/sys-0024 | beta/sys-0002 | E-W | 1 | ChartedPassage | Unstable |
| sl-0002 | alpha/sys-0018 | beta/sys-0001 | E-W | 2 | ChartedPassage | Unstable |
| sl-0003 | alpha/sys-0003 | gamma/sys-0017 | N-S | 1 | ChartedPassage | Unstable |
| sl-0004 | alpha/sys-0007 | gamma/sys-0019 | N-S | 1 | ChartedPassage | Unstable |
| sl-0005 | gamma/sys-0019 | delta/sys-0004 | E-W | 2 | ChartedPassage | Unstable |
| sl-0006 | gamma/sys-0021 | delta/sys-0001 | E-W | 2 | ChartedPassage | Unstable |
| sl-0007 | beta/sys-0003 | delta/sys-0001 | N-S | 1 | ChartedPassage | Unstable |
| sl-0008 | beta/sys-0013 | delta/sys-0005 | N-S | 1 | ChartedPassage | Unstable |

## Super-manifest

- Stitch seed hash: `2a057afbc30a6d6e80cc85c599caf7073db0dd3183f2376abe6d7103cad9f45a`
- Settings digest: `blake3:8f240cecaee8fe62ccc095051839a52e239220d8c806b529639be484ca9ae49b`
- Child digests:
  - `alpha` → `blake3:319b6007b3f4b91a5a91018dddd781dacd2bb9542b72ce6b05fe11d464e999b9`
  - `beta` → `blake3:9f9aeec80da7d4495704be9145fd08db3c2aead57c7554d3cda5913a002434b4`
  - `delta` → `blake3:e2b2728cdfdb1529709040356ba9fe0bf28270340d544cae0adb960d239a5b0b`
  - `gamma` → `blake3:8eb7068c3dd423ef911217f13b6f0befa73eaf22bdd9e0bb4845b3c9c4fa14a8`

