use std::{
    path::Path,
    process,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use binsync::{generate_manifest, BasicChunkProvider, Syncer};
use clap::{App, Arg, SubCommand};
use indicatif::{ProgressBar, ProgressStyle};

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

            println!("[1/2] Generating manifest from {}", from);

            let stop_spinner = Arc::new(AtomicBool::new(false));
            let handle = thread::spawn({
                let stop = stop_spinner.clone();
                move || {
                    let spinner = ProgressBar::new(10000);
                    spinner.set_style(
                        ProgressStyle::default_spinner()
                            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
                            .template(
                                "{prefix:.bold.dim} [{elapsed_precise}] {spinner} {wide_msg}",
                            ),
                    );
                    spinner.set_prefix("[1/2]");

                    while !stop.load(Ordering::SeqCst) {
                        spinner.inc(1);
                        thread::sleep(Duration::from_millis(100));
                    }

                    spinner.finish_with_message("Manifest generated.");
                }
            });

            let manifest = match generate_manifest(&from) {
                Ok(manifest) => manifest,
                Err(err) => {
                    eprintln!("Failed to generate manifest: {}", err);
                    process::exit(1);
                }
            };

            stop_spinner.store(true, Ordering::SeqCst);
            handle.join().unwrap();

            let from_path = Path::new(&from);
            let basic_provider = BasicChunkProvider::new(from_path);

            let to_path = Path::new(&to);

            println!("[2/2] Syncing files to {}", to);

            let bar = ProgressBar::new(100);
            bar.set_style(
                ProgressStyle::default_bar()
                    .template(
                        "{prefix:.bold.dim} [{elapsed_precise}] [{wide_bar:.cyan/blue}] ({eta})",
                    )
                    .progress_chars("#>-"),
            );
            bar.set_prefix("[2/2]");

            let mut syncer = Syncer::new(to_path, basic_provider, manifest);
            syncer.on_progress(|amt| bar.set_position(amt.into()));
            if let Err(err) = syncer.sync() {
                eprintln!("Error running sync: {}", err);
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
