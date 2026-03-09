use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

mod account;
mod engine;
mod transaction;

fn main() {
    let file_path = read_file_path();
    let reader = open_file(&file_path);
    let accounts = engine::process_transactions(reader);
    engine::output_statement(&accounts);
}

fn read_file_path() -> PathBuf {
    let args: Vec<String> = env::args().collect();
    match args.get(1) {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("Usage: cargo run -- <transactions.csv>");
            std::process::exit(1);
        }
    }
}

fn open_file(file_path: &PathBuf) -> BufReader<File> {
    let file = match File::open(file_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open file {}: {}", file_path.display(), e);
            std::process::exit(1);
        }
    };

    BufReader::new(file)
}
