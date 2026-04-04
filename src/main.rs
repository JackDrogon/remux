fn main() {
    color_eyre::install().expect("color_eyre installation should succeed");

    if let Err(error) = remux::run(std::env::args()) {
        eprintln!("{}", remux::cli::render_error(&error));
        std::process::exit(1);
    }
}
