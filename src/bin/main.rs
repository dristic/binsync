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

use binsync::{generate_manifest, CachingChunkProvider, Syncer};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Sync { from: String, to: String },
    Generate { from: String },
}

/// A command-line interface for the crate. Allows you to run various commands
/// without having to compile your own rust program and inspect the outputs.
fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Sync { from, to } => {
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
            let provider = CachingChunkProvider::new(from_path);

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

            let mut syncer = Syncer::new(to_path, provider, manifest);
            syncer.on_progress(|amt| bar.set_position(amt.into()));
            if let Err(err) = syncer.sync() {
                eprintln!("Error running sync: {}", err);
                process::exit(1);
            }

            bar.finish();

            println!("Sync completed in {}s", now.elapsed().as_secs());
        }
        Commands::Generate { from } => {
            let now = Instant::now();

            println!("[1/1] Generating manifest from {}", from);

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

            match generate_manifest(&from) {
                Ok(manifest) => println!("Generated manifest: {:?}", manifest),
                Err(msg) => {
                    eprintln!("Error running sync: {}", msg);
                    process::exit(1);
                }
            }

            stop_spinner.store(true, Ordering::SeqCst);
            handle.join().unwrap();

            println!("Manifest generated in {}s", now.elapsed().as_secs());
        }
    }
}
