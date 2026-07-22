use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use binary_record_cli::{RecordFileError, append_file, get_file, list_file};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new() -> Self {
        let unique = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "binary-record-cli-test-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("temporary test directory should be created");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn append_list_and_get_preserve_arbitrary_bytes() {
    let temp = TempDirectory::new();
    let records = temp.join("records.bin");
    let first = temp.join("first.bin");
    let second = temp.join("second.bin");
    let first_payload = [0, b'\n', 0xff, 0xfe];
    let second_payload = b"second record\n";
    fs::write(&first, first_payload).unwrap();
    fs::write(&second, second_payload).unwrap();

    assert_eq!(append_file(&records, &first).unwrap(), 0);
    assert_eq!(append_file(&records, &second).unwrap(), 1);
    assert_eq!(
        list_file(&records).unwrap(),
        [
            binary_record_cli::RecordInfo {
                number: 0,
                payload_bytes: first_payload.len(),
            },
            binary_record_cli::RecordInfo {
                number: 1,
                payload_bytes: second_payload.len(),
            },
        ]
    );
    assert_eq!(get_file(&records, 0).unwrap(), first_payload);
    assert_eq!(get_file(&records, 1).unwrap(), second_payload);
}

#[test]
fn missing_file_returns_an_io_error() {
    let temp = TempDirectory::new();
    let error = list_file(&temp.join("missing.bin")).unwrap_err();
    assert!(matches!(error, RecordFileError::Io { .. }));
}

#[test]
fn missing_record_reports_requested_number_and_count() {
    let temp = TempDirectory::new();
    let records = temp.join("records.bin");
    fs::write(&records, []).unwrap();
    let error = get_file(&records, 7).unwrap_err();
    assert!(matches!(
        error,
        RecordFileError::RecordNotFound {
            requested: 7,
            record_count: 0
        }
    ));
}

#[test]
fn malformed_existing_file_is_not_appended_to() {
    let temp = TempDirectory::new();
    let records = temp.join("records.bin");
    let input = temp.join("input.bin");
    fs::write(&records, [0, 0, 0, 4, 1]).unwrap();
    fs::write(&input, [9, 9]).unwrap();
    let before = fs::read(&records).unwrap();

    let error = append_file(&records, &input).unwrap_err();
    assert!(matches!(error, RecordFileError::TruncatedPayload { .. }));
    assert_eq!(fs::read(&records).unwrap(), before);
}
