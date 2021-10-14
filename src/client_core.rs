#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]
use crate::common::OutputType;
use log::debug;
use std::{
    collections::HashMap,
    convert::{TryFrom, TryInto},
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    process::{Child, ChildStdout, Command, Stdio},
    sync::mpsc::{channel, Sender},
    thread, time,
};
use uuid::Uuid;

#[cfg(test)]
use glob::glob;

const MAX_PAGES: u16 = 10_000;
const MAX_IMG_WIDTH: usize = 10_000;
const MAX_IMG_HEIGHT: usize = 10_000;
const MAX_IMG_SIZE: usize = MAX_IMG_WIDTH * MAX_IMG_HEIGHT * 4;

#[cfg(not(test))]
const QREXEC_BINARY: &str = "/usr/bin/qrexec-client-vm";

#[cfg(test)]
const QREXEC_BINARY: &str = "target/release/qubes-converter-server";

#[test]
fn convert_all_in_one_integration_test() {
    let _ = env_logger::builder().is_test(true).try_init();
    let mut files_that_must_exist = Vec::new();
    let temporary_directory = format!("/home/user/.temp_qubes_convert_{}", Uuid::new_v4());
    fs::create_dir_all(&temporary_directory).unwrap();
    let mut files = Vec::new();
    for entry in glob("tests/files/*").expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                println!("{:?}", path.display());
                let file_base_name = path.file_stem().unwrap().to_str().unwrap();
                let file_extension = path.extension().unwrap().to_str().unwrap();
                let filename = format!(
                    "{}/{}.{}",
                    &temporary_directory, &file_base_name, &file_extension
                );
                fs::copy(&path, &filename).unwrap();
                files.push(filename);
                let mimetype: mime::Mime = tree_magic::from_filepath(&path)
                    .parse()
                    .expect("Incorrect detection of mimetype");
                let mut expected_output_filename =
                    format!("{}/{}.trusted.", &temporary_directory, &file_base_name);
                expected_output_filename.push_str(match (mimetype.type_(), mimetype.subtype()) {
                    (mime::AUDIO, _) => panic!("Audio convert not implemented"),
                    (mime::VIDEO, _) => panic!("Video convert not implemented"),
                    (mime::IMAGE, _) => "png",
                    _ => "pdf",
                });
                match fs::remove_file(&expected_output_filename) {
                    Ok(_) => panic!("Converted file already exist before beginning of the tests !"),
                    Err(_) => {}
                }
                files_that_must_exist.push((
                    expected_output_filename.to_string(),
                    format!("{}.{}", &file_base_name, file_extension),
                ));
            }
            Err(_e) => panic!("glob error"),
        }
    }
    let parameters = ConvertParameters {
        in_place: false,
        archive: Some(format!("{}/", temporary_directory)),
        files,
        default_password: "toor".to_string(),
        max_pages_converted_in_parallele: 1,
        ocr: None,
        stderr: true,
    };
    let (transmitter_convert_events, _receiver_convert_events) = channel();
    convert_all_files(&transmitter_convert_events, parameters).unwrap();
    for file_that_must_exist in files_that_must_exist {
        assert_eq!(true, std::path::Path::new(&file_that_must_exist.0).exists());
        fs::remove_file(&file_that_must_exist.0).unwrap();
    }
    fs::remove_dir_all(&temporary_directory).unwrap();
}

#[test]
fn convert_one_by_one_integration_test() {
    let _ = env_logger::builder().is_test(true).try_init();
    let temporary_directory = format!("/home/user/.temp_qubes_convert_{}", Uuid::new_v4());
    fs::create_dir_all(&temporary_directory).unwrap();
    for entry in glob("tests/files/*").expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                println!("{:?}", path.display());
                let file_base_name = path.file_stem().unwrap().to_str().unwrap();
                let file_extension = path.extension().unwrap().to_str().unwrap();
                fs::copy(
                    &path,
                    &format!(
                        "{}/{}.{}",
                        &temporary_directory, &file_base_name, &file_extension
                    ),
                )
                .unwrap();
                let mimetype: mime::Mime = tree_magic::from_filepath(&path)
                    .parse()
                    .expect("Incorrect detection of mimetype");
                let mut expected_output_filename =
                    format!("{}/{}.trusted.", &temporary_directory, &file_base_name);
                expected_output_filename.push_str(match (mimetype.type_(), mimetype.subtype()) {
                    (mime::AUDIO, _) => panic!("Audio convert not implemented"),
                    (mime::VIDEO, _) => panic!("Video convert not implemented"),
                    (mime::IMAGE, _) => "png",
                    _ => "pdf",
                });
                match fs::remove_file(&expected_output_filename) {
                    Ok(_) => panic!("Converted file already exist before beginning of the tests !"),
                    Err(_) => {}
                }
                let parameters = ConvertParameters {
                    in_place: false,
                    archive: Some(format!("{}/", temporary_directory)),
                    files: vec![format!(
                        "{}/{}.{}",
                        &temporary_directory, &file_base_name, &file_extension
                    )],
                    default_password: "toor".to_string(),
                    max_pages_converted_in_parallele: 1,
                    ocr: None,
                    stderr: true,
                };
                let (transmitter_convert_events, _receiver_convert_events) = channel();
                convert_all_files(&transmitter_convert_events, parameters).unwrap();
                assert_eq!(
                    true,
                    std::path::Path::new(&expected_output_filename).exists()
                );
                fs::remove_file(&expected_output_filename).unwrap();
            }
            Err(_e) => panic!("glob error"),
        }
    }
    fs::remove_dir_all(&temporary_directory).unwrap();
}

#[test]
fn convert_one_big_integration_test() {
    let _ = env_logger::builder().is_test(true).try_init();
    let temporary_directory = format!("/home/user/.temp_qubes_convert_{}", Uuid::new_v4());
    fs::create_dir_all(&temporary_directory).unwrap();
    let file = "IPCC_AR6_WGI_Full_Report.pdf";
    let path = format!("tests/files/{}", file);
    fs::copy(&path, &format!("{}/{}", &temporary_directory, &file)).unwrap();
    let mimetype: mime::Mime = tree_magic::from_filepath(std::path::Path::new(&path))
        .parse()
        .expect("Incorrect detection of mimetype");
    let mut expected_output_filename =
        format!("{}/IPCC_AR6_WGI_Full_Report.trusted.", &temporary_directory);
    expected_output_filename.push_str(match (mimetype.type_(), mimetype.subtype()) {
        (mime::AUDIO, _) => panic!("Audio convert not implemented"),
        (mime::VIDEO, _) => panic!("Video convert not implemented"),
        (mime::IMAGE, _) => "png",
        _ => "pdf",
    });
    match fs::remove_file(&expected_output_filename) {
        Ok(_) => panic!("Converted file already exist before beginning of the tests !"),
        Err(_) => {}
    }
    let parameters = ConvertParameters {
        in_place: false,
        archive: Some(format!("{}/", temporary_directory)),
        files: vec![format!("{}/{}", &temporary_directory, &file)],
        default_password: "toor".to_string(),
        max_pages_converted_in_parallele: 4,
        ocr: None,
        stderr: true,
    };
    let (transmitter_convert_events, _receiver_convert_events) = channel();
    convert_all_files(&transmitter_convert_events, parameters).unwrap();
    assert_eq!(
        true,
        std::path::Path::new(&expected_output_filename).exists()
    );
    fs::remove_file(&expected_output_filename).unwrap();
    fs::remove_dir_all(&temporary_directory).unwrap();
}

impl OutputType {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Image => "png",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConvertParameters {
    pub files: Vec<String>,
    pub in_place: bool,
    pub archive: Option<String>,
    pub default_password: String,
    pub max_pages_converted_in_parallele: u8,
    pub ocr: Option<String>,
    pub stderr: bool,
}
#[derive(Debug)]
pub enum ConvertEvent {
    FileToConvert {
        file: String,
    },
    FileInfo {
        output_type: OutputType,
        number_pages: u16,
        file: String,
    },
    PageConverted {
        file: String,
        page: u16,
    },
    FileConverted {
        file: String,
    },
    Failure {
        file: String,
        message: String,
    },
}
pub fn default_archive_folder() -> String {
    format!(
        "{}/QubesUntrusted/",
        home::home_dir().unwrap().to_str().unwrap()
    )
}
fn convert_one_page(
    mpsc_sender: &Sender<ConvertEvent>,
    source_file: &str,
    process_stdout: &mut ChildStdout,
    temporary_file_base_page: &str,
    output_type: OutputType,
    ocr: &Option<String>,
) -> Result<(String, Option<Child>), Box<dyn std::error::Error>> {
    debug!("reading size and output type from server");
    let mut buffer_size = [0; 2 + 2];
    process_stdout.read_exact(&mut buffer_size)?;
    let width_raw = buffer_size[..2].try_into().unwrap();
    let height_raw = buffer_size[2..4].try_into().unwrap();
    let width = u32::from(u16::from_le_bytes(width_raw));
    let height = u32::from(u16::from_le_bytes(height_raw));
    if height as usize > MAX_IMG_HEIGHT
        || width as usize > MAX_IMG_WIDTH
        || width as usize * height as usize * 4 > MAX_IMG_SIZE
    {
        let failure_message = "Max image size exceeded: Probably DOS attempt";
        mpsc_sender.send(ConvertEvent::Failure {
            file: source_file.to_string(),
            message: failure_message.to_string(),
        })?;
        panic!("{}", failure_message);
    }

    debug!("reading page data from server");
    let mut buffer_page = vec![0; (height * width * 4) as usize];
    process_stdout.read_exact(&mut buffer_page)?;

    let png_file_path = format!("{}.png", temporary_file_base_page);
    let image = image::RgbaImage::from_raw(width, height, buffer_page).unwrap();
    image.save(&png_file_path).unwrap();

    match output_type {
        OutputType::Pdf => {
            let pdf_file_path = format!("{}.pdf", temporary_file_base_page);
            let mut process_name = "gm";
            let mut process_args = vec!["convert", &png_file_path, &pdf_file_path];
            if let Some(ocr_lang) = ocr {
                process_name = "tesseract";
                process_args = vec![
                    &png_file_path,
                    temporary_file_base_page,
                    "-l",
                    ocr_lang,
                    "--dpi",
                    "70",
                    "pdf",
                ];
            } else {
            }
            let convert_to_pdf_process = Command::new(process_name)
                .args(&process_args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Unable to launch the 'convert to pdf' process (tesseract/gm)");
            Ok((pdf_file_path, Some(convert_to_pdf_process)))
        }
        OutputType::Image => Ok((png_file_path, None)),
    }
}
pub fn list_ocr_langs() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let command_output = Command::new("tesseract")
        .arg("--list-langs")
        .output()
        .expect(
            "Unable to list languages supported by tesseract. Tesseract is probably not installed.",
        );
    let mut result = Vec::new();
    let stdout = String::from_utf8(command_output.stdout)?;
    let mut header = true;
    for line in stdout.lines() {
        if header {
            header = false;
            continue;
        }
        result.push(line.to_string());
    }
    Ok(result)
}

fn convert_all_pages(
    mpsc_sender: &Sender<ConvertEvent>,
    process_stdout: &mut ChildStdout,
    source_file: &str,
    temporary_directory: &str,
    parameters: &ConvertParameters,
    output_type: OutputType,
    number_pages: u16,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut output_pages = Vec::new();
    let mut all_pages_convert_process: HashMap<u16, (String, Option<Child>)> = HashMap::new();

    // Tesseract process require gigantic amount of memory.
    // Memory starving tesseract process will slow down everything and result in much MUCH worse
    // performance (freezing, some kind of deadlock and crashing included).
    // So the optimal amount of concurrent tesseract seems to be a computation between number of
    // CPU physical core available and memory available.
    //
    // On my particular setup (highend gaming setup from late 2017, 8 physical core and 32 go ram)
    // , "3" seems to be the best number for the fastest conversion.
    // This number will vary depending on the hardware.
    // In case of doubt, less tesseract process is better than more tesseract process.
    let maximum_number_process = parameters.max_pages_converted_in_parallele;

    for page in 0..number_pages {
        while all_pages_convert_process.len() >= maximum_number_process.into() {
            for page_id in all_pages_convert_process
                .keys()
                .copied()
                .collect::<Vec<u16>>()
            {
                let mut page_convert_process = all_pages_convert_process.remove(&page_id).unwrap();
                let page_path = page_convert_process.0.to_string();
                let mut page_converted = true;
                if let Some(ref mut process) = page_convert_process.1 {
                    if process.try_wait().expect("'try_wait' failed").is_none() {
                        all_pages_convert_process.insert(page_id, page_convert_process);
                        page_converted = false;
                    }
                }
                if page_converted {
                    debug!("Sending page converted information");
                    output_pages.push(page_path);
                    mpsc_sender.send(ConvertEvent::PageConverted {
                        file: source_file.to_string(),
                        page: page_id,
                    })?;
                }
            }
            if all_pages_convert_process.keys().len() >= maximum_number_process.into() {
                debug!("sleeping");
                let sleep_time = time::Duration::from_millis(200);
                thread::sleep(sleep_time);
            }
        }
        let temporary_file_base_page = format!("{}.{}", temporary_directory, page);
        let converted_page = convert_one_page(
            mpsc_sender,
            source_file,
            process_stdout,
            &temporary_file_base_page,
            output_type,
            &parameters.ocr,
        )?;
        all_pages_convert_process.insert(page, converted_page);
    }
    for (page_id, page_convert_process) in all_pages_convert_process {
        if let Some(mut process) = page_convert_process.1 {
            process.wait()?;
        }
        output_pages.push(page_convert_process.0.to_string());
        mpsc_sender.send(ConvertEvent::PageConverted {
            file: source_file.to_string(),
            page: page_id,
        })?;
    }
    Ok(output_pages)
}

fn convert_one_file(
    mpsc_sender: &Sender<ConvertEvent>,
    process_stdout: &mut ChildStdout,
    source_file: &str,
    temporary_directory: &str,
    parameters: &ConvertParameters,
    archive_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!("BEGIN CONVERT ONE FILE: {}", source_file);
    let mut buffer_pages_and_type = vec![0_u8; 2 + 1];
    process_stdout.read_exact(&mut buffer_pages_and_type)?;
    let number_pages_raw = buffer_pages_and_type[..2].try_into()?;
    let number_pages = u16::from_le_bytes(number_pages_raw);
    if number_pages > MAX_PAGES {
        debug!("Number of page sended by the server: {}", number_pages);
        let failure_message = "Max page number exceeded: Probably DOS attempt";
        mpsc_sender.send(ConvertEvent::Failure {
            file: source_file.to_string(),
            message: failure_message.to_string(),
        })?;
        panic!("{}", failure_message);
    }
    let source_file_path = fs::canonicalize(source_file)?;
    let source_file_basename = source_file_path.file_stem().unwrap().to_str().unwrap();
    let empty_extension = OsString::new();
    let source_file_extension = source_file_path
        .extension()
        .unwrap_or(&empty_extension)
        .to_str()
        .unwrap();
    let source_directory = source_file_path.parent().unwrap().to_str().unwrap();
    let mut output_file = format!("{}/{}.trusted.", source_directory, source_file_basename);
    let output_type = OutputType::try_from(*buffer_pages_and_type.get(2).unwrap())?;
    output_file.push_str(output_type.extension());
    if output_type == OutputType::Image && number_pages != 1 {
        let failure_message = "Image can only be 1 page. Abording.";
        mpsc_sender.send(ConvertEvent::Failure {
            file: source_file.to_string(),
            message: failure_message.to_string(),
        })?;
        panic!("{}", failure_message);
    }
    mpsc_sender.send(ConvertEvent::FileInfo {
        file: source_file.to_string(),
        output_type,
        number_pages,
    })?;

    let output_pages = convert_all_pages(
        mpsc_sender,
        process_stdout,
        source_file,
        temporary_directory,
        parameters,
        output_type,
        number_pages,
    )?;

    debug!("CONVERTED ALL PAGES");
    match output_type {
        OutputType::Image => {
            fs::copy(output_pages.get(0).unwrap(), output_file)?;
        }
        OutputType::Pdf => {
            let mut pdftk_args = output_pages;
            pdftk_args.push("cat".to_string());
            pdftk_args.push("output".to_string());
            pdftk_args.push(output_file);
            let command_output = Command::new("pdftk")
                .args(&pdftk_args)
                .output()
                .expect("Unable to launch pdftk process");
            if !command_output.status.success() {
                let failure_message =
                    "pdftk failed. Probable cause is 'out of space'. Check with 'df -h'";
                mpsc_sender.send(ConvertEvent::Failure {
                    file: source_file.to_string(),
                    message: failure_message.to_string(),
                })?;
                panic!("{}", failure_message);
            }
        }
    }
    mpsc_sender.send(ConvertEvent::FileConverted {
        file: source_file.to_string(),
    })?;
    if parameters.in_place {
        fs::remove_file(source_file_path)?;
    } else {
        let archive_file = format!(
            "{}{}.{}",
            &archive_path, &source_file_basename, &source_file_extension
        );
        debug!("archiving {:?} to {}", source_file_path, &archive_file);
        fs::copy(&source_file_path, &archive_file)?;
        fs::remove_file(&source_file_path)?;
    }
    debug!("END CONVERT ONE FILE: {}", source_file);
    Ok(())
}
pub fn convert_all_files(
    message_for_ui_emetter: &Sender<ConvertEvent>,
    mut parameters: ConvertParameters,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut server_process = Command::new(QREXEC_BINARY);
    server_process
        .args(&["@dispvm", "qubes.Convert"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    if !parameters.stderr {
        server_process.stderr(Stdio::null());
    }
    let mut server_process = server_process
        .spawn()
        .expect("Convert server failed to start");
    parameters.max_pages_converted_in_parallele = if parameters.ocr.is_some() {parameters.max_pages_converted_in_parallele}else{(num_cpus::get()).try_into().unwrap()};
    debug!("{:?}", parameters);

    // We don't use the "/tmp/" directory since it's size is limited and not easily configurable.
    // Example: impossible to convert a GIEC report in the 1go /tmp/ fs.
    let temporary_directory = format!("/home/user/.temp_qubes_convert_{}", Uuid::new_v4());
    fs::create_dir_all(&temporary_directory)?;
    let archive_path = match &parameters.archive {
        Some(path) => format!("{}/", fs::canonicalize(path).unwrap().to_str().unwrap()),
        None => default_archive_folder(),
    };
    fs::create_dir_all(&archive_path)?;
    let mut server_process_stdin = server_process.stdin.take().unwrap();
    let mut server_process_stdout = server_process.stdout.as_mut().unwrap();

    server_process_stdin.write_all(format!("{}\n", parameters.default_password).as_bytes())?;
    server_process_stdin.write_all(format!("{}\n", parameters.files.len()).as_bytes())?;
    let (tx, rx) = channel();
    let temporary_directory_clone = temporary_directory.clone();
    let files = parameters.files.clone();
    let message_for_ui_emetter_clone = message_for_ui_emetter.clone();
    thread::spawn(move || {
        for (file_id, filename) in files.into_iter().enumerate() {
            message_for_ui_emetter_clone
                .send(ConvertEvent::FileToConvert {
                    file: filename.to_string(),
                })
                .unwrap();
            debug!("Transmitting file {} to server", filename);
            let temporary_directory_file = format!("{}/{}", &temporary_directory_clone, file_id);
            fs::create_dir_all(&temporary_directory_file).unwrap();
            let mut buffer = Vec::new();
            let mut file = File::open(&filename).unwrap();
            file.read_to_end(&mut buffer).unwrap();
            server_process_stdin
                .write_all(format!("{}\n", buffer.len()).as_bytes())
                .unwrap();
            server_process_stdin.write_all(&buffer).unwrap();
            debug!("File {} have been transmitted to the server", filename);
            tx.send((filename, temporary_directory_file)).unwrap();
        }
    });
    for file_info in rx {
        let filename = file_info.0;
        let temporary_directory_file = file_info.1;
        if let Err(e) = convert_one_file(
            message_for_ui_emetter,
            &mut server_process_stdout,
            &filename,
            &temporary_directory_file,
            &parameters,
            &archive_path,
        ) {
            message_for_ui_emetter.send(ConvertEvent::Failure {
                file: filename.to_string(),
                message: format!("{:?}", e),
            })?;
        }
        fs::remove_dir_all(&temporary_directory_file).unwrap();
    }
    fs::remove_dir_all(&temporary_directory).unwrap();
    Ok(())
}
