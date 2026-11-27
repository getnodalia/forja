// SPDX-License-Identifier: MIT OR Apache-2.0
//! What can go wrong while reading a DWG file. Every input is hostile: nothing here panics.

use std::fmt;

/// A reading failure with where it happened, so a damaged file can be reported usefully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The bytes do not start like a DWG file.
    NotDwg,
    /// A DWG version this reader does not decode yet (the code, e.g. `AC1032`).
    UnsupportedVersion(String),
    /// A read went past the end of the data it was allowed to use.
    Truncated(&'static str),
    /// A value the format does not allow.
    Invalid(&'static str),
    /// A checksum did not match (what was being checked).
    Checksum(&'static str),
    /// A declared size is beyond what this reader accepts.
    TooLarge(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotDwg => write!(f, "not a DWG file"),
            Error::UnsupportedVersion(v) => write!(f, "DWG version {v} is not supported by the native reader yet"),
            Error::Truncated(what) => write!(f, "DWG data ends early while reading {what}"),
            Error::Invalid(what) => write!(f, "invalid DWG data: {what}"),
            Error::Checksum(what) => write!(f, "DWG checksum mismatch in {what}"),
            Error::TooLarge(what) => write!(f, "DWG {what} is larger than the reader accepts"),
        }
    }
}

impl std::error::Error for Error {}

/// Result of a DWG read.
pub type Result<T> = std::result::Result<T, Error>;
