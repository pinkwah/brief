mod config;
mod guest;
mod host;
// mod install;
mod mapper;
mod util;

use clap::{Parser, Subcommand};

#[macro_export]
macro_rules! config {
    () => {
        (*$crate::config::CONFIG).lock().unwrap()
    };
}

#[macro_export]
macro_rules! mapper {
    () => {
        $crate::mapper::MAPPER.lock().unwrap()
    };
}

pub use util::block_on;

#[derive(Parser, Debug)]
#[command(name = "nixbox")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    StartService,

    Run {
        #[arg(short, long)]
        env: Vec<String>,

        #[arg()]
        program: String,

        #[arg()]
        args: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    use Command::*;
    match cli.command {
        StartService => {
            host::start_server();
        }

        Run { env, program, args } => block_on(async {
            let client = guest::client().await.unwrap();
            client.run(&program, &args).await.unwrap();
        }),
    }
}
