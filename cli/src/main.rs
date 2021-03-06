#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]
use clap::{AppSettings, Parser};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use glob::glob;
use qubes_converter_client;
use qubes_converter_client::{convert_all_files, list_ocr_langs, ConvertEvent, ConvertParameters};
use qubes_converter_common;
use std::{
    convert::TryInto,
    io,
    sync::mpsc::{self, Receiver},
    thread,
};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Gauge},
    Terminal,
};
#[derive(Parser)]
#[clap(version, about, author)]
#[clap(setting = AppSettings::ArgRequiredElseHelp)]
struct Opts {
    #[clap(required = true)]
    files: Vec<String>,

    #[clap(short, long)]
    in_place: bool,

    #[clap(short, long)]
    no_fancy_ui: bool,

    #[clap(short, long)]
    archive: Option<String>,

    #[clap(short, long)]
    default_password: Option<String>,

    #[clap(
        short,
        long,
        help = "WARNING: using this option increase the attack surface. Example: if there is a exploitable bug in tesseract, this software won't protect you."
    )]
    ocr_lang: Option<String>,

    #[clap(short, long)]
    list_ocr_langs: bool,

    #[clap(short, long, default_value = "1")]
    max_tesseract_process: u8,
}
struct FancyTuiData {
    filename: String,
    number_pages: u16,
    current_page: u16,
    output_type: Option<qubes_converter_common::OutputType>,
    failed: bool,
    started: bool,
}
fn once_correct_file_found(
    file: &str,
    tui_data: &mut Vec<FancyTuiData>,
    funct: &mut impl FnMut(usize, &mut FancyTuiData),
) {
    for (id, data) in tui_data.iter_mut().enumerate() {
        if data.filename == file {
            funct(id, data);
            return;
        }
    }
}
fn fancy_ui_main_loop(
    receiver_convert_events: Receiver<ConvertEvent>,
    all_files: &mut Vec<String>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) {
    let mut tui_data = Vec::new();
    let number_of_files = all_files.len();
    for convert_status in receiver_convert_events {
        match convert_status {
            ConvertEvent::FileToConvert { file } => tui_data.push(FancyTuiData {
                filename: file.to_string(),
                number_pages: 0,
                current_page: 0,
                output_type: None,
                failed: false,
                started: false,
            }),
            ConvertEvent::FileInfo {
                output_type,
                number_pages,
                file,
            } => once_correct_file_found(&file, &mut tui_data, &mut |_, data| {
                data.output_type = Some(output_type);
                data.number_pages = number_pages;
                data.started = true;
            }),
            ConvertEvent::PageConverted { file, .. } => {
                once_correct_file_found(&file, &mut tui_data, &mut |_, data| {
                    data.current_page += 1;
                });
            }
            ConvertEvent::FileConverted { file } => {
                let mut to_remove_id = 0;
                once_correct_file_found(&file, &mut tui_data, &mut |id, _data| {
                    to_remove_id = id;
                });
                tui_data.remove(to_remove_id);
                all_files.retain(|x| *x != file);
            }
            ConvertEvent::Failure { file, message } => {
                once_correct_file_found(&file, &mut tui_data, &mut |_, data| {
                    data.failed = true;
                });
                eprintln!("{}: Failure, {}", file, message);
            }
        }
        terminal
            .draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(vec![Constraint::Percentage(10); number_of_files].as_ref())
                    .split(f.size());
                for (chunk, data) in tui_data.iter().enumerate() {
                    let color = if data.started {
                        if data.failed {
                            Color::Red
                        } else {
                            Color::Green
                        }
                    } else {
                        Color::Blue
                    };
                    let percent: u32 = if data.started {
                        u32::from(data.current_page) * 100 / u32::from(data.number_pages)
                    } else {
                        0
                    };
                    let gauge = Gauge::default()
                        .block(
                            Block::default()
                                .title(format!(
                                    "{} ({}/{})",
                                    data.filename, data.current_page, data.number_pages
                                ))
                                .borders(Borders::ALL),
                        )
                        .gauge_style(Style::default().fg(color))
                        .percent(percent.try_into().unwrap());
                    f.render_widget(gauge, chunks[chunk]);
                }
            })
            .unwrap();
    }
}
fn fancy_ui(receiver_convert_events: Receiver<ConvertEvent>, all_files: &mut Vec<String>) {
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).unwrap();
    let backend = CrosstermBackend::new(stdout);

    enable_raw_mode().unwrap();
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.clear().unwrap();
    fancy_ui_main_loop(receiver_convert_events, all_files, &mut terminal);

    disable_raw_mode().unwrap();
    execute!(terminal.backend_mut(), LeaveAlternateScreen,).unwrap();
    terminal.show_cursor().unwrap();
}
fn non_fancy_ui(receiver_convert_events: Receiver<ConvertEvent>, all_files: &mut Vec<String>) {
    for convert_status in receiver_convert_events {
        match convert_status {
            ConvertEvent::FileToConvert { file } => {
                println!("Sending to server {} for conversion ", file);
            }
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
                println!("{}: converted page n\u{b0}{}", file, page);
            }
            ConvertEvent::FileConverted { file } => {
                all_files.retain(|x| *x != file);
                println!("converted file {}", file);
            }
            ConvertEvent::Failure { file: _, message } => eprintln!("{}", message),
        }
    }
}
fn main() {
    env_logger::init();
    let opts: Opts = Opts::parse();
    if opts.list_ocr_langs {
        let langs = list_ocr_langs().unwrap();
        println!("List of language supported by your tesseract installation: ");
        for lang in langs {
            println!("{}", lang);
        }
        return;
    }
    let mut all_files = Vec::new();
    {
        let files = opts.files;
        for file in &files {
            for entry in glob(file).expect("Failed to read glob pattern") {
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
    let parameters = ConvertParameters {
        in_place: opts.in_place,
        archive: opts.archive,
        files: all_files.clone(),
        default_password: opts.default_password.unwrap_or_default(),
        max_pages_converted_in_parallele: opts.max_tesseract_process,
        ocr: opts.ocr_lang,
        stderr: opts.no_fancy_ui,
    };
    let (transmitter_convert_events, receiver_convert_events) = mpsc::channel();
    thread::spawn(move || {
        convert_all_files(&transmitter_convert_events, parameters).unwrap();
    });

    if opts.no_fancy_ui {
        non_fancy_ui(receiver_convert_events, &mut all_files);
    } else {
        fancy_ui(receiver_convert_events, &mut all_files);
    }
    if all_files.is_empty() {
        println!("All files have been successfully converted");
    } else {
        eprintln!("The following file convert crashed: {:?}", all_files);
    }
}
