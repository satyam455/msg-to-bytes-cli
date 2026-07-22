# Binary Record CLI

This is the first practice project before building the replicated message log.
It teaches the Rust skills needed to safely read and write arbitrary bytes,
assign record numbers, report errors with `Result`, and test malformed input.

## What we are building

The program stores several binary records in one file. Each record receives a
number based on its position:

```text
record 0
record 1
record 2
...
```

The records are raw bytes, not lines of text. A record may therefore contain:

- text;
- newlines;
- zero bytes;
- image bytes;
- bytes that are not valid UTF-8.

## Why we need a file format

If arbitrary records are placed directly beside one another, the reader cannot
know where one record ends and the next begins. Each payload is therefore
preceded by a four-byte unsigned length in big-endian order.

```text
+----------------------+-------------------+
| payload length: u32  | payload: N bytes  |
+----------------------+-------------------+
        4 bytes               N bytes
```

A complete file is a repetition of that layout:

```text
+--------+-----------+--------+-----------+--------+-----------+
| len 0  | payload 0 | len 1  | payload 1 | len 2  | payload 2 |
+--------+-----------+--------+-----------+--------+-----------+
```

Example: the payload `cat` has three bytes, so its encoded record is:

```text
00 00 00 03 63 61 74
| length=3 | c  a  t
```

This is a small version of the self-delimiting record idea that the main
storage engine will later use with offsets, timestamps, flags, and checksums.

## Commands

### Append an input file as one record

```bash
cargo run -- append records.bin photo.jpg
```

The CLI reads every byte from `photo.jpg`, appends one length-prefixed record to
`records.bin`, and prints the assigned record number.

### List records

```bash
cargo run -- list records.bin
```

Output shows each record number and payload length:

```text
0: 128 bytes
1: 2048 bytes
```

The program does not print payloads during `list`, because arbitrary bytes may
not be valid terminal text.

### Extract one record

Write the raw payload to standard output:

```bash
cargo run -- get records.bin 1 > recovered.bin
```

Or write it directly to a named output file:

```bash
cargo run -- get records.bin 1 recovered.bin
```

### Show help

```bash
cargo run -- help
```

## Errors we handle

The program returns a `Result` and prints a useful message when:

- a command or required argument is missing;
- an unknown command is supplied;
- a record number is not an unsigned integer;
- the input or record file does not exist;
- a file cannot be read, created, appended, or written;
- fewer than four bytes remain for a record length;
- a length says a payload is larger than the remaining bytes;
- a payload is too large to represent with a `u32` length;
- the requested record number does not exist.

Bad user input and malformed files must never cause a panic.

## Code structure

```text
cli/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs       # reusable parsing, encoding, file operations, and errors
│   └── main.rs      # command-line argument handling and user-facing output
└── tests/
    └── cli.rs       # public API tests using temporary files
```

### `src/lib.rs`

The library owns the actual behavior so it can be tested without starting a
child process. It contains:

- `RecordFileError`, the typed error enum;
- `decode_records`, which borrows input bytes and validates the entire format;
- `encode_record`, which creates one length-prefixed record;
- `append_file`, which appends an arbitrary input file and returns its number;
- `list_file`, which returns record numbers and lengths;
- `get_file`, which returns one owned payload.

### `src/main.rs`

The binary handles only the outer CLI concerns:

- collect arguments;
- choose a command;
- parse the record number;
- call library functions;
- print output or return an error.

Keeping command parsing separate from file-format logic makes both easier to
understand and test.

## Ownership design

The decoder borrows the complete file as `&[u8]` and returns borrowed payload
slices. It does not copy every record just to list or count them:

```text
owned Vec<u8> containing file
            |
            +---- &bytes[4..7]  record 0 payload
            +---- &bytes[11..]  record 1 payload
```

Those slices cannot outlive the owned file buffer. When `get_file` must return
a payload after its local file buffer is dropped, it converts the chosen slice
to an owned `Vec<u8>`. This is intentional and demonstrates why Rust prevents
returning references to local variables.

The append path borrows the input payload while encoding it. The encoded
`Vec<u8>` owns the bytes that are passed to `write_all`.

## Invariants

1. Every record begins with exactly four length bytes.
2. A declared payload must fit completely inside the file.
3. No trailing bytes are silently ignored.
4. Record numbers start at zero and follow file order.
5. Appending never changes existing record bytes.
6. Listing never requires payload bytes to be UTF-8.
7. Expected errors are returned, not panicked.

## Test plan

Unit tests cover:

- empty files;
- empty payloads;
- text, newline, zero, and invalid UTF-8 bytes;
- multiple record round trips;
- truncated length prefixes;
- truncated payloads;
- missing record numbers;
- missing files;
- append/list/get behavior through temporary files.

Run all checks with:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```
# msg-to-bytes-cli
