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

#[test]
fn layers_are_read_with_their_state_and_entities_resolve_to_them() {
    for seed in [1u64, 7] {
        let (dwg, sources) = random_dwg(seed, 200);
        let drawing = forja_dwg::read(&dwg).unwrap();
        assert!(drawing.damaged.is_empty(), "{:?}", drawing.damaged);
        for (i, name) in common::LAYERS.iter().enumerate() {
            let layer = drawing.layers.values().find(|l| l.name == *name).unwrap_or_else(|| panic!("layer {name} missing"));
            assert_eq!(layer.color as usize, i + 1, "colour of {name}");
            assert!(!layer.frozen && !layer.off && !layer.locked, "{name}");
        }
        for (e, s) in drawing.entities.iter().zip(&sources) {
            let (Source::Line { layer, .. } | Source::Circle { layer, .. } | Source::Arc { layer, .. } | Source::Point { layer, .. }) = s;
            assert_eq!(drawing.layer_of(e).map(|l| l.name.as_str()), Some(*layer), "seed {seed}");
        }
    }
}

#[test]
fn layer_flags_are_read() {
    let mut s = String::new();
    let mut g = |c: i32, v: &str| s.push_str(&format!("{c:>3}\n{v}\n"));
    g(0, "SECTION");
    g(2, "HEADER");
    g(9, "$ACADVER");
    g(1, "AC1015");
    g(0, "ENDSEC");
    g(0, "SECTION");
    g(2, "TABLES");
    g(0, "TABLE");
    g(2, "LAYER");
    g(70, "5");
    for (name, flags, color) in [("0", "0", "7"), ("Frozen", "1", "1"), ("Locked", "4", "2"), ("Hidden", "0", "-3"), ("Both", "5", "-250")] {
        g(0, "LAYER");
        g(2, name);
        g(70, flags);
        g(62, color);
        g(6, "CONTINUOUS");
    }
    g(0, "ENDTAB");
    g(0, "ENDSEC");
    g(0, "EOF");
    let dwg = nodalia_dwg::dxf_to_dwg_as(s.as_bytes(), "AC1015").unwrap();
    let drawing = forja_dwg::read(&dwg).unwrap();
    let by = |n: &str| drawing.layers.values().find(|l| l.name == n).unwrap_or_else(|| panic!("{n} missing"));
    let state = |n: &str| (by(n).frozen, by(n).off, by(n).locked, by(n).color);
    assert_eq!(state("0"), (false, false, false, 7));
    assert_eq!(state("Frozen"), (true, false, false, 1));
    assert_eq!(state("Locked"), (false, false, true, 2));
    assert_eq!(state("Hidden"), (false, true, false, 3));
    assert_eq!(state("Both"), (true, true, true, 250));
}
