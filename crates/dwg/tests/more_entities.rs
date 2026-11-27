// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg(not(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64"))))]
//! TEXT, LWPOLYLINE and INSERT, with the block table, read against files the bridge writes.
mod common;

use common::Rng;
use forja_dwg::Drawing;
use forja_dwg::object::Entity;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs()))
}

struct Doc(String);

impl Doc {
    fn new() -> Self {
        let mut d = Doc(String::new());
        d.g(0, "SECTION");
        d.g(2, "HEADER");
        d.g(9, "$ACADVER");
        d.g(1, "AC1015");
        d.g(0, "ENDSEC");
        d
    }
    fn g(&mut self, c: i32, v: &str) {
        self.0.push_str(&format!("{c:>3}\n{v}\n"));
    }
    fn n(&mut self, c: i32, v: f64) {
        self.g(c, &v.to_string());
    }
    fn finish(mut self) -> Drawing {
        self.g(0, "EOF");
        let dwg = nodalia_dwg::dxf_to_dwg_as(self.0.as_bytes(), "AC1015").unwrap();
        let drawing = forja_dwg::read(&dwg).unwrap();
        assert!(drawing.damaged.is_empty(), "{:?}", drawing.damaged);
        drawing
    }
}

#[test]
fn text_is_read_with_its_fields() {
    let mut d = Doc::new();
    d.g(0, "SECTION");
    d.g(2, "ENTITIES");
    for (value, h, rot, h_align, v_align) in [("Hello", 2.5, 0.0, 0, 0), ("Línea ñ", 4.0, 30.0, 1, 2), ("Wide", 1.0, 0.0, 2, 3)] {
        d.g(0, "TEXT");
        d.g(8, "0");
        d.n(10, 10.0);
        d.n(20, 20.0);
        d.n(30, 0.0);
        d.n(40, h);
        d.g(1, value);
        if rot != 0.0 {
            d.n(50, rot);
        }
        if h_align != 0 || v_align != 0 {
            d.g(72, &h_align.to_string());
            d.n(11, 12.0);
            d.n(21, 22.0);
            d.n(31, 0.0);
            d.g(73, &v_align.to_string());
        }
    }
    d.g(0, "ENDSEC");
    let drawing = d.finish();
    let texts: Vec<_> = drawing.entities.iter().filter_map(|e| if let Entity::Text(t) = e { Some(t) } else { None }).collect();
    assert_eq!(texts.len(), 3);
    assert_eq!(texts[0].value, "Hello");
    assert!(close(texts[0].height, 2.5) && texts[0].insertion == [10.0, 20.0, 0.0] && texts[0].alignment.is_none());
    assert_eq!(texts[1].value, "Línea ñ");
    assert!(close(texts[1].rotation, 30f64.to_radians()) && texts[1].horizontal == 1 && texts[1].vertical == 2);
    assert_eq!(texts[1].alignment, Some([12.0, 22.0]));
    assert_eq!((texts[2].horizontal, texts[2].vertical), (2, 3));
    assert!(texts.iter().all(|t| t.style.is_some()));
    assert!(texts.iter().all(|t| drawing.layer_of(&Entity::Text((*t).clone())).map(|l| l.name.as_str()) == Some("0")));
}

#[test]
fn lwpolylines_keep_vertices_bulges_widths_and_closing() {
    let mut rng = Rng(0xC0FFEE);
    let mut d = Doc::new();
    d.g(0, "SECTION");
    d.g(2, "ENTITIES");
    let mut sources = Vec::new();
    for k in 0..60 {
        let n = 2 + (rng.next() % 9) as usize;
        let closed = k % 3 == 0;
        let bulged = k % 2 == 0;
        let wide = k % 5 == 0;
        let constant = k % 7 == 0;
        let pts: Vec<[f64; 2]> = (0..n).map(|_| [rng.f(-300.0, 300.0), rng.f(-300.0, 300.0)]).collect();
        let bulges: Vec<f64> = (0..n).map(|i| if bulged && i % 2 == 0 { rng.f(-1.5, 1.5) } else { 0.0 }).collect();
        let widths: Vec<[f64; 2]> = (0..n).map(|_| [rng.f(0.1, 3.0), rng.f(0.1, 3.0)]).collect();
        d.g(0, "LWPOLYLINE");
        d.g(8, "0");
        d.g(90, &n.to_string());
        d.g(70, if closed { "1" } else { "0" });
        if constant {
            d.n(43, 0.75);
        }
        for i in 0..n {
            d.n(10, pts[i][0]);
            d.n(20, pts[i][1]);
            if wide {
                d.n(40, widths[i][0]);
                d.n(41, widths[i][1]);
            }
            if bulges[i] != 0.0 {
                d.n(42, bulges[i]);
            }
        }
        sources.push((pts, bulges, widths, closed, wide, constant));
    }
    d.g(0, "ENDSEC");
    let drawing = d.finish();
    let polys: Vec<_> = drawing.entities.iter().filter_map(|e| if let Entity::LwPolyline(p) = e { Some(p) } else { None }).collect();
    assert_eq!(polys.len(), sources.len());
    for (p, (pts, bulges, widths, closed, wide, constant)) in polys.iter().zip(&sources) {
        assert_eq!(p.closed, *closed);
        assert_eq!(p.vertices.len(), pts.len());
        if *constant {
            assert!(close(p.constant_width, 0.75));
        }
        for (i, v) in p.vertices.iter().enumerate() {
            assert!(close(v.point[0], pts[i][0]) && close(v.point[1], pts[i][1]), "vertex {i}");
            assert!(close(v.bulge, bulges[i]), "bulge {i}: {} vs {}", v.bulge, bulges[i]);
            if *wide {
                assert!(close(v.start_width, widths[i][0]) && close(v.end_width, widths[i][1]), "width {i}");
            }
        }
    }
}

#[test]
fn inserts_point_at_their_blocks_and_blocks_own_their_entities() {
    let mut d = Doc::new();
    d.g(0, "SECTION");
    d.g(2, "BLOCKS");
    d.g(0, "BLOCK");
    d.g(8, "0");
    d.g(2, "Chair");
    d.g(70, "0");
    d.n(10, 0.0);
    d.n(20, 0.0);
    d.n(30, 0.0);
    d.g(0, "LINE");
    d.g(8, "0");
    d.n(10, 0.0);
    d.n(20, 0.0);
    d.n(30, 0.0);
    d.n(11, 1.0);
    d.n(21, 1.0);
    d.n(31, 0.0);
    d.g(0, "ENDBLK");
    d.g(8, "0");
    d.g(0, "ENDSEC");
    d.g(0, "SECTION");
    d.g(2, "ENTITIES");
    let scales: [[f64; 3]; 4] = [[1.0, 1.0, 1.0], [2.0, 2.0, 2.0], [2.0, 3.0, 1.0], [-1.0, 1.0, 1.0]];
    for (i, s) in scales.iter().enumerate() {
        d.g(0, "INSERT");
        d.g(8, "0");
        d.g(2, "Chair");
        d.n(10, 100.0 * i as f64);
        d.n(20, 5.0);
        d.n(30, 0.0);
        d.n(41, s[0]);
        d.n(42, s[1]);
        d.n(43, s[2]);
        d.n(50, 15.0 * i as f64);
    }
    d.g(0, "ENDSEC");
    let drawing = d.finish();
    let chair = drawing.blocks.values().find(|b| b.name == "Chair").expect("block table has Chair");
    assert!(!chair.anonymous && !chair.xref);
    assert!(drawing.blocks.values().any(|b| b.name == "*Model_Space"));
    let inserts: Vec<_> = drawing.entities.iter().filter_map(|e| if let Entity::Insert(i) = e { Some(i) } else { None }).collect();
    assert_eq!(inserts.len(), 4);
    for (i, ins) in inserts.iter().enumerate() {
        assert_eq!(drawing.block_of(ins).map(|b| b.name.as_str()), Some("Chair"));
        assert_eq!(ins.position, [100.0 * i as f64, 5.0, 0.0]);
        assert!(ins.scale.iter().zip(scales[i]).all(|(a, b)| close(*a, b)), "scale {:?} vs {:?}", ins.scale, scales[i]);
        assert!(close(ins.rotation, (15.0 * i as f64).to_radians()));
        assert!(!ins.has_attributes);
    }
    // The line inside the block is owned by the block, not by model space.
    let inside: Vec<_> = drawing.entities_of_block(chair.handle).collect();
    assert_eq!(inside.len(), 1);
    assert!(matches!(inside[0], Entity::Line { .. }));
}

#[test]
fn mutated_files_with_the_new_entities_never_panic() {
    let mut d = Doc::new();
    d.g(0, "SECTION");
    d.g(2, "BLOCKS");
    d.g(0, "BLOCK");
    d.g(8, "0");
    d.g(2, "B");
    d.g(70, "0");
    d.n(10, 0.0);
    d.n(20, 0.0);
    d.n(30, 0.0);
    d.g(0, "ENDBLK");
    d.g(8, "0");
    d.g(0, "ENDSEC");
    d.g(0, "SECTION");
    d.g(2, "ENTITIES");
    d.g(0, "TEXT");
    d.g(8, "0");
    d.n(10, 1.0);
    d.n(20, 2.0);
    d.n(30, 0.0);
    d.n(40, 3.0);
    d.g(1, "abc");
    d.g(0, "LWPOLYLINE");
    d.g(8, "0");
    d.g(90, "3");
    d.g(70, "1");
    for (x, y) in [(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)] {
        d.n(10, x);
        d.n(20, y);
        d.n(42, 0.5);
    }
    d.g(0, "INSERT");
    d.g(8, "0");
    d.g(2, "B");
    d.n(10, 1.0);
    d.n(20, 1.0);
    d.n(30, 0.0);
    d.g(0, "ENDSEC");
    d.g(0, "EOF");
    let dwg = nodalia_dwg::dxf_to_dwg_as(d.0.as_bytes(), "AC1015").unwrap();
    let mut rng = Rng(99);
    for _ in 0..4000 {
        let mut bad = dwg.clone();
        for _ in 0..1 + rng.next() % 4 {
            let at = (rng.next() as usize) % bad.len();
            bad[at] ^= 1 << (rng.next() % 8);
        }
        let _ = std::panic::catch_unwind(|| forja_dwg::read(&bad)).expect("the reader must not panic");
    }
}
