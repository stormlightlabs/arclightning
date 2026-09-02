use clap::{Parser, error::ErrorKind};

fn main() {
    let json_requested = std::env::args().any(|argument| argument == "--json");
    let cli = match arcl_cli::cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) if matches!(error.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) => error.exit(),
        Err(error) => {
            if json_requested {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "format_version": 1,
                        "error": {
                            "code": "invalid_request",
                            "message": error.to_string(),
                            "exit_code": 2,
                        }
                    })
                );
                std::process::exit(2);
            }
            error.exit();
        }
    };
    let json = cli.json;

    if let Err(error) = arcl_cli::app::run(cli) {
        let exit_code = error
            .downcast_ref::<arcl_cli::app::ApplicationError>()
            .map_or(1, |application_error| i32::from(application_error.exit_code()));
        if json {
            if let Some(application_error) = error.downcast_ref::<arcl_cli::app::ApplicationError>() {
                eprintln!("{}", application_error.json());
            } else {
                eprintln!(
                    "{{\"format_version\":1,\"error\":{{\"code\":\"application_error\",\"message\":{}}}}}",
                    serde_json::to_string(&format!("{error:#}")).unwrap_or_else(|_| "\"application error\"".to_owned())
                );
            }
        } else {
            eprintln!("error: {error:#}");
        }
        std::process::exit(exit_code);
    }
}
