// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg(not(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64"))))]
mod common;

use std::collections::HashMap;

use common::{Source, random_dwg};
use forja_dwg::object::Entity;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs()))
}

fn close3(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| close(*x, y))
}

#[test]
fn small_sample_is_read_exactly() {
    let drawing = forja_dwg::read(&common::sample_dwg()).unwrap();
    assert!(drawing.damaged.is_empty(), "{:?}", drawing.damaged);
    assert_eq!(drawing.entities.len(), 4);
    let Entity::Line { start, end, .. } = &drawing.entities[0] else { panic!("first entity is a line") };
    assert_eq!((*start, *end), ([1.5, 2.5, 0.0], [10.25, -4.0, 0.0]));
    let Entity::Circle { center, radius, .. } = &drawing.entities[1] else { panic!("second entity is a circle") };
    assert_eq!((*center, *radius), ([5.0, 6.0, 0.0], 3.5));
    let Entity::Arc { radius, start_angle, end_angle, .. } = &drawing.entities[2] else { panic!("third entity is an arc") };
    assert!(close(*radius, 2.0) && close(*start_angle, 10f64.to_radians()) && close(*end_angle, 100f64.to_radians()));
    let Entity::Point { position, .. } = &drawing.entities[3] else { panic!("fourth entity is a point") };
    assert_eq!(*position, [7.0, 8.0, 9.0]);
    // The table and dictionary objects are counted, not decoded.
    assert!(drawing.skipped.values().sum::<usize>() > 20);
    assert!(!drawing.classes.is_empty());
}

#[test]
fn random_drawings_match_what_was_written() {
    for seed in [1u64, 2, 3, 0xDEAD_BEEF] {
        let (dwg, sources) = random_dwg(seed, 400);
        let drawing = forja_dwg::read(&dwg).unwrap();
        assert!(drawing.damaged.is_empty(), "seed {seed}: {:?}", drawing.damaged);
        assert_eq!(drawing.entities.len(), sources.len(), "seed {seed}");
        // One layer handle per layer name, distinct between names.
        let mut layers: HashMap<&str, u64> = HashMap::new();
        for (e, s) in drawing.entities.iter().zip(&sources) {
            let (common, layer) = match (e, s) {
                (Entity::Line { common, start, end, thickness, extrusion }, Source::Line { layer, a, b, thickness: t, ext }) => {
                    assert!(close3(*start, *a) && close3(*end, *b), "seed {seed}: line {start:?} {end:?} vs {a:?} {b:?}");
                    assert!(close(*thickness, *t) && close3(*extrusion, *ext), "seed {seed}");
                    (common, layer)
                }
                (Entity::Circle { common, center, radius, thickness, extrusion }, Source::Circle { layer, c, r, thickness: t, ext }) => {
                    assert!(close3(*center, *c) && close(*radius, *r) && close(*thickness, *t) && close3(*extrusion, *ext), "seed {seed}");
                    (common, layer)
                }
                (
                    Entity::Arc { common, center, radius, thickness, extrusion, start_angle, end_angle },
                    Source::Arc { layer, c, r, thickness: t, ext, a0, a1 },
                ) => {
                    assert!(close3(*center, *c) && close(*radius, *r) && close(*thickness, *t) && close3(*extrusion, *ext), "seed {seed}");
                    assert!(close(*start_angle, a0.to_radians()) && close(*end_angle, a1.to_radians()), "seed {seed}");
                    (common, layer)
                }
                (Entity::Point { common, position, thickness, extrusion, .. }, Source::Point { layer, p, thickness: t, ext, .. }) => {
                    assert!(close3(*position, *p) && close(*thickness, *t) && close3(*extrusion, *ext), "seed {seed}");
                    (common, layer)
                }
                (e, s) => panic!("seed {seed}: kind mismatch {e:?} vs {s:?}"),
            };
            let handle = common.layer.expect("layer handle");
            assert_eq!(*layers.entry(layer).or_insert(handle), handle, "seed {seed}: layer {layer} has two handles");
        }
        let mut seen: Vec<u64> = layers.values().copied().collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), layers.len(), "seed {seed}: two layer names share a handle");
    }
}
