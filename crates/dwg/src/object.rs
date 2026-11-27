// SPDX-License-Identifier: MIT OR Apache-2.0
//! Objects of a 2000-format file: the common header, the type table and the entities decoded so
//! far (LINE, CIRCLE, ARC and POINT).

use crate::bits::Bits;
use crate::crc::{SECTION_SEED, crc16};
use crate::error::{Error, Result};
use crate::file::{Class, ObjectRef};

/// Largest object body accepted, in bytes.
const MAX_OBJECT_BYTES: usize = 1 << 26;
/// Most extended-data blocks or reactors one object may declare.
const MAX_LISTED: u32 = 1 << 20;

/// The DXF-style name of a fixed object type code, if the format defines one.
pub fn type_name(code: u16) -> Option<&'static str> {
    Some(match code {
        1 => "TEXT",
        2 => "ATTRIB",
        3 => "ATTDEF",
        4 => "BLOCK",
        5 => "ENDBLK",
        6 => "SEQEND",
        7 => "INSERT",
        8 => "MINSERT",
        10 => "VERTEX_2D",
        11 => "VERTEX_3D",
        12 => "VERTEX_MESH",
        13 => "VERTEX_PFACE",
        14 => "VERTEX_PFACE_FACE",
        15 => "POLYLINE_2D",
        16 => "POLYLINE_3D",
        17 => "ARC",
        18 => "CIRCLE",
        19 => "LINE",
        20 => "DIMENSION_ORDINATE",
        21 => "DIMENSION_LINEAR",
        22 => "DIMENSION_ALIGNED",
        23 => "DIMENSION_ANG_3PT",
        24 => "DIMENSION_ANG_2LN",
        25 => "DIMENSION_RADIUS",
        26 => "DIMENSION_DIAMETER",
        27 => "POINT",
        28 => "3DFACE",
        29 => "POLYLINE_PFACE",
        30 => "POLYLINE_MESH",
        31 => "SOLID",
        32 => "TRACE",
        33 => "SHAPE",
        34 => "VIEWPORT",
        35 => "ELLIPSE",
        36 => "SPLINE",
        37 => "REGION",
        38 => "3DSOLID",
        39 => "BODY",
        40 => "RAY",
        41 => "XLINE",
        42 => "DICTIONARY",
        44 => "MTEXT",
        45 => "LEADER",
        46 => "TOLERANCE",
        47 => "MLINE",
        48 => "BLOCK_CONTROL",
        49 => "BLOCK_HEADER",
        50 => "LAYER_CONTROL",
        51 => "LAYER",
        52 => "STYLE_CONTROL",
        53 => "STYLE",
        56 => "LTYPE_CONTROL",
        57 => "LTYPE",
        60 => "VIEW_CONTROL",
        61 => "VIEW",
        62 => "UCS_CONTROL",
        63 => "UCS",
        64 => "VPORT_CONTROL",
        65 => "VPORT",
        66 => "APPID_CONTROL",
        67 => "APPID",
        68 => "DIMSTYLE_CONTROL",
        69 => "DIMSTYLE",
        70 => "VP_ENT_HDR_CONTROL",
        71 => "VP_ENT_HDR",
        72 => "GROUP",
        73 => "MLINESTYLE",
        74 => "OLE2FRAME",
        76 => "LONG_TRANSACTION",
        77 => "LWPOLYLINE",
        78 => "HATCH",
        79 => "XRECORD",
        80 => "ACDBPLACEHOLDER",
        81 => "VBA_PROJECT",
        82 => "LAYOUT",
        _ => return None,
    })
}

/// True for the fixed type codes that are entities (drawn things) rather than table records or
/// other objects.
pub fn is_entity_code(code: u16) -> bool {
    matches!(code, 1..=8 | 10..=41 | 43..=47 | 77 | 78)
}

/// An object's header: who it is and what it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// The object's handle.
    pub handle: u64,
    /// The type code (fixed codes below 500, class numbers from 500).
    pub code: u16,
    /// Where the object's data ends and its handle stream begins, in bits from the start of the
    /// object body.
    pub data_bits: usize,
}

/// The bytes of an object, after the size and with the checksum verified.
pub fn body(data: &[u8], at: ObjectRef) -> Result<&[u8]> {
    let mut head = Bits::new(data.get(at.offset..).ok_or(Error::Truncated("an object"))?);
    let size = usize::try_from(head.ms()?).map_err(|_| Error::TooLarge("object"))?;
    if size > MAX_OBJECT_BYTES {
        return Err(Error::TooLarge("object"));
    }
    let start = at.offset + head.pos() / 8;
    let end = start.checked_add(size).ok_or(Error::Invalid("object size"))?;
    let body = data.get(start..end).ok_or(Error::Truncated("an object"))?;
    let stored = data.get(end..end + 2).ok_or(Error::Truncated("an object checksum"))?;
    if crc16(data.get(at.offset..end).ok_or(Error::Truncated("an object"))?, SECTION_SEED) != u16::from_le_bytes([stored[0], stored[1]]) {
        return Err(Error::Checksum("an object"));
    }
    Ok(body)
}

/// Read an object's header from its body.
pub fn header(body: &[u8], expected: u64) -> Result<(Header, Bits<'_>)> {
    let mut bits = Bits::new(body);
    let code = bits.bs()?;
    let data_bits = bits.rl()? as usize;
    let (_, handle) = bits.handle()?;
    if handle != expected {
        return Err(Error::Invalid("object handle does not match the object map"));
    }
    if data_bits > body.len().saturating_mul(8) {
        return Err(Error::Invalid("object data size"));
    }
    Ok((Header { handle, code, data_bits }, bits))
}

/// Entity properties common to every kind.
#[derive(Clone, Debug, PartialEq)]
pub struct Common {
    /// The entity's handle.
    pub handle: u64,
    /// Handle of the layer it is on.
    pub layer: Option<u64>,
    /// Colour index (256 = by layer, 0 = by block).
    pub color: u16,
    /// Line type scale.
    pub ltype_scale: f64,
    /// True when the entity is invisible.
    pub invisible: bool,
    /// Line weight code.
    pub lineweight: u8,
}

/// An entity this reader decodes.
#[derive(Clone, Debug, PartialEq)]
pub enum Entity {
    /// A line segment.
    Line { common: Common, start: [f64; 3], end: [f64; 3], thickness: f64, extrusion: [f64; 3] },
    /// A circle.
    Circle { common: Common, center: [f64; 3], radius: f64, thickness: f64, extrusion: [f64; 3] },
    /// A circular arc (angles in radians, counter-clockwise).
    Arc { common: Common, center: [f64; 3], radius: f64, thickness: f64, extrusion: [f64; 3], start_angle: f64, end_angle: f64 },
    /// A point.
    Point { common: Common, position: [f64; 3], thickness: f64, extrusion: [f64; 3], x_axis_angle: f64 },
}

/// What reading one object gave.
#[derive(Clone, Debug, PartialEq)]
pub enum Decoded {
    /// A supported entity.
    Entity(Entity),
    /// An entity or object this reader does not decode yet (its type code).
    Skipped(u16),
}

/// Resolve a handle read from the handle stream against the owning object's handle.
fn resolve(code: u8, value: u64, own: u64) -> u64 {
    match code {
        6 => own.wrapping_add(1),
        8 => own.wrapping_sub(1),
        0xA => own.wrapping_add(value),
        0xC => own.wrapping_sub(value),
        _ => value,
    }
}

fn skip_extended_data(bits: &mut Bits<'_>) -> Result<()> {
    for _ in 0..MAX_LISTED {
        let size = usize::from(bits.bs()?);
        if size == 0 {
            return Ok(());
        }
        bits.handle()?;
        bits.bytes(size)?;
    }
    Err(Error::TooLarge("extended data"))
}

/// Decode the common entity data that precedes an entity's own fields, then the handle stream.
/// `bits` is positioned just after the object handle; the entity-specific reader gets it next.
struct Prefix {
    entmode: u8,
    reactors: u32,
    nolinks: bool,
    color: u16,
    ltype_scale: f64,
    invisible: bool,
    lineweight: u8,
}

fn read_prefix(bits: &mut Bits<'_>) -> Result<Prefix> {
    skip_extended_data(bits)?;
    if bits.bit()? {
        // A cached picture of the entity: skip its bytes.
        let n = bits.rl()? as usize;
        bits.bytes(n)?;
    }
    let entmode = bits.bb()?;
    let reactors = bits.bl()?;
    if reactors > MAX_LISTED {
        return Err(Error::TooLarge("reactor list"));
    }
    let nolinks = bits.bit()?;
    let color = bits.bs()?;
    let ltype_scale = bits.bd()?;
    // Line type and plot style flags: they only add handles at the end of the stream.
    bits.bb()?;
    bits.bb()?;
    let invisible = bits.bs()? & 1 == 1;
    let lineweight = bits.rc()?;
    Ok(Prefix { entmode, reactors, nolinks, color, ltype_scale, invisible, lineweight })
}

/// Read the handle stream that follows an entity's data and return its layer handle.
///
/// The stream runs: owner (only when the entity is not in model or paper space), reactors, the
/// extended dictionary, the previous and next entity (unless the entity has no links), then the
/// layer, and the line type and plot style when those are set explicitly.
fn read_layer(bits: &mut Bits<'_>, h: &Header, p: &Prefix) -> Result<Option<u64>> {
    bits.seek(h.data_bits)?;
    if p.entmode == 0 {
        bits.handle()?;
    }
    for _ in 0..p.reactors {
        bits.handle()?;
    }
    bits.handle()?;
    if !p.nolinks {
        bits.handle()?;
        bits.handle()?;
    }
    let (code, value) = bits.handle()?;
    Ok(Some(resolve(code, value, h.handle)))
}

/// Decode one object. Types this reader does not know are reported as [`Decoded::Skipped`].
pub fn decode(data: &[u8], at: ObjectRef, _classes: &[Class]) -> Result<Decoded> {
    let body = body(data, at)?;
    let (h, mut bits) = header(body, at.handle)?;
    if !matches!(h.code, 17 | 18 | 19 | 27) {
        return Ok(Decoded::Skipped(h.code));
    }
    let p = read_prefix(&mut bits)?;
    let mut fields = bits.clone();
    let entity = match h.code {
        19 => {
            let z_zero = fields.bit()?;
            let sx = fields.rd()?;
            let ex = fields.dd(sx)?;
            let sy = fields.rd()?;
            let ey = fields.dd(sy)?;
            let (sz, ez) = if z_zero {
                (0.0, 0.0)
            } else {
                let sz = fields.rd()?;
                (sz, fields.dd(sz)?)
            };
            let thickness = fields.bt()?;
            let extrusion = fields.be()?;
            Kind::Line { start: [sx, sy, sz], end: [ex, ey, ez], thickness, extrusion }
        }
        18 => Kind::Circle { center: fields.bd3()?, radius: fields.bd()?, thickness: fields.bt()?, extrusion: fields.be()? },
        17 => Kind::Arc {
            center: fields.bd3()?,
            radius: fields.bd()?,
            thickness: fields.bt()?,
            extrusion: fields.be()?,
            start_angle: fields.bd()?,
            end_angle: fields.bd()?,
        },
        _ => Kind::Point { position: fields.bd3()?, thickness: fields.bt()?, extrusion: fields.be()?, x_axis_angle: fields.bd()? },
    };
    let mut stream = bits;
    let layer = read_layer(&mut stream, &h, &p)?;
    let common = Common { handle: h.handle, layer, color: p.color, ltype_scale: p.ltype_scale, invisible: p.invisible, lineweight: p.lineweight };
    Ok(Decoded::Entity(match entity {
        Kind::Line { start, end, thickness, extrusion } => Entity::Line { common, start, end, thickness, extrusion },
        Kind::Circle { center, radius, thickness, extrusion } => Entity::Circle { common, center, radius, thickness, extrusion },
        Kind::Arc { center, radius, thickness, extrusion, start_angle, end_angle } => {
            Entity::Arc { common, center, radius, thickness, extrusion, start_angle, end_angle }
        }
        Kind::Point { position, thickness, extrusion, x_axis_angle } => Entity::Point { common, position, thickness, extrusion, x_axis_angle },
    }))
}

enum Kind {
    Line { start: [f64; 3], end: [f64; 3], thickness: f64, extrusion: [f64; 3] },
    Circle { center: [f64; 3], radius: f64, thickness: f64, extrusion: [f64; 3] },
    Arc { center: [f64; 3], radius: f64, thickness: f64, extrusion: [f64; 3], start_angle: f64, end_angle: f64 },
    Point { position: [f64; 3], thickness: f64, extrusion: [f64; 3], x_axis_angle: f64 },
}
