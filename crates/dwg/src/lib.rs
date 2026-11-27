// SPDX-License-Identifier: MIT OR Apache-2.0
//! Forja's own DWG reader, written from the Open Design Alliance's public specification.
//!
//! It is a work in progress: it reads the 2000 format (`AC1015`) container, classes, object map
//! and a first set of entities and layers. Everything else (other versions, most entity types, writing) still
//! goes through the bridge in `nodalia-dwg`. See the crate README for what is and is not covered.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

pub mod bits;
pub mod crc;
pub mod error;
pub mod file;
pub mod object;

pub use error::{Error, Result};

use std::collections::BTreeMap;

use object::{Decoded, Entity, Layer};

/// The version code this reader decodes.
pub const SUPPORTED_VERSION: &str = "AC1015";

/// The DWG version code at the start of `bytes` (such as `AC1032`), if they look like a DWG file.
pub fn version(bytes: &[u8]) -> Option<&str> {
    let head = bytes.get(..6)?;
    (head.starts_with(b"AC10") && head.get(4..6)?.iter().all(u8::is_ascii_digit)).then(|| std::str::from_utf8(head).ok()).flatten()
}

/// What reading every object of a drawing gave.
#[derive(Clone, Debug, Default)]
pub struct Drawing {
    /// The code page number the file declares.
    pub codepage: u16,
    /// The classes the file defines.
    pub classes: Vec<file::Class>,
    /// The entities this reader decodes, in object-map order.
    pub entities: Vec<Entity>,
    /// The layers, by handle.
    pub layers: BTreeMap<u64, Layer>,
    /// Objects of types not decoded yet, counted by type code.
    pub skipped: BTreeMap<u16, usize>,
    /// Objects that were present but damaged: their handle and the reason.
    pub damaged: Vec<(u64, Error)>,
}

impl Drawing {
    /// The layer an entity is on, if its layer handle resolves to a decoded layer.
    pub fn layer_of(&self, entity: &Entity) -> Option<&Layer> {
        self.layers.get(&entity.common().layer?)
    }
}

/// Read a drawing from DWG bytes. Only the 2000 format is decoded so far; any other version fails
/// with [`Error::UnsupportedVersion`] so the caller can fall back to another reader.
pub fn read(bytes: &[u8]) -> Result<Drawing> {
    let v = version(bytes).ok_or(Error::NotDwg)?;
    if v != SUPPORTED_VERSION {
        return Err(Error::UnsupportedVersion(v.to_string()));
    }
    let container = file::Container::parse(bytes)?;
    let mut drawing = Drawing { codepage: container.codepage, classes: container.classes.clone(), ..Drawing::default() };
    for at in &container.objects {
        match object::decode(bytes, *at, &container.classes) {
            Ok(Decoded::Entity(e)) => drawing.entities.push(e),
            Ok(Decoded::Layer(l)) => {
                drawing.layers.insert(l.handle, l);
            }
            Ok(Decoded::Skipped(code)) => *drawing.skipped.entry(code).or_insert(0) += 1,
            Err(e) => drawing.damaged.push((at.handle, e)),
        }
    }
    Ok(drawing)
}
