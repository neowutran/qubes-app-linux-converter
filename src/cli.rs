#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]
mod client_core;
mod common;
use clap::{crate_authors, crate_version, AppSettings, Clap};
use client_core::{convert_all_files, ConvertEvent, ConvertParameters};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    files: Vec<String>,
    #[clap(short, long)]
    in_place: bool,
    #[clap(short, long)]
    archive: Option<String>,
    #[clap(short, long, default_value = "")]
    default_password: String
}
fn main() {
    env_logger::init();
    let opts: Opts = Opts::parse();
    let mut files = opts.files;
    files.dedup();
    let parameters = ConvertParameters {
        in_place: opts.in_place,
        archive: opts.archive,
        files: files.clone(),
        default_password: opts.default_password
    };
    let (transmitter_convert_events, receiver_convert_events) = mpsc::channel();
    thread::spawn(move || {
        convert_all_files(&transmitter_convert_events, &parameters).unwrap();
    });

    for convert_status in receiver_convert_events {
        match convert_status {
            ConvertEvent::FileInfo {
                output_type,
                number_pages,
                file,
            } => println!("{}", number_pages),
            ConvertEvent::PageConverted { file, page } => println!("{}", file),
            ConvertEvent::FileConverted { file } => {
                files.retain(|x| *x != file);
                println!("{}", file);
            }
            ConvertEvent::Failure { file, message } => eprintln!("{}", message),
        }
    }
    println!("The following file convert crashed: {:?}", files);
    // TODO pretty cli/tui display
}
