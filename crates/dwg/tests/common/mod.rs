// SPDX-License-Identifier: MIT OR Apache-2.0
#![allow(dead_code)]
//! Test drawings, written by the existing bridge (the oracle) and read by the native reader.

pub fn dxf() -> String {
    let mut s = String::new();
    let mut g = |c: i32, v: &str| s.push_str(&format!("{c:>3}\n{v}\n"));
    g(0, "SECTION");
    g(2, "HEADER");
    g(9, "$ACADVER");
    g(1, "AC1015");
    g(0, "ENDSEC");
    g(0, "SECTION");
    g(2, "ENTITIES");
    g(0, "LINE");
    g(8, "0");
    g(10, "1.5");
    g(20, "2.5");
    g(30, "0.0");
    g(11, "10.25");
    g(21, "-4.0");
    g(31, "0.0");
    g(0, "CIRCLE");
    g(8, "0");
    g(10, "5.0");
    g(20, "6.0");
    g(30, "0.0");
    g(40, "3.5");
    g(0, "ARC");
    g(8, "0");
    g(10, "0.0");
    g(20, "0.0");
    g(30, "0.0");
    g(40, "2.0");
    g(50, "10.0");
    g(51, "100.0");
    g(0, "POINT");
    g(8, "0");
    g(10, "7.0");
    g(20, "8.0");
    g(30, "9.0");
    g(0, "ENDSEC");
    g(0, "EOF");
    s
}

pub fn sample_dwg() -> Vec<u8> {
    nodalia_dwg::dxf_to_dwg_as(dxf().as_bytes(), "AC1015").unwrap()
}

/// A small deterministic generator (xorshift64*), so the tests need no extra crate.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// A number in `[lo, hi)` with a mix of "round" and awkward values.
    pub fn f(&mut self, lo: f64, hi: f64) -> f64 {
        let u = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        match self.next() % 4 {
            0 => (lo + u * (hi - lo)).round(),
            1 => ((lo + u * (hi - lo)) * 4.0).round() / 4.0,
            _ => lo + u * (hi - lo),
        }
    }
}

/// What a generated drawing holds, in the order the entities were written.
#[derive(Clone, Debug)]
pub enum Source {
    Line { layer: &'static str, a: [f64; 3], b: [f64; 3], thickness: f64, ext: [f64; 3] },
    Circle { layer: &'static str, c: [f64; 3], r: f64, thickness: f64, ext: [f64; 3] },
    Arc { layer: &'static str, c: [f64; 3], r: f64, thickness: f64, ext: [f64; 3], a0: f64, a1: f64 },
    Point { layer: &'static str, p: [f64; 3], thickness: f64, ext: [f64; 3], angle: f64 },
}

pub const LAYERS: [&str; 3] = ["0", "Walls", "Doors"];

/// A drawing with `n` random LINE, CIRCLE, ARC and POINT entities on three layers.
pub fn random_drawing(seed: u64, n: usize) -> (String, Vec<Source>) {
    let mut rng = Rng(seed | 1);
    let mut out = String::new();
    let mut g = |c: i32, v: String| out.push_str(&format!("{c:>3}\n{v}\n"));
    g(0, "SECTION".into());
    g(2, "HEADER".into());
    g(9, "$ACADVER".into());
    g(1, "AC1015".into());
    g(0, "ENDSEC".into());
    g(0, "SECTION".into());
    g(2, "TABLES".into());
    g(0, "TABLE".into());
    g(2, "LAYER".into());
    g(70, "3".into());
    for (i, name) in LAYERS.iter().enumerate() {
        g(0, "LAYER".into());
        g(2, (*name).into());
        g(70, "0".into());
        g(62, (i + 1).to_string());
        g(6, "CONTINUOUS".into());
    }
    g(0, "ENDTAB".into());
    g(0, "ENDSEC".into());
    g(0, "SECTION".into());
    g(2, "ENTITIES".into());
    let mut sources = Vec::new();
    for _ in 0..n {
        let layer = LAYERS[(rng.next() % 3) as usize];
        let flat = rng.next().is_multiple_of(2);
        let pt = |rng: &mut Rng| [rng.f(-500.0, 500.0), rng.f(-500.0, 500.0), if flat { 0.0 } else { rng.f(-50.0, 50.0) }];
        let thickness = if rng.next().is_multiple_of(4) { rng.f(0.1, 5.0) } else { 0.0 };
        let ext = if rng.next().is_multiple_of(5) { [0.0, 0.0, -1.0] } else { [0.0, 0.0, 1.0] };
        let kind = rng.next() % 4;
        let common = |g: &mut dyn FnMut(i32, String), name: &str| {
            g(0, name.into());
            g(8, layer.into());
            if thickness != 0.0 {
                g(39, thickness.to_string());
            }
            if ext[2] < 0.0 {
                g(210, "0.0".into());
                g(220, "0.0".into());
                g(230, "-1.0".into());
            }
        };
        match kind {
            0 => {
                let (a, b) = (pt(&mut rng), pt(&mut rng));
                common(&mut g, "LINE");
                for (i, c) in [10, 20, 30].into_iter().enumerate() {
                    g(c, a[i].to_string());
                }
                for (i, c) in [11, 21, 31].into_iter().enumerate() {
                    g(c, b[i].to_string());
                }
                sources.push(Source::Line { layer, a, b, thickness, ext });
            }
            1 => {
                let (c, r) = (pt(&mut rng), rng.f(0.5, 100.0));
                common(&mut g, "CIRCLE");
                for (i, k) in [10, 20, 30].into_iter().enumerate() {
                    g(k, c[i].to_string());
                }
                g(40, r.to_string());
                sources.push(Source::Circle { layer, c, r, thickness, ext });
            }
            2 => {
                let (c, r) = (pt(&mut rng), rng.f(0.5, 100.0));
                let (a0, a1) = (rng.f(0.0, 180.0), rng.f(180.0, 360.0));
                common(&mut g, "ARC");
                for (i, k) in [10, 20, 30].into_iter().enumerate() {
                    g(k, c[i].to_string());
                }
                g(40, r.to_string());
                g(50, a0.to_string());
                g(51, a1.to_string());
                sources.push(Source::Arc { layer, c, r, thickness, ext, a0, a1 });
            }
            _ => {
                let p = pt(&mut rng);
                common(&mut g, "POINT");
                for (i, k) in [10, 20, 30].into_iter().enumerate() {
                    g(k, p[i].to_string());
                }
                sources.push(Source::Point { layer, p, thickness, ext, angle: 0.0 });
            }
        }
    }
    g(0, "ENDSEC".into());
    g(0, "EOF".into());
    (out, sources)
}

pub fn random_dwg(seed: u64, n: usize) -> (Vec<u8>, Vec<Source>) {
    let (dxf, sources) = random_drawing(seed, n);
    (nodalia_dwg::dxf_to_dwg_as(dxf.as_bytes(), "AC1015").unwrap(), sources)
}
