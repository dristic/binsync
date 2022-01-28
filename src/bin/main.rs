use std::process;

use binsync::generate_manifest;
use clap::{App, Arg, SubCommand};

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

            if let Err(msg) = binsync::sync(&from, &to) {
                eprintln!("Error running sync: {}", msg);
                process::exit(1);
            }
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
