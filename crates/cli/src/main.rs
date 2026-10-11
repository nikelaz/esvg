use clap::Parser;
use esvg_cli::cli::Cli;
use esvg_cli::run;

fn main() {
    let cli = Cli::parse();
    std::process::exit(run::execute(cli.command).into());
}
