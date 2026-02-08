use std::process;
use chaosfilter_cli::entry;

fn main() {
    if let Err(e) = entry(std::env::args_os()) {
        eprintln!("{:#}", e);
        process::exit(1);
    }
}
