use std::env;
use std::error::Error;
use std::io::{self, Write};
use std::path::Path;

use binary_record_cli::{append_file, get_file, list_file, write_output};

const HELP: &str = "Binary Record CLI

Usage:
  binary-record-cli append <records-file> <input-file>
  binary-record-cli list   <records-file>
  binary-record-cli get    <records-file> <record-number> [output-file]
  binary-record-cli help
";

fn main() {
    if let Err(error) = run(env::args().skip(1)) {
        eprintln!("error: {error}");
        eprintln!("\n{HELP}");
        std::process::exit(1);
    }
}

fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = arguments.into_iter().collect();
    let Some(command) = args.first().map(String::as_str) else {
        return Err("missing command".into());
    };

    match command {
        "append" => {
            require_argument_count(&args, 3, "append requires a record file and input file")?;
            let number = append_file(Path::new(&args[1]), Path::new(&args[2]))?;
            println!("appended record {number}");
        }
        "list" => {
            require_argument_count(&args, 2, "list requires a record file")?;
            for record in list_file(Path::new(&args[1]))? {
                println!("{}: {} bytes", record.number, record.payload_bytes);
            }
        }
        "get" => {
            if args.len() != 3 && args.len() != 4 {
                return Err(
                    "get requires a record file, record number, and optional output file".into(),
                );
            }
            let number = args[2]
                .parse::<usize>()
                .map_err(|_| format!("'{}' is not a valid unsigned record number", args[2]))?;
            let payload = get_file(Path::new(&args[1]), number)?;
            if let Some(output_path) = args.get(3) {
                write_output(Path::new(output_path), &payload)?;
            } else {
                io::stdout().write_all(&payload)?;
            }
        }
        "help" | "--help" | "-h" => {
            require_argument_count(&args, 1, "help does not accept arguments")?;
            print!("{HELP}");
        }
        unknown => return Err(format!("unknown command '{unknown}'").into()),
    }

    Ok(())
}

fn require_argument_count(
    arguments: &[String],
    expected: usize,
    message: &'static str,
) -> Result<(), Box<dyn Error>> {
    if arguments.len() == expected {
        Ok(())
    } else {
        Err(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_command_is_an_error() {
        let error = run(Vec::<String>::new()).unwrap_err();
        assert_eq!(error.to_string(), "missing command");
    }

    #[test]
    fn malformed_record_number_is_an_error() {
        let error = run([
            "get".to_owned(),
            "records.bin".to_owned(),
            "not-a-number".to_owned(),
        ])
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("not a valid unsigned record number")
        );
    }
}
