use clap::Parser;

fn main() {
    let cli = arcl::cli::Cli::parse();

    if let Err(error) = arcl::app::run(cli) {
        let exit_code = error
            .downcast_ref::<arcl::app::ApplicationError>()
            .map_or(1, |application_error| i32::from(application_error.exit_code()));
        eprintln!("error: {error:#}");
        std::process::exit(exit_code);
    }
}
