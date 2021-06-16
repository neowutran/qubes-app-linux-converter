#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]
use crate::common::{strict_process_execute, OutputType, IMG_DEPTH};
use log::{debug, error};
use std::{
    convert::TryInto,
    env::temp_dir,
    ffi::OsString,
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    process::{ChildStdout, Command, Stdio},
    sync::mpsc::Sender,
};
use uuid::Uuid;

#[cfg(test)]
use glob::glob;
#[cfg(test)]
use std::sync::mpsc::{self, Receiver};

const MAX_PAGES: u16 = 10_000;
const MAX_IMG_WIDTH: usize = 10_000;
const MAX_IMG_HEIGHT: usize = 10_000;
const MAX_IMG_SIZE: usize = MAX_IMG_WIDTH * MAX_IMG_HEIGHT * 3;

#[cfg(not(test))]
const QREXEC_BINARY: &str = "/usr/bin/qrexec-client-vm";

#[cfg(test)]
const QREXEC_BINARY: &str = "target/debug/server";

#[test]
fn convert_integration_test() {
    env_logger::init();
    for entry in glob("tests/files/*").expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                println!("{:?}", path.display());
                let directory = path.parent().unwrap().to_str().unwrap();
                let file_base_name = path.file_stem().unwrap().to_str().unwrap();
                let file_extension = path.extension().unwrap().to_str().unwrap();
                let mimetype: mime::Mime = tree_magic::from_filepath(&path)
                    .parse()
                    .expect("Incorrect detection of mimetype");
                let mut expected_output_filename = format!(
                    "{}/{}.trusted.",
                    &directory,
                    &file_base_name
                );
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
                        archive: Some("./tests/archives".to_string()),
                        files: vec![path.to_str().unwrap().to_string()],
                        default_password: "toor".to_string()
                    };
                    let (transmitter_convert_events, receiver_convert_events) = mpsc::channel();
                    convert_all_files(&transmitter_convert_events, &parameters).unwrap();
                assert_eq!(
                    true,
                    std::path::Path::new(&expected_output_filename).exists()
                    );
                fs::remove_file(&expected_output_filename).unwrap();
                fs::rename(&format!("tests/archives/{}.{}", &file_base_name, file_extension),  &path).unwrap();
            }
            Err(e) => panic!("glob error"),
        }
    }
}


#[derive(Debug)]
pub struct ConvertParameters {
    pub files: Vec<String>,
    pub in_place: bool,
    pub archive: Option<String>,
    pub default_password: String
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
    tmp_output_file: &str,
    full_pdf_file: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!("BEGIN CONVERT ONE PAGE: {}", temporary_file_base_page);

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

    let rgba_file_path = format!("{}.rgba", temporary_file_base_page);
    let png_file_path = format!("{}.png", temporary_file_base_page);
    debug!("rgba file is: {}", &rgba_file_path);
    let mut rgba_file = File::create(&rgba_file_path)?;
    rgba_file.write_all(&buffer_page)?;
    debug!("convert RGBA to PNG");
    strict_process_execute(
        "gm",
        &[
            "convert",
            "-size",
            &format!("{}x{}", width, height),
            "-depth",
            &format!("{}", IMG_DEPTH),
            &format!("rgba:{}", rgba_file_path),
            &format!("png:{}", png_file_path),
        ],
    );
    debug!("convert PNG to output type");
    match output_type {
        OutputType::Image => {
            fs::copy(&png_file_path, &tmp_output_file)?;
        }
        OutputType::Pdf => {
            let pdf_file_path = format!("{}.pdf", temporary_file_base_page);
            strict_process_execute("gm", &["convert", &png_file_path, &pdf_file_path]);
            // Merge this PDF page with the others
            if Path::new(tmp_output_file).exists() {
                strict_process_execute(
                    "pdfunite",
                    &[tmp_output_file, &pdf_file_path, full_pdf_file],
                );
                fs::copy(&full_pdf_file, &tmp_output_file)?;
            } else {
                fs::copy(&pdf_file_path, &tmp_output_file)?;
            }
            fs::remove_file(&pdf_file_path)?;
        }
    }

    debug!("remove temporary files");
    fs::remove_file(&png_file_path)?;
    fs::remove_file(&rgba_file_path)?;
    debug!("END CONVERT ONE PAGE: {}", temporary_file_base_page);
    Ok(())
}
fn convert_one_file(
    mpsc_sender: &Sender<ConvertEvent>,
    process_stdout: &mut ChildStdout,
    filename: &str,
    temporary_directory: &str,
    parameters: &ConvertParameters,
    archive_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!("BEGIN CONVERT ONE FILE: {}", filename);
    let mut buffer_pages_and_type = vec![0_u8; 2 + 1];
    process_stdout.read_exact(&mut buffer_pages_and_type)?;
    let number_pages_raw = buffer_pages_and_type[..2].try_into().unwrap();
    let number_pages = u16::from_le_bytes(number_pages_raw);
    if number_pages > MAX_PAGES {
        debug!("Number of page sended by the server: {}", number_pages);
        //TODO remove me:
        let mut sss = vec![0_u8; 150];
        process_stdout.read_exact(&mut sss).unwrap();
        debug!("{:?}", sss);
        panic!("Max page number exceeded: Probably DOS attempt");
    }
    let filename_path = fs::canonicalize(filename)?;
    let file_stem = filename_path.file_stem().unwrap().to_str().unwrap();
    let empty_extension = OsString::new();
    let file_extension = filename_path
        .extension()
        .unwrap_or(&empty_extension)
        .to_str()
        .unwrap();
    let file_parent = filename_path.parent().unwrap().to_str().unwrap();
    let temporary_file_base = format!("{}/{}", temporary_directory, file_stem);
    let mut tmp_output_file = format!("{}.trusted.", temporary_file_base);
    let mut output_file = format!("{}/{}.trusted.", file_parent, file_stem);
    let output_type = OutputType::from(*buffer_pages_and_type.get(2).unwrap());
    tmp_output_file.push_str(output_type.extension());
    output_file.push_str(output_type.extension());
    let full_pdf_file = format!("{}/{}.pdf", temporary_directory, file_stem);
    if output_type == OutputType::Image && number_pages != 1 {
        panic!("Image can only be 1 page. Abording.");
    }
    mpsc_sender.send(ConvertEvent::FileInfo {
        file: filename.to_string(),
        output_type,
        number_pages,
    })?;
    for page in 0..number_pages {
        let temporary_file_base_page = format!("{}.{}", temporary_file_base, page);
        convert_one_page(
            process_stdout,
            &temporary_file_base_page,
            output_type,
            &tmp_output_file,
            &full_pdf_file,
        )?;
        mpsc_sender.send(ConvertEvent::PageConverted {
            file: filename.to_string(),
            page,
        })?;
    }
    debug!("CONVERTED ALL PAGES");
    mpsc_sender.send(ConvertEvent::FileConverted {
        file: filename.to_string(),
    })?;
    if output_type == OutputType::Pdf {
        let _dont_care_if_fail = fs::remove_file(&full_pdf_file);
    }
    if parameters.in_place {
        fs::remove_file(filename_path)?;
    } else {
        let archive_file = format!("{}{}.{}", &archive_path, &file_stem, &file_extension);
        debug!("archiving {:?} to {}", filename_path, &archive_file);
        fs::copy(&filename_path, &archive_file)?;
        fs::remove_file(&filename_path)?;
    }
    debug!(
        "moving temp output file '{}' to final ouput file '{}'",
        &tmp_output_file, &output_file
    );
    fs::copy(&tmp_output_file, &output_file)?;
    fs::remove_file(&tmp_output_file)?;
    debug!("END CONVERT ONE FILE: {}", filename);
    Ok(())
}
pub fn convert_all_files(
    message_for_ui_emetter: &Sender<ConvertEvent>,
    parameters: &ConvertParameters,
) -> Result<(), Box<dyn std::error::Error>> {
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

    for filename in &parameters.files {
        debug!("Transmitting file {} to server", filename);
        let mut buffer = Vec::new();
        let mut file = File::open(&filename)?;
        file.read_to_end(&mut buffer)?;
        server_process_stdin.write_all(format!("{}\n", buffer.len()).as_bytes())?;
        server_process_stdin.write_all(&buffer)?;
        debug!("File {} have been transmitted to the server", filename);
    }
    for filename in &parameters.files {
        if let Err(e) = convert_one_file(
            message_for_ui_emetter,
            &mut server_process_stdout,
            filename,
            &temporary_directory,
            parameters,
            &archive_path,
        ) {
            message_for_ui_emetter.send(ConvertEvent::Failure {
                file: filename.to_string(),
                message: format!("{:?}", e),
            })?;
        }
    }
    Ok(())
}
