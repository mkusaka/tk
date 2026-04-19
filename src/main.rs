mod app;
mod cli;
mod error;
mod model;
mod output;
mod storage;
mod watch;

use clap::Parser;
use cli::{Cli, OutputFormat};
use output::print_error;

fn main() {
    let cli = Cli::parse();
    let fallback_format = cli.format.unwrap_or(OutputFormat::Text);
    let exit_code = match app::run(cli) {
        Ok(code) => code,
        Err(err) => {
            print_error(&err, fallback_format);
            err.code.exit_code()
        }
    };
    std::process::exit(exit_code);
}
