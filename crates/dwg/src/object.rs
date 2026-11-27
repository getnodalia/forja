// SPDX-License-Identifier: MIT OR Apache-2.0
//! Objects of a 2000-format file: the common header, the type table and the objects decoded so
//! far (LINE, CIRCLE, ARC, POINT, TEXT, LWPOLYLINE and INSERT entities, layers and block headers).

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
    /// Handle of the block definition that owns the entity, when it is not in model or paper
    /// space.
    pub owner: Option<u64>,
    /// True when the entity is in paper space (false: model space or inside a block).
    pub paper_space: bool,
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
    /// A single line of text.
    Text(Text),
    /// A lightweight polyline.
    LwPolyline(LwPolyline),
    /// A reference to a block definition.
    Insert(Insert),
}

/// A TEXT entity.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    /// Common properties.
    pub common: Common,
    /// Insertion point (z is the elevation).
    pub insertion: [f64; 3],
    /// Second alignment point, when the text is aligned by one.
    pub alignment: Option<[f64; 2]>,
    /// Extrusion direction.
    pub extrusion: [f64; 3],
    /// Thickness.
    pub thickness: f64,
    /// Oblique angle, in radians.
    pub oblique: f64,
    /// Rotation, in radians.
    pub rotation: f64,
    /// Text height.
    pub height: f64,
    /// Width factor.
    pub width_factor: f64,
    /// The text. The file stores it in its code page; only UTF-8 and Latin-1 are recovered exactly.
    pub value: String,
    /// Generation flags (bit 0 backward, bit 1 upside down).
    pub generation: u16,
    /// Horizontal alignment code (0 left, 1 centre, 2 right, 3 aligned, 4 middle, 5 fit).
    pub horizontal: u16,
    /// Vertical alignment code (0 baseline, 1 bottom, 2 middle, 3 top).
    pub vertical: u16,
    /// Handle of the text style.
    pub style: Option<u64>,
}

/// One vertex of a lightweight polyline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LwVertex {
    /// Position in the polyline's plane.
    pub point: [f64; 2],
    /// Bulge of the segment that starts here (tangent of a quarter of its included angle).
    pub bulge: f64,
    /// Width at the start of that segment.
    pub start_width: f64,
    /// Width at its end.
    pub end_width: f64,
}

/// An LWPOLYLINE entity.
#[derive(Clone, Debug, PartialEq)]
pub struct LwPolyline {
    /// Common properties.
    pub common: Common,
    /// True when the last vertex joins the first.
    pub closed: bool,
    /// Width applied to every segment that has none of its own.
    pub constant_width: f64,
    /// Elevation of the polyline's plane.
    pub elevation: f64,
    /// Thickness.
    pub thickness: f64,
    /// Extrusion direction.
    pub extrusion: [f64; 3],
    /// The vertices.
    pub vertices: Vec<LwVertex>,
}

/// An INSERT entity.
#[derive(Clone, Debug, PartialEq)]
pub struct Insert {
    /// Common properties.
    pub common: Common,
    /// Insertion point.
    pub position: [f64; 3],
    /// Scale factors.
    pub scale: [f64; 3],
    /// Rotation, in radians.
    pub rotation: f64,
    /// Extrusion direction.
    pub extrusion: [f64; 3],
    /// Handle of the block header it inserts.
    pub block: Option<u64>,
    /// True when the insert carries attributes.
    pub has_attributes: bool,
}

/// A block definition (an entry of the block table).
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The block header's handle: what an [`Insert::block`] and an entity's owner refer to.
    pub handle: u64,
    /// The block's name.
    pub name: String,
    /// True for blocks the program made itself (dimensions, hatch patterns).
    pub anonymous: bool,
    /// True when the block is an external reference.
    pub xref: bool,
}

/// A layer (an entry of the layer table).
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// The layer's handle: what an entity's [`Common::layer`] refers to.
    pub handle: u64,
    /// The layer's name. The file stores it in its code page; only UTF-8 and Latin-1 text is
    /// recovered exactly so far.
    pub name: String,
    /// True when the layer is frozen.
    pub frozen: bool,
    /// True when the layer is turned off.
    pub off: bool,
    /// True when the layer is frozen in new viewports.
    pub frozen_in_new: bool,
    /// True when the layer is locked.
    pub locked: bool,
    /// True when the layer plots.
    pub plot: bool,
    /// Line weight code (31 = default).
    pub lineweight: u8,
    /// Colour index.
    pub color: u16,
}

impl Entity {
    /// The properties every entity has.
    pub fn common(&self) -> &Common {
        match self {
            Entity::Line { common, .. } | Entity::Circle { common, .. } | Entity::Arc { common, .. } | Entity::Point { common, .. } => common,
            Entity::Text(t) => &t.common,
            Entity::LwPolyline(p) => &p.common,
            Entity::Insert(i) => &i.common,
        }
    }
}

/// What reading one object gave.
#[derive(Clone, Debug, PartialEq)]
pub enum Decoded {
    /// A supported entity.
    Entity(Entity),
    /// A layer table entry.
    Layer(Layer),
    /// A block table entry.
    Block(Block),
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
    /// Line type flags: 3 means a line type handle is in the stream.
    ltype_flags: u8,
    /// Plot style flags: 3 means a plot style handle is in the stream.
    plot_flags: u8,
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
    let ltype_flags = bits.bb()?;
    let plot_flags = bits.bb()?;
    let invisible = bits.bs()? & 1 == 1;
    let lineweight = bits.rc()?;
    Ok(Prefix { entmode, reactors, nolinks, color, ltype_scale, invisible, lineweight, ltype_flags, plot_flags })
}

/// What the common part of an entity's handle stream held.
struct Stream {
    owner: Option<u64>,
    layer: u64,
}

/// Read the common part of the handle stream that follows an entity's data, leaving `bits` at the
/// handles that belong to the entity itself.
///
/// The stream runs: owner (only when the entity is not in model or paper space), reactors, the
/// extended dictionary, the previous and next entity (unless the entity has no links), then the
/// layer, and the line type and plot style when those are set explicitly.
fn read_stream(bits: &mut Bits<'_>, h: &Header, p: &Prefix) -> Result<Stream> {
    bits.seek(h.data_bits)?;
    let owner = if p.entmode == 0 {
        let (code, value) = bits.handle()?;
        Some(resolve(code, value, h.handle))
    } else {
        None
    };
    for _ in 0..p.reactors {
        bits.handle()?;
    }
    bits.handle()?;
    if !p.nolinks {
        bits.handle()?;
        bits.handle()?;
    }
    let (code, value) = bits.handle()?;
    let layer = resolve(code, value, h.handle);
    if p.ltype_flags == 3 {
        bits.handle()?;
    }
    if p.plot_flags == 3 {
        bits.handle()?;
    }
    Ok(Stream { owner, layer })
}

/// Text from a file, in UTF-8 when it is valid and Latin-1 otherwise.
fn text_of(raw: Vec<u8>) -> String {
    match String::from_utf8(raw) {
        Ok(s) => s,
        Err(e) => e.into_bytes().into_iter().map(char::from).collect(),
    }
}

/// A handle from the entity-specific part of the stream, resolved against the entity.
fn entity_handle(bits: &mut Bits<'_>, own: u64) -> Result<u64> {
    let (code, value) = bits.handle()?;
    Ok(resolve(code, value, own))
}

/// Decode one object. Types this reader does not know are reported as [`Decoded::Skipped`].
pub fn decode(data: &[u8], at: ObjectRef, _classes: &[Class]) -> Result<Decoded> {
    let body = body(data, at)?;
    let (h, mut bits) = header(body, at.handle)?;
    if h.code == 51 {
        return decode_layer(&h, bits).map(Decoded::Layer);
    }
    if h.code == 49 {
        return decode_block(&h, bits).map(Decoded::Block);
    }
    if !matches!(h.code, 1 | 7 | 17 | 18 | 19 | 27 | 77) {
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
        1 => Kind::Text(read_text(&mut fields)?),
        7 => Kind::Insert(read_insert(&mut fields)?),
        77 => Kind::LwPolyline(read_lwpolyline(&mut fields)?),
        _ => Kind::Point { position: fields.bd3()?, thickness: fields.bt()?, extrusion: fields.be()?, x_axis_angle: fields.bd()? },
    };
    let mut stream = bits;
    let common_stream = read_stream(&mut stream, &h, &p)?;
    let common = Common {
        handle: h.handle,
        layer: Some(common_stream.layer),
        owner: common_stream.owner,
        paper_space: p.entmode == 1,
        color: p.color,
        ltype_scale: p.ltype_scale,
        invisible: p.invisible,
        lineweight: p.lineweight,
    };
    Ok(Decoded::Entity(match entity {
        Kind::Text(t) => {
            let style = entity_handle(&mut stream, h.handle).ok();
            Entity::Text(Text { common, style, ..t })
        }
        Kind::Insert(i) => {
            let block = entity_handle(&mut stream, h.handle).ok();
            Entity::Insert(Insert { common, block, ..i })
        }
        Kind::LwPolyline(p) => Entity::LwPolyline(LwPolyline { common, ..p }),
        Kind::Line { start, end, thickness, extrusion } => Entity::Line { common, start, end, thickness, extrusion },
        Kind::Circle { center, radius, thickness, extrusion } => Entity::Circle { common, center, radius, thickness, extrusion },
        Kind::Arc { center, radius, thickness, extrusion, start_angle, end_angle } => {
            Entity::Arc { common, center, radius, thickness, extrusion, start_angle, end_angle }
        }
        Kind::Point { position, thickness, extrusion, x_axis_angle } => Entity::Point { common, position, thickness, extrusion, x_axis_angle },
    }))
}

/// Decode a LAYER object: the common object data, the name, then a packed flags short (bit 0
/// frozen, 1 off, 2 frozen in new viewports, 3 locked, 4 plots, 5-9 line weight) and the colour.
fn decode_layer(h: &Header, mut bits: Bits<'_>) -> Result<Layer> {
    skip_extended_data(&mut bits)?;
    let reactors = bits.bl()?;
    if reactors > MAX_LISTED {
        return Err(Error::TooLarge("reactor list"));
    }
    let name = text_of(bits.text()?);
    bits.bit()?; // the "64" flag: set when something refers to the layer
    bits.bs()?; // external reference index
    bits.bit()?; // depends on an external reference
    let flags = bits.bs()?;
    let color = bits.bs()?;
    Ok(Layer {
        handle: h.handle,
        name,
        frozen: flags & 1 != 0,
        off: flags & 2 != 0,
        frozen_in_new: flags & 4 != 0,
        locked: flags & 8 != 0,
        plot: flags & 16 != 0,
        lineweight: ((flags >> 5) & 0x1F) as u8,
        color,
    })
}

/// The data of a TEXT entity: flags say which fields are present, the rest take their defaults.
fn read_text(f: &mut Bits<'_>) -> Result<Text> {
    let flags = f.rc()?;
    let elevation = if flags & 0x01 == 0 { f.rd()? } else { 0.0 };
    let (ix, iy) = (f.rd()?, f.rd()?);
    let alignment = if flags & 0x02 == 0 {
        let ax = f.dd(ix)?;
        Some([ax, f.dd(iy)?])
    } else {
        None
    };
    let extrusion = f.be()?;
    let thickness = f.bt()?;
    let oblique = if flags & 0x04 == 0 { f.rd()? } else { 0.0 };
    let rotation = if flags & 0x08 == 0 { f.rd()? } else { 0.0 };
    let height = f.rd()?;
    let width_factor = if flags & 0x10 == 0 { f.rd()? } else { 1.0 };
    let value = text_of(f.text()?);
    let generation = if flags & 0x20 == 0 { f.bs()? } else { 0 };
    let horizontal = if flags & 0x40 == 0 { f.bs()? } else { 0 };
    let vertical = if flags & 0x80 == 0 { f.bs()? } else { 0 };
    Ok(Text {
        common: EMPTY_COMMON,
        insertion: [ix, iy, elevation],
        alignment,
        extrusion,
        thickness,
        oblique,
        rotation,
        height,
        width_factor,
        value,
        generation,
        horizontal,
        vertical,
        style: None,
    })
}

/// The data of an INSERT entity.
fn read_insert(f: &mut Bits<'_>) -> Result<Insert> {
    let position = f.bd3()?;
    let scale = match f.bb()? {
        0 => {
            let x = f.rd()?;
            [x, f.dd(x)?, f.dd(x)?]
        }
        1 => [1.0, f.dd(1.0)?, f.dd(1.0)?],
        2 => {
            let x = f.rd()?;
            [x, x, x]
        }
        _ => [1.0, 1.0, 1.0],
    };
    let rotation = f.bd()?;
    let extrusion = f.be()?;
    let has_attributes = f.bit()?;
    Ok(Insert { common: EMPTY_COMMON, position, scale, rotation, extrusion, block: None, has_attributes })
}

/// The data of an LWPOLYLINE entity.
fn read_lwpolyline(f: &mut Bits<'_>) -> Result<LwPolyline> {
    let flag = f.bs()?;
    let constant_width = if flag & 4 != 0 { f.bd()? } else { 0.0 };
    let elevation = if flag & 8 != 0 { f.bd()? } else { 0.0 };
    let thickness = if flag & 2 != 0 { f.bd()? } else { 0.0 };
    let extrusion = if flag & 1 != 0 { f.bd3()? } else { [0.0, 0.0, 1.0] };
    let points = f.bl()? as usize;
    let bulges = if flag & 16 != 0 { f.bl()? as usize } else { 0 };
    let widths = if flag & 32 != 0 { f.bl()? as usize } else { 0 };
    // Every vertex takes at least two bits, so a count above the bits left is a lie.
    let room = f.remaining();
    if points > room / 2 || bulges > room / 2 || widths > room / 2 {
        return Err(Error::TooLarge("polyline vertex count"));
    }
    let mut vertices: Vec<LwVertex> = Vec::with_capacity(points);
    for i in 0..points {
        let point = match vertices.last() {
            Some(prev) if i > 0 => {
                let x = f.dd(prev.point[0])?;
                [x, f.dd(prev.point[1])?]
            }
            _ => [f.rd()?, f.rd()?],
        };
        vertices.push(LwVertex { point, bulge: 0.0, start_width: constant_width, end_width: constant_width });
    }
    for i in 0..bulges {
        let b = f.bd()?;
        if let Some(v) = vertices.get_mut(i) {
            v.bulge = b;
        }
    }
    for i in 0..widths {
        let (s, e) = (f.bd()?, f.bd()?);
        if let Some(v) = vertices.get_mut(i) {
            v.start_width = s;
            v.end_width = e;
        }
    }
    Ok(LwPolyline { common: EMPTY_COMMON, closed: flag & 512 != 0, constant_width, elevation, thickness, extrusion, vertices })
}

const EMPTY_COMMON: Common =
    Common { handle: 0, layer: None, owner: None, paper_space: false, color: 256, ltype_scale: 1.0, invisible: false, lineweight: 0 };

/// Decode a BLOCK_HEADER object: the common object data, then the name and a few flags.
fn decode_block(h: &Header, mut bits: Bits<'_>) -> Result<Block> {
    skip_extended_data(&mut bits)?;
    let reactors = bits.bl()?;
    if reactors > MAX_LISTED {
        return Err(Error::TooLarge("reactor list"));
    }
    let name = text_of(bits.text()?);
    bits.bit()?; // the "64" flag
    bits.bs()?; // external reference index
    bits.bit()?; // depends on an external reference
    let anonymous = bits.bit()?;
    bits.bit()?; // has attribute definitions
    let xref = bits.bit()?;
    Ok(Block { handle: h.handle, name, anonymous, xref })
}

enum Kind {
    Text(Text),
    Insert(Insert),
    LwPolyline(LwPolyline),
    Line { start: [f64; 3], end: [f64; 3], thickness: f64, extrusion: [f64; 3] },
    Circle { center: [f64; 3], radius: f64, thickness: f64, extrusion: [f64; 3] },
    Arc { center: [f64; 3], radius: f64, thickness: f64, extrusion: [f64; 3], start_angle: f64, end_angle: f64 },
    Point { position: [f64; 3], thickness: f64, extrusion: [f64; 3], x_axis_angle: f64 },
}
