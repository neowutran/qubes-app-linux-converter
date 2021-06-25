#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]
mod client_core;
mod common;
use clap::{crate_authors, crate_version, AppSettings, Clap};
use client_core::{convert_all_files, ConvertEvent, ConvertParameters};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use glob::glob;
use std::{
    io,
    sync::mpsc::{self},
    thread,
};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Gauge, List, ListItem},
    Terminal,
};
#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
#[clap(setting = AppSettings::ColoredHelp)]
struct Opts {
    files: Vec<String>,
    #[clap(short, long)]
    in_place: bool,
    #[clap(short, long)]
    no_fancy_ui: bool,
    #[clap(short, long)]
    archive: Option<String>,
    #[clap(short, long)]
    default_password: Option<String>,
}
struct FancyTuiData {
    filename: String,
    number_pages: u16,
    current_page: u16,
    output_type: common::OutputType,
    failed: bool,
}
fn main() {
    env_logger::init();
    let opts: Opts = Opts::parse();
    let mut all_files = Vec::new();
    {
        let files = opts.files;
        for file in &files {
            for entry in glob(&format!("{}", file)).expect("Failed to read glob pattern") {
                match entry {
                    Ok(path) => all_files.push(path.to_str().unwrap().to_string()),
                    Err(e) => {
                        eprintln!("{:?}", e);
                        panic!("Unable to parse the list of files to convert");
                    }
                }
            }
        }
    }
    all_files.dedup();
    println!("{:?}", all_files);
    if all_files.is_empty() {
        eprintln!("You provided no files to convert");
        return;
    }
    let parameters = ConvertParameters {
        in_place: opts.in_place,
        archive: opts.archive,
        files: all_files.clone(),
        default_password: opts.default_password.unwrap_or(String::new()),
    };
    let (transmitter_convert_events, receiver_convert_events) = mpsc::channel();
    thread::spawn(move || {
        convert_all_files(&transmitter_convert_events, &parameters).unwrap();
    });

    if opts.no_fancy_ui {
        for convert_status in receiver_convert_events {
            match convert_status {
                ConvertEvent::FileInfo {
                    output_type,
                    number_pages,
                    file,
                } => println!(
                    "{}: {} pages, output will be {}",
                    file,
                    number_pages,
                    output_type.extension()
                ),
                ConvertEvent::PageConverted { file, page } => {
                    println!("{}: converted page n°{}", file, page)
                }
                ConvertEvent::FileConverted { file } => {
                    all_files.retain(|x| *x != file);
                    println!("converted file {}", file);
                }
                ConvertEvent::Failure { file: _, message } => eprintln!("{}", message),
            }
        }
        println!("The following file convert crashed: {:?}", all_files);
    } else {
        let mut tui_data = Vec::new();
        let number_of_files = all_files.len();
        let mut tui_initialized = false;
        let mut stdout = io::stdout();
        //execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
        execute!(stdout, EnterAlternateScreen).unwrap();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).unwrap();
        for convert_status in receiver_convert_events {
            if !tui_initialized {
                tui_initialized = true;
                enable_raw_mode().unwrap();
                terminal.clear().unwrap();
                println!("TOTO");
            }
            match convert_status {
                ConvertEvent::FileInfo {
                    output_type,
                    number_pages,
                    file,
                } => tui_data.push(FancyTuiData {
                    filename: file,
                    number_pages,
                    output_type,
                    current_page: 0,
                    failed: false,
                }),
                ConvertEvent::PageConverted { file, page } => {
                    for data in tui_data.iter_mut() {
                        if data.filename == file {
                            data.current_page = page;
                        }
                    }
                }
                ConvertEvent::FileConverted { file } => {
                    for data in tui_data.iter_mut() {
                        if data.filename == file {
                            data.current_page = data.number_pages;
                        }
                    }
                    all_files.retain(|x| *x != file);
                }
                ConvertEvent::Failure { file, message: _ } => {
                    for data in tui_data.iter_mut() {
                        if data.filename == file {
                            data.failed = true;
                        }
                    }
                }
            }
            terminal
                .draw(|f| {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints(vec![Constraint::Percentage(10); number_of_files].as_ref())
                        .split(f.size());
                    for (chunk, data) in tui_data.iter().enumerate() {
                        let color = if data.failed {
                            Color::Red
                        } else {
                            Color::Green
                        };
                        let gauge = Gauge::default()
                            .block(
                                Block::default()
                                    .title(data.filename.to_string())
                                    .borders(Borders::ALL),
                            )
                            .gauge_style(Style::default().fg(color))
                            .percent(&data.current_page * 100 / data.number_pages);
                        f.render_widget(gauge, chunks[chunk]);
                    }
                })
                .unwrap();
        }
    }
    disable_raw_mode().unwrap();
    println!("The following file convert crashed: {:?}", all_files);
    // TODO pretty cli/tui display
}
