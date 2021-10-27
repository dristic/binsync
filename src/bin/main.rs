use std::process;

use binsync::Opts;
use clap::{App, Arg, SubCommand};

fn main() {
    let matches = App::new("Binsync")
        .version("1.0")
        .subcommand(SubCommand::with_name("server"))
        .subcommand(
            SubCommand::with_name("sync")
                .arg(Arg::with_name("FROM").required(true))
                .arg(Arg::with_name("TO").required(true)),
        )
        .subcommand(SubCommand::with_name("test"))
        .subcommand(SubCommand::with_name("client"))
        .get_matches();

    match matches.subcommand() {
        ("sync", Some(m)) => {
            let opts = Opts {
                from: String::from(m.value_of("FROM").unwrap()),
                to: String::from(m.value_of("TO").unwrap()),
            };

            if let Err(msg) = binsync::sync(opts) {
                eprintln!("Error running sync: {}", msg);
                process::exit(1);
            }
        }
        _ => {
            eprintln!("Command not found.");
            process::exit(1);
        }
    }
}
