# CLAUDE.md

Working reference for this repository. See [AGENTS.md](AGENTS.md) for architectural invariants and [ROADMAP.md](ROADMAP.md) for current status.

---

## What this project is

A Rust-based, in-memory **geospatial stream processor**: location updates in → structured spatial events out (enter/exit, approach/recede, assignment changes). Developer-first; embeddable. Not a GIS library, database, or batch ETL.

---

## Workspace layout

```
crates/engine/               # Core processing — GeoEngine trait, Engine, SpatialRule pipeline
crates/state/                # EntityState, Event enum, membership transitions
crates/spatial/              # Geometry, SpatialIndex trait, NaiveSpatialIndex (R-tree)
crates/polygon-json/         # GeoJSON polygon parsing helper (30-line utility)
crates/adapters/stdin-stdout/ # NDJSON line I/O
crates/adapters/napi/        # Node.js NAPI bindings (feature-gated)
crates/cli/                  # geo-stream binary
protocol/                    # Wire contract: ndjson.md + schema/
examples/                    # Sample NDJSON and GeoJSON files
```

## Where code belongs

| What | Where |
|------|-------|
| New rule or orchestration | `crates/engine` |
| New geometry or index behaviour | `crates/spatial` (no domain rules) |
| New wire format or stdio handling | `crates/adapters/*` or `crates/cli`, update `protocol/` if contract changes |
| Shared state shape or transitions | `crates/state` |

---

## Key types and API

- **`GeoEngine` trait** (`crates/engine`): zone registration + `process_event(&mut self, PointUpdate) -> Vec<Event>`
- **`Engine` struct**: concrete impl; `process_batch`, `with_rules`, `register_zone_with_dwell` (plain `register_zone` uses default instant dwell)
- **`PointUpdate`**: `{ id, x, y, t_ms }` — `t_ms` is Unix epoch milliseconds
- **`Event` enum** (`crates/state`): `Enter`/`Exit`, `Approach`/`Recede`, `AssignmentChanged` — each carries `t_ms`
- **`SpatialRule` trait**: composable pipeline; default order: `ZoneRule → RadiusRule → CatalogRule`
- **`NaiveSpatialIndex`**: R-tree (rstar) on polygon bounding boxes + exact point-in-polygon; circles are a linear scan
- **`sort_events_deterministic`**: stable ordering by `(entity_id, t_ms, tier, zone_id, enter_before_exit)`

---

## Invariants (obey on every change)

1. **Engine owns logic** — no IO, no protocol types inside `crates/engine`
2. **Adapters are thin** — parse/serialize only; no spatial logic or business rules
3. **Event-first** — `process_event` is primary; `process_batch` is a thin wrapper
4. **Spatial is pluggable** — use `SpatialRule` trait, never `match rule_type { Zone => ... }`
5. **State is explicit** — `(old_state, event) → (new_state, outputs)`; no hidden cross-module mutation
6. **Errors** — `Result<T, E>` in core paths; no panics in engine/state/spatial

---

## Build and test commands

```bash
cargo build                          # debug build
cargo test                           # all workspace tests
cargo test -p cli                    # NDJSON integration tests
cargo bench -p engine                # Criterion benchmarks (output: target/criterion/)
cargo fmt --all                      # format
cargo clippy --workspace --all-targets -- -D warnings   # lint (CI enforces -D warnings)
make run                             # pipe examples/sample-input.ndjson through CLI
make docker-build                    # multi-stage Docker image
```

CI runs: `fmt`, `clippy -D warnings`, `cargo test`, JSON Schema validation of example files.

---

## Testing expectations

- Unit tests in `crates/engine` and `crates/state`
- Integration tests: `crates/cli/tests/fixtures/*.ndjson`
- Examples: `examples/sample-*.ndjson`
- Determinism: same inputs → same outputs always

---

## Current v1 work (ROADMAP.md)

Done: SpatialRule decoupled from NaiveSpatialIndex, R-tree for circles, polygon holes, timestamp monotonicity enforcement.

Remaining:
- [ ] Zone ID scoping (global vs per-type)
- [ ] Merge `polygon-json` into `crates/spatial`
- [ ] Tests: cross-type duplicate IDs, timestamp edge cases
- [ ] Stabilise NDJSON wire protocol to v1

---

## Anti-patterns

- Mixing engine logic with adapter code
- Hardcoding spatial behaviour via large `match` trees in core
- Designing around batch as the primary model
- Implicit or shared mutable state
- Leaking protocol types into the engine

---

## Guiding principle

> Build the engine abstraction first, not the ecosystem. Simplicity + extensibility over completeness.
