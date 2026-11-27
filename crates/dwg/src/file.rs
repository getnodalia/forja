// SPDX-License-Identifier: MIT OR Apache-2.0
//! The 2000-format (`AC1015`) file container: the header with its section locators, the class
//! section and the object map.

use crate::bits::Bits;
use crate::crc::{SECTION_SEED, crc16};
use crate::error::{Error, Result};

/// The 16 bytes that end the file header.
const HEADER_END_SENTINEL: [u8; 16] = [0x95, 0xA0, 0x4E, 0x28, 0x99, 0x82, 0x1A, 0xE5, 0x5E, 0x41, 0xE0, 0x5F, 0x9D, 0x3A, 0x4D, 0x00];
/// Start and end sentinels of the header-variables section.
const VARS_START: [u8; 16] = [0xCF, 0x7B, 0x1F, 0x23, 0xFD, 0xDE, 0x38, 0xA9, 0x5F, 0x7C, 0x68, 0xB8, 0x4E, 0x6D, 0x33, 0x5F];
const VARS_END: [u8; 16] = [0x30, 0x84, 0xE0, 0xDC, 0x02, 0x21, 0xC7, 0x56, 0xA0, 0x83, 0x97, 0x47, 0xB1, 0x92, 0xCC, 0xA0];
/// Start and end sentinels of the class section.
const CLASSES_START: [u8; 16] = [0x8D, 0xA1, 0xC4, 0xB8, 0xC4, 0xA9, 0xF8, 0xC5, 0xC0, 0xDC, 0xF4, 0x5F, 0xE7, 0xCF, 0xB6, 0x8A];
const CLASSES_END: [u8; 16] = [0x72, 0x5E, 0x3B, 0x47, 0x3B, 0x56, 0x07, 0x3A, 0x3F, 0x23, 0x0B, 0xA0, 0x18, 0x30, 0x49, 0x75];

/// Most section locators a file header may declare.
const MAX_LOCATORS: usize = 16;
/// Most objects one drawing may hold (the object map is bounded by the file size too).
pub const MAX_OBJECTS: usize = 8_000_000;
/// Largest class table.
const MAX_CLASSES: usize = 100_000;

/// Which part of the file a locator points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKind {
    /// The header variables.
    Header,
    /// The class definitions.
    Classes,
    /// The object map (handle → file offset).
    ObjectMap,
    /// Anything else, by record number.
    Other(u8),
}

/// One entry of the file header's section table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Locator {
    /// What the section holds.
    pub kind: SectionKind,
    /// Where it starts.
    pub offset: usize,
    /// How long it is.
    pub size: usize,
}

/// A class defined by the file (custom entities and objects use class numbers from 500).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    /// The class number objects refer to.
    pub number: u16,
    /// The proxy flags.
    pub proxy_flags: u16,
    /// The application that defines it.
    pub application: String,
    /// The C++ class name.
    pub cpp_name: String,
    /// The DXF record name.
    pub dxf_name: String,
    /// True when the class is an entity (item class id 0x1F2), false for an object (0x1F3).
    pub is_entity: bool,
}

/// Where an object starts, by handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectRef {
    /// The object's handle.
    pub handle: u64,
    /// Byte offset of the object in the file.
    pub offset: usize,
}

/// The parsed container of a 2000-format file.
#[derive(Clone, Debug)]
pub struct Container {
    /// The code page number from the header.
    pub codepage: u16,
    /// The section table.
    pub sections: Vec<Locator>,
    /// The class definitions.
    pub classes: Vec<Class>,
    /// Every object in the drawing, in the order the map lists them.
    pub objects: Vec<ObjectRef>,
}

fn le_u32(data: &[u8], at: usize) -> Result<u32> {
    let b = data.get(at..at.checked_add(4).ok_or(Error::Invalid("offset"))?).ok_or(Error::Truncated("a header field"))?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn le_u16(data: &[u8], at: usize) -> Result<u16> {
    let b = data.get(at..at.checked_add(2).ok_or(Error::Invalid("offset"))?).ok_or(Error::Truncated("a header field"))?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}

/// The slice `data[offset..offset+size]`, or an error saying which section is outside the file.
fn slice<'a>(data: &'a [u8], offset: usize, size: usize, what: &'static str) -> Result<&'a [u8]> {
    let end = offset.checked_add(size).ok_or(Error::Invalid(what))?;
    data.get(offset..end).ok_or(Error::Truncated(what))
}

impl Container {
    /// Parse the header, class section and object map of a 2000-format file.
    pub fn parse(data: &[u8]) -> Result<Container> {
        let codepage = le_u16(data, 0x13)?;
        let count = le_u32(data, 0x15)? as usize;
        if count > MAX_LOCATORS {
            return Err(Error::TooLarge("section table"));
        }
        let mut sections = Vec::with_capacity(count);
        let mut p = 0x19usize;
        for _ in 0..count {
            let number = *data.get(p).ok_or(Error::Truncated("a section locator"))?;
            let offset = le_u32(data, p + 1)? as usize;
            let size = le_u32(data, p + 5)? as usize;
            let kind = match number {
                0 => SectionKind::Header,
                1 => SectionKind::Classes,
                2 => SectionKind::ObjectMap,
                n => SectionKind::Other(n),
            };
            sections.push(Locator { kind, offset, size });
            p += 9;
        }
        // The header checksum is seeded with 0 and folded with a value that depends on how many
        // locators there are (published for 3 to 6).
        let stored = le_u16(data, p)?;
        let sum = crc16(data.get(..p).ok_or(Error::Truncated("the file header"))?, 0);
        let fold = match count {
            3 => 0xA598,
            4 => 0x8101,
            5 => 0x3CC4,
            6 => 0x8461,
            _ => return Err(Error::Invalid("unexpected section count")),
        };
        if sum ^ fold != stored {
            return Err(Error::Checksum("the file header"));
        }
        if data.get(p + 2..p + 18) != Some(&HEADER_END_SENTINEL[..]) {
            return Err(Error::Invalid("file header sentinel"));
        }
        let find = |kind: SectionKind| sections.iter().find(|s| s.kind == kind).copied();
        let vars = find(SectionKind::Header).ok_or(Error::Invalid("no header variables section"))?;
        check_sized(data, vars, &VARS_START, &VARS_END, "header variables")?;
        let classes_loc = find(SectionKind::Classes).ok_or(Error::Invalid("no class section"))?;
        let classes = read_classes(data, classes_loc)?;
        let map = find(SectionKind::ObjectMap).ok_or(Error::Invalid("no object map"))?;
        let objects = read_object_map(data, map)?;
        Ok(Container { codepage, sections, classes, objects })
    }
}

/// Check a section that is `start sentinel, RL size, data, CRC, end sentinel`: both sentinels and
/// the checksum. Returns the data between the size and the checksum.
fn check_sized<'a>(data: &'a [u8], loc: Locator, start: &[u8; 16], end: &[u8; 16], what: &'static str) -> Result<&'a [u8]> {
    let whole = slice(data, loc.offset, loc.size, what)?;
    if whole.get(..16) != Some(&start[..]) {
        return Err(Error::Invalid(what));
    }
    let size = le_u32(whole, 16)? as usize;
    let body = whole.get(20..20usize.checked_add(size).ok_or(Error::Invalid(what))?).ok_or(Error::Truncated(what))?;
    let crc_at = 20 + size;
    let stored = le_u16(whole, crc_at)?;
    // The checksum covers the size field and the data.
    if crc16(whole.get(16..crc_at).ok_or(Error::Truncated(what))?, SECTION_SEED) != stored {
        return Err(Error::Checksum(what));
    }
    if whole.get(crc_at + 2..crc_at + 18) != Some(&end[..]) {
        return Err(Error::Invalid(what));
    }
    Ok(body)
}

fn lossy(b: Vec<u8>) -> String {
    String::from_utf8_lossy(&b).into_owned()
}

fn read_classes(data: &[u8], loc: Locator) -> Result<Vec<Class>> {
    let body = check_sized(data, loc, &CLASSES_START, &CLASSES_END, "class section")?;
    let mut bits = Bits::new(body);
    let mut classes = Vec::new();
    // Each class takes well over a byte, so the body size bounds the loop too.
    while bits.remaining() >= 16 && classes.len() < MAX_CLASSES {
        let number = bits.bs()?;
        let proxy_flags = bits.bs()?;
        let application = lossy(bits.text()?);
        let cpp_name = lossy(bits.text()?);
        let dxf_name = lossy(bits.text()?);
        let _was_zombie = bits.bit()?;
        let item = bits.bs()?;
        classes.push(Class { number, proxy_flags, application, cpp_name, dxf_name, is_entity: item == 0x1F2 });
    }
    Ok(classes)
}

fn read_object_map(data: &[u8], loc: Locator) -> Result<Vec<ObjectRef>> {
    let mut at = loc.offset;
    let mut out = Vec::new();
    loop {
        let head = data.get(at..at.checked_add(2).ok_or(Error::Invalid("object map"))?).ok_or(Error::Truncated("object map"))?;
        let size = usize::from(u16::from_be_bytes([head[0], head[1]]));
        if size == 2 {
            return Ok(out); // The last page holds only its size and checksum.
        }
        if !(2..=2040).contains(&size) {
            return Err(Error::Invalid("object map page size"));
        }
        let page = slice(data, at, size, "object map page")?;
        let stored = slice(data, at + size, 2, "object map checksum")?;
        if crc16(page, SECTION_SEED) != u16::from_be_bytes([stored[0], stored[1]]) {
            return Err(Error::Checksum("object map page"));
        }
        let mut bits = Bits::new(page.get(2..).ok_or(Error::Truncated("object map"))?);
        // Handles and offsets are running sums within a page; each page starts again from zero.
        let (mut handle, mut offset) = (0i64, 0i64);
        while bits.remaining() >= 16 {
            let dh = bits.mc_unsigned()?;
            let dl = bits.mc_signed()?;
            handle = handle.checked_add(i64::try_from(dh).map_err(|_| Error::Invalid("handle step"))?).ok_or(Error::Invalid("handle"))?;
            offset = offset.checked_add(dl).ok_or(Error::Invalid("offset"))?;
            if out.len() >= MAX_OBJECTS {
                return Err(Error::TooLarge("object map"));
            }
            out.push(ObjectRef {
                handle: u64::try_from(handle).map_err(|_| Error::Invalid("negative handle"))?,
                offset: usize::try_from(offset).map_err(|_| Error::Invalid("negative object offset"))?,
            });
        }
        at = at.checked_add(size + 2).ok_or(Error::Invalid("object map"))?;
    }
}
