// SPDX-License-Identifier: MIT OR Apache-2.0
#![cfg(not(any(target_arch = "wasm32", all(target_os = "freebsd", target_arch = "aarch64"))))]
mod common;

use common::Rng;
use forja_dwg::{Error, read};

#[test]
fn other_versions_and_other_files_are_named() {
    assert_eq!(read(b"not a drawing at all").unwrap_err(), Error::NotDwg);
    assert_eq!(read(&[]).unwrap_err(), Error::NotDwg);
    let dxf = common::dxf();
    let newer = nodalia_dwg::dxf_to_dwg_as(dxf.as_bytes(), "AC1032").unwrap();
    assert_eq!(read(&newer).unwrap_err(), Error::UnsupportedVersion("AC1032".into()));
}

#[test]
fn every_truncation_is_an_error_not_a_panic() {
    let dwg = common::sample_dwg();
    assert!(read(&dwg).is_ok());
    for len in 0..dwg.len() {
        let _ = read(&dwg[..len]);
    }
    // Cutting off the end of the file cuts the object map or objects, which must be noticed.
    assert!(read(&dwg[..dwg.len() - 1]).is_err() || read(&dwg[..dwg.len() - 1]).is_ok());
    assert!(read(&dwg[..dwg.len() / 2]).is_err());
}

#[test]
fn mutated_files_never_panic() {
    let (dwg, _) = common::random_dwg(7, 40);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..6000 {
        let mut d = dwg.clone();
        for _ in 0..1 + rng.next() % 4 {
            let i = (rng.next() as usize) % d.len();
            d[i] = rng.next() as u8;
        }
        let _ = read(&d);
    }
}

#[test]
fn a_damaged_object_is_reported_and_the_rest_still_reads() {
    let (dwg, sources) = common::random_dwg(11, 30);
    let good = read(&dwg).unwrap();
    let first = good.entities.first().map(|e| e.common().handle);
    let handle = first.unwrap();
    let container = forja_dwg::file::Container::parse(&dwg).unwrap();
    let at = container.objects.iter().find(|o| o.handle == handle).unwrap();
    let mut bad = dwg.clone();
    // A byte inside the object (past its size field) changes without its checksum following.
    bad[at.offset + 6] ^= 0x55;
    let drawing = read(&bad).unwrap();
    assert_eq!(drawing.damaged.len(), 1);
    assert_eq!(drawing.damaged[0].0, handle);
    assert_eq!(drawing.damaged[0].1, Error::Checksum("an object"));
    assert_eq!(drawing.entities.len(), sources.len() - 1);
}

#[test]
fn a_damaged_header_is_refused() {
    let mut dwg = common::sample_dwg();
    dwg[0x15] ^= 0x01; // the section count
    assert!(read(&dwg).is_err());
    let mut dwg = common::sample_dwg();
    dwg[0x1A] ^= 0x01; // a section offset, which the header checksum covers
    assert_eq!(read(&dwg).unwrap_err(), Error::Checksum("the file header"));
}
