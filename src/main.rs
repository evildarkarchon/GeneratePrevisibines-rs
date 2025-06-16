use clap::Parser;
use env_logger;
use log::error;

use generate_previsbines::{Args, PrevisbineBuilder};

fn main() {
    // Initialize logger
    env_logger::init();

    // Parse command line arguments
    let args = Args::parse();

    // Create and run the builder
    match PrevisbineBuilder::new(args) {
        Ok(mut builder) => {
            if let Err(e) = builder.run() {
                error!("{}", e);
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            error!("{}", e);
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}