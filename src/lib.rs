//! Reusable binary-record encoding, decoding, and file operations.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const LENGTH_PREFIX_BYTES: usize = 4;

/// One decoded record that borrows its payload from the original file buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedRecord<'a> {
    pub number: usize,
    pub payload: &'a [u8],
}

/// Information displayed by the `list` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordInfo {
    pub number: usize,
    pub payload_bytes: usize,
}

/// Every expected failure produced by the record-file library.
#[derive(Debug)]
pub enum RecordFileError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    TruncatedLength {
        byte_offset: usize,
        remaining_bytes: usize,
    },
    TruncatedPayload {
        record_number: usize,
        declared_bytes: usize,
        remaining_bytes: usize,
    },
    PayloadTooLarge {
        payload_bytes: usize,
    },
    RecordNotFound {
        requested: usize,
        record_count: usize,
    },
}

impl fmt::Display for RecordFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} '{}': {source}",
                path.display()
            ),
            Self::TruncatedLength {
                byte_offset,
                remaining_bytes,
            } => write!(
                formatter,
                "malformed record file: length prefix at byte {byte_offset} needs 4 bytes but only {remaining_bytes} remain"
            ),
            Self::TruncatedPayload {
                record_number,
                declared_bytes,
                remaining_bytes,
            } => write!(
                formatter,
                "malformed record {record_number}: length declares {declared_bytes} payload bytes but only {remaining_bytes} remain"
            ),
            Self::PayloadTooLarge { payload_bytes } => write!(
                formatter,
                "payload contains {payload_bytes} bytes, exceeding the u32 record limit"
            ),
            Self::RecordNotFound {
                requested,
                record_count,
            } => write!(
                formatter,
                "record {requested} does not exist; file contains {record_count} records"
            ),
        }
    }
}

impl std::error::Error for RecordFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, RecordFileError>;

/// Decode and validate every record without copying its payload.
pub fn decode_records(bytes: &[u8]) -> Result<Vec<DecodedRecord<'_>>> {
    let mut records = Vec::new();
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        let remaining = bytes.len() - cursor;
        if remaining < LENGTH_PREFIX_BYTES {
            return Err(RecordFileError::TruncatedLength {
                byte_offset: cursor,
                remaining_bytes: remaining,
            });
        }

        let length_end = cursor + LENGTH_PREFIX_BYTES;
        let length_bytes: [u8; LENGTH_PREFIX_BYTES] = bytes[cursor..length_end]
            .try_into()
            .expect("the four-byte length slice was checked above");
        let payload_length = u32::from_be_bytes(length_bytes) as usize;
        let payload_start = length_end;
        let payload_end =
            payload_start
                .checked_add(payload_length)
                .ok_or(RecordFileError::TruncatedPayload {
                    record_number: records.len(),
                    declared_bytes: payload_length,
                    remaining_bytes: bytes.len() - payload_start,
                })?;

        if payload_end > bytes.len() {
            return Err(RecordFileError::TruncatedPayload {
                record_number: records.len(),
                declared_bytes: payload_length,
                remaining_bytes: bytes.len() - payload_start,
            });
        }

        records.push(DecodedRecord {
            number: records.len(),
            payload: &bytes[payload_start..payload_end],
        });
        cursor = payload_end;
    }

    Ok(records)
}

/// Encode one arbitrary byte payload as a length-prefixed record.
pub fn encode_record(payload: &[u8]) -> Result<Vec<u8>> {
    let payload_length =
        u32::try_from(payload.len()).map_err(|_| RecordFileError::PayloadTooLarge {
            payload_bytes: payload.len(),
        })?;

    let mut encoded = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload.len());
    encoded.extend_from_slice(&payload_length.to_be_bytes());
    encoded.extend_from_slice(payload);
    Ok(encoded)
}

/// Append all bytes from `input_path` as one record in `record_path`.
pub fn append_file(record_path: &Path, input_path: &Path) -> Result<usize> {
    let input = read_path(input_path, "read input file")?;

    let existing = match fs::read(record_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(source) => {
            return Err(RecordFileError::Io {
                operation: "read record file",
                path: record_path.to_path_buf(),
                source,
            });
        }
    };

    // Validate the complete existing file before adding another record.
    let record_number = decode_records(&existing)?.len();
    let encoded = encode_record(&input)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(record_path)
        .map_err(|source| RecordFileError::Io {
            operation: "open record file for append",
            path: record_path.to_path_buf(),
            source,
        })?;

    file.write_all(&encoded)
        .map_err(|source| RecordFileError::Io {
            operation: "append record",
            path: record_path.to_path_buf(),
            source,
        })?;

    Ok(record_number)
}

/// Return record numbers and lengths without exposing or interpreting payloads.
pub fn list_file(record_path: &Path) -> Result<Vec<RecordInfo>> {
    let bytes = read_path(record_path, "read record file")?;
    let records = decode_records(&bytes)?;
    Ok(records
        .iter()
        .map(|record| RecordInfo {
            number: record.number,
            payload_bytes: record.payload.len(),
        })
        .collect())
}

/// Return an owned copy of one record payload.
pub fn get_file(record_path: &Path, requested: usize) -> Result<Vec<u8>> {
    let bytes = read_path(record_path, "read record file")?;
    let records = decode_records(&bytes)?;
    records
        .get(requested)
        .map(|record| record.payload.to_vec())
        .ok_or(RecordFileError::RecordNotFound {
            requested,
            record_count: records.len(),
        })
}

/// Write raw bytes to a destination file.
pub fn write_output(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).map_err(|source| RecordFileError::Io {
        operation: "write output file",
        path: path.to_path_buf(),
        source,
    })
}

fn read_path(path: &Path, operation: &'static str) -> Result<Vec<u8>> {
    fs::read(path).map_err(|source| RecordFileError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_contains_no_records() {
        assert_eq!(decode_records(&[]).unwrap(), Vec::new());
    }

    #[test]
    fn arbitrary_payloads_round_trip() {
        let payloads: [&[u8]; 4] = [b"", b"hello\nworld", &[0, 1, 0, 2], &[0xff, 0xfe]];
        let mut file = Vec::new();

        for payload in payloads {
            file.extend(encode_record(payload).unwrap());
        }

        let decoded = decode_records(&file).unwrap();
        assert_eq!(decoded.len(), payloads.len());
        for (number, (record, expected)) in decoded.iter().zip(payloads).enumerate() {
            assert_eq!(record.number, number);
            assert_eq!(record.payload, expected);
        }
    }

    #[test]
    fn truncated_length_is_an_error() {
        let error = decode_records(&[0, 0, 0]).unwrap_err();
        assert!(matches!(
            error,
            RecordFileError::TruncatedLength {
                byte_offset: 0,
                remaining_bytes: 3
            }
        ));
    }

    #[test]
    fn truncated_payload_is_an_error() {
        let error = decode_records(&[0, 0, 0, 5, 1, 2]).unwrap_err();
        assert!(matches!(
            error,
            RecordFileError::TruncatedPayload {
                record_number: 0,
                declared_bytes: 5,
                remaining_bytes: 2
            }
        ));
    }
}
