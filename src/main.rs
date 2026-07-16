use clap::Parser;

fn main() {
    let cli = arcl::cli::Cli::parse();

    if let Err(error) = arcl::app::run(cli) {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
