use std::{process, time::Instant};

use binsync::generate_manifest;
use clap::{App, Arg, SubCommand};
use indicatif::ProgressBar;

/// A command-line interface for the crate. Allows you to run various commands
/// without having to compile your own rust program and inspect the outputs.
fn main() {
    let matches = App::new("Binsync")
        .version("1.0")
        .subcommand(
            SubCommand::with_name("sync")
                .arg(Arg::with_name("FROM").required(true))
                .arg(Arg::with_name("TO").required(true)),
        )
        .subcommand(SubCommand::with_name("generate").arg(Arg::with_name("FROM").required(true)))
        .get_matches();

    match matches.subcommand() {
        ("sync", Some(m)) => {
            let from = String::from(m.value_of("FROM").unwrap());
            let to = String::from(m.value_of("TO").unwrap());

            let now = Instant::now();

            let bar = ProgressBar::new(100);

            if let Err(msg) =
                binsync::sync_with_progress(&from, &to, |amt| bar.set_position(amt.into()))
            {
                eprintln!("Error running sync: {}", msg);
                process::exit(1);
            }

            bar.finish();

            println!("Sync completed in {}s", now.elapsed().as_secs());
        }
        ("generate", Some(m)) => {
            let from = String::from(m.value_of("FROM").unwrap());

            match generate_manifest(&from) {
                Ok(manifest) => println!("Generated manifest: {:?}", manifest),
                Err(msg) => {
                    eprintln!("Error running sync: {}", msg);
                    process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("Command not found.");
            process::exit(1);
        }
    }
}
