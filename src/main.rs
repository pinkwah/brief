mod config;
mod guest;
mod host;
mod install;
mod util;

use clap::{Parser, Subcommand};
use config::Config;

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

    Install,

    Run {
        #[arg(short, long)]
        env: Vec<String>,

        #[arg()]
        rest: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    use Command::*;
    match cli.command {
        StartService => {
            let config = Config::from_file_or_default();
            host::start_server(&config);
        }

        Install => {
            install::install();
        }

        Run { env, .. } => {
            println!("> {:?}", env);
        }
    }

    // let config = Config::from_file_or_default();
    // // ensure_server(config);

    // tokio::runtime::Builder::new_current_thread()
    //     .enable_all()
    //     .build()
    //     .unwrap()
    //     .block_on(async {
    //         let client = host::client().await.unwrap();
    //         println!("Return: {:?}", client.run("Foobar").await);

    //         let client = guest::client().await.unwrap();
    //         println!("Return: {:?}", client.run("ls", &["/"]).await);
    //     });
}
