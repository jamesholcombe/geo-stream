//! Criterion benchmarks for `Engine::process_batch` / `process_event` hot path.
//!
//! Run from the repo root: `cargo bench -p engine`

use criterion::black_box;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use engine::GeoEngine;
use engine::{Circle, Engine, PointUpdate, Zone};
use geo::{LineString, Polygon};

fn unit_square_at(origin_x: f64, origin_y: f64) -> Polygon<f64> {
    Polygon::new(
        LineString::from(vec![
            (origin_x, origin_y),
            (origin_x + 1.0, origin_y),
            (origin_x + 1.0, origin_y + 1.0),
            (origin_x, origin_y + 1.0),
            (origin_x, origin_y),
        ]),
        vec![],
    )
}

fn register_n_disjoint_zones(engine: &mut Engine, n: usize) {
    for i in 0..n {
        let ox = (i as f64) * 2.0;
        engine
            .register_zone(Zone {
                id: format!("zone-{i}"),
                polygon: unit_square_at(ox, 0.0),
            })
            .unwrap();
    }
}

/// One entity at a fixed point inside `zone-0`.
fn process_batch_steady_one_entity(c: &mut Criterion) {
    let mut group = c.benchmark_group("process_batch_steady_one_entity");
    for n in [32, 128, 512, 2048] {
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, &n| {
            let mut engine = Engine::new();
            register_n_disjoint_zones(&mut engine, n);
            let batch = vec![PointUpdate {
                id: "entity-1".into(),
                x: 0.5,
                y: 0.5,
                t_ms: 0,
            }];
            engine.process_batch(batch.clone());
            b.iter(|| {
                let result = engine.process_batch(batch.clone());
                black_box(result)
            });
        });
    }
    group.finish();
}

/// Many entities in one batch; each update scans zones (inside zone-0 only).
fn process_batch_steady_many_entities(c: &mut Criterion) {
    let n_zones = 128;
    let mut group = c.benchmark_group("process_batch_steady_many_entities");
    for m in [16, 64, 256, 1024] {
        group.throughput(Throughput::Elements(m as u64));
        group.bench_with_input(
            BenchmarkId::new("batch_size", format!("n{n_zones}_m{m}")),
            &m,
            |b, &m| {
                let mut engine = Engine::new();
                register_n_disjoint_zones(&mut engine, n_zones);
                let batch: Vec<PointUpdate> = (0..m)
                    .map(|i| PointUpdate {
                        id: format!("e-{i}"),
                        x: 0.5,
                        y: 0.5,
                        t_ms: 0,
                    })
                    .collect();
                engine.process_batch(batch.clone());
                b.iter(|| {
                    let result = engine.process_batch(batch.clone());
                    black_box(result)
                });
            },
        );
    }
    group.finish();
}

/// Zone + catalog + circle registered; single steady update.
fn process_batch_mixed_zones_steady(c: &mut Criterion) {
    c.bench_function("process_batch_mixed_zones_one_entity", |b| {
        let mut engine = Engine::new();
        for i in 0..32 {
            engine
                .register_zone(Zone {
                    id: format!("fence-{i}"),
                    polygon: unit_square_at((i as f64) * 2.0, 0.0),
                })
                .unwrap();
        }
        engine
            .register_catalog_region(Zone {
                id: "cat-a".into(),
                polygon: unit_square_at(0.0, 0.0),
            })
            .unwrap();
        engine
            .register_catalog_region(Zone {
                id: "cat-b".into(),
                polygon: unit_square_at(0.0, 0.0),
            })
            .unwrap();
        engine
            .register_circle(Circle {
                id: "rad-1".into(),
                cx: 0.5,
                cy: 0.5,
                r: 10.0,
            })
            .unwrap();

        let batch = vec![PointUpdate {
            id: "entity-1".into(),
            x: 0.5,
            y: 0.5,
            t_ms: 0,
        }];
        engine.process_batch(batch.clone());

        b.iter(|| {
            let result = engine.process_batch(batch.clone());
            black_box(result)
        });
    });
}

criterion_group!(
    benches,
    process_batch_steady_one_entity,
    process_batch_steady_many_entities,
    process_batch_mixed_zones_steady
);
criterion_main!(benches);
