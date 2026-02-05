fn main() {
    if let Err(e) = chaosfilter_cli::entry(std::env::args_os()) {
        eprintln!("{:#}", e);
        std::process::exit(1);
    }
}
