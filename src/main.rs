fn main() {
    if let Err(error) = remux::run(std::env::args()) {
        use std::io::Write;
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "{}", remux::cli::render_error(&error));
        std::process::exit(1);
    }
}
