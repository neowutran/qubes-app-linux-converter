#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]
use crate::common::{strict_process_execute, OutputType};
use log::debug;
use std::{
    convert::TryInto,
    env::temp_dir,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    process::{ChildStdout, Command, Stdio},
    sync::mpsc::Sender,
};
use uuid::Uuid;

#[cfg(test)]
use glob::glob;
#[cfg(test)]
use std::sync::mpsc;

const MAX_PAGES: u16 = 10_000;
const MAX_IMG_WIDTH: usize = 10_000;
const MAX_IMG_HEIGHT: usize = 10_000;
const MAX_IMG_SIZE: usize = MAX_IMG_WIDTH * MAX_IMG_HEIGHT * 4;

#[cfg(not(test))]
const QREXEC_BINARY: &str = "/usr/bin/qrexec-client-vm";

#[cfg(test)]
const QREXEC_BINARY: &str = "target/debug/qubes-app-linux-converter-server";

#[test]
fn convert_all_in_one_integration_test() {
    let _ = env_logger::builder().is_test(true).try_init();
    let mut files_that_must_exist = Vec::new();
    let temporary_directory = format!(
        "{}/qubes_convert_{}",
        temp_dir().to_str().unwrap(),
        Uuid::new_v4()
    );
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
    };
    let (transmitter_convert_events, _receiver_convert_events) = mpsc::channel();
    convert_all_files(&transmitter_convert_events, &parameters).unwrap();
    for file_that_must_exist in files_that_must_exist {
        assert_eq!(true, std::path::Path::new(&file_that_must_exist.0).exists());
        fs::remove_file(&file_that_must_exist.0).unwrap();
    }
}

#[test]
fn convert_one_by_one_integration_test() {
    let _ = env_logger::builder().is_test(true).try_init();
    let temporary_directory = format!(
        "{}/qubes_convert_{}",
        temp_dir().to_str().unwrap(),
        Uuid::new_v4()
    );
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
                };
                let (transmitter_convert_events, _receiver_convert_events) = mpsc::channel();
                convert_all_files(&transmitter_convert_events, &parameters).unwrap();
                assert_eq!(
                    true,
                    std::path::Path::new(&expected_output_filename).exists()
                );
                fs::remove_file(&expected_output_filename).unwrap();
            }
            Err(_e) => panic!("glob error"),
        }
    }
}

#[derive(Debug)]
pub struct ConvertParameters {
    pub files: Vec<String>,
    pub in_place: bool,
    pub archive: Option<String>,
    pub default_password: String,
}
#[derive(Debug)]
pub enum ConvertEvent {
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
    process_stdout: &mut ChildStdout,
    temporary_file_base_page: &str,
    output_type: OutputType,
) -> Result<String, Box<dyn std::error::Error>> {
    debug!("reading size and output type from server");
    let mut buffer_size = [0; 2 + 2];
    process_stdout.read_exact(&mut buffer_size)?;
    let width_raw = buffer_size[..2].try_into().unwrap();
    let height_raw = buffer_size[2..4].try_into().unwrap();
    let width = u16::from_le_bytes(width_raw) as usize;
    let height = u16::from_le_bytes(height_raw) as usize;
    if height > MAX_IMG_HEIGHT || width > MAX_IMG_WIDTH || width * height * 4 > MAX_IMG_SIZE {
        panic!("Max image size exceeded: Probably DOS attempt");
    }

    debug!("reading page data from server");
    let mut buffer_page = vec![0; (height * width * 4) as usize];
    process_stdout.read_exact(&mut buffer_page)?;

    let png_file_path = format!("{}.png", temporary_file_base_page);
    let image = image::RgbaImage::from_raw(width as u32, height as u32, buffer_page).unwrap();
    image.save(&png_file_path).unwrap();

    match output_type {
        OutputType::Pdf => {
            let pdf_file_path = format!("{}.pdf", temporary_file_base_page);
            strict_process_execute("gm", &["convert", &png_file_path, &pdf_file_path]);
            Ok(pdf_file_path)
        }
        OutputType::Image => Ok(png_file_path),
    }
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
    let number_pages_raw = buffer_pages_and_type[..2].try_into().unwrap();
    let number_pages = u16::from_le_bytes(number_pages_raw);
    if number_pages > MAX_PAGES {
        debug!("Number of page sended by the server: {}", number_pages);
        panic!("Max page number exceeded: Probably DOS attempt");
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
    let output_type = OutputType::from(*buffer_pages_and_type.get(2).unwrap());
    output_file.push_str(output_type.extension());
    if output_type == OutputType::Image && number_pages != 1 {
        panic!("Image can only be 1 page. Abording.");
    }
    mpsc_sender.send(ConvertEvent::FileInfo {
        file: source_file.to_string(),
        output_type,
        number_pages,
    })?;
    let mut output_files = Vec::new();
    for page in 0..number_pages {
        let temporary_file_base_page = format!("{}.{}", temporary_directory, page);
        let converted_page =
            convert_one_page(process_stdout, &temporary_file_base_page, output_type)?;
        output_files.push(converted_page);
        mpsc_sender.send(ConvertEvent::PageConverted {
            file: source_file.to_string(),
            page,
        })?;
    }
    debug!("CONVERTED ALL PAGES");
    match output_type {
        OutputType::Image => {
            fs::copy(output_files.get(0).unwrap(), output_file).unwrap();
        }
        OutputType::Pdf => {
            output_files.push(output_file);
            if !Command::new("pdfunite")
                .args(&output_files)
                .output()
                .expect("Unable to launch pdfunite process")
                .status
                .success()
            {
                panic!("pdfunite failed");
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
    parameters: &ConvertParameters,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!("{:?}", parameters);
    let temporary_directory = format!(
        "{}/qubes_convert_{}",
        temp_dir().to_str().unwrap(),
        Uuid::new_v4()
    );
    fs::create_dir_all(&temporary_directory)?;
    let archive_path = match &parameters.archive {
        Some(path) => format!("{}/", fs::canonicalize(path).unwrap().to_str().unwrap()),
        None => default_archive_folder(),
    };
    fs::create_dir_all(&archive_path)?;
    let mut server_process = Command::new(QREXEC_BINARY)
        .args(&["@dispvm", "qubes.Convert"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Convert server failed to start");
    let mut server_process_stdin = server_process.stdin.take().unwrap();
    let mut server_process_stdout = server_process.stdout.as_mut().unwrap();

    server_process_stdin.write_all(format!("{}\n", parameters.default_password).as_bytes())?;
    server_process_stdin.write_all(format!("{}\n", parameters.files.len()).as_bytes())?;

    let mut file_id = 0;
    for filename in &parameters.files {
        debug!("Transmitting file {} to server", filename);
        let temporary_directory_file = format!("{}/{}", temporary_directory, file_id);
        fs::create_dir_all(&temporary_directory_file).unwrap();
        let mut buffer = Vec::new();
        let mut file = File::open(&filename)?;
        file.read_to_end(&mut buffer)?;
        server_process_stdin.write_all(format!("{}\n", buffer.len()).as_bytes())?;
        server_process_stdin.write_all(&buffer)?;
        debug!("File {} have been transmitted to the server", filename);
        if let Err(e) = convert_one_file(
            message_for_ui_emetter,
            &mut server_process_stdout,
            filename,
            &temporary_directory_file,
            parameters,
            &archive_path,
        ) {
            message_for_ui_emetter.send(ConvertEvent::Failure {
                file: filename.to_string(),
                message: format!("{:?}", e),
            })?;
        }
        file_id += 1;
        fs::remove_dir_all(&temporary_directory_file).unwrap();
    }
    fs::remove_dir_all(&temporary_directory).unwrap();
    Ok(())
}
