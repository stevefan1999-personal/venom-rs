use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Adds files to myapp
    GenerateShellCode(generate_shellcode::Args),
    Inject(inject::Args),
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::GenerateShellCode(args) => generate_shellcode::run(args),
        Commands::Inject(args) => inject::run(args),
    }
}
