/*
 This program is free software; you can redistribute it and/or
 modify it under the terms of the GNU General Public License
 as published by the Free Software Foundation; either version 2
 of the License, or (at your option) any later version.

 This program is distributed in the hope that it will be useful,
 but WITHOUT ANY WARRANTY; without even the implied warranty of
 MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 GNU General Public License for more details.

 You should have received a copy of the GNU General Public License
 along with this program; if not, write to the Free Software
 Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301, USA.

-------------------------------------------------
 A similar project exist:
 - https://github.com/firstlookmedia/dangerzone-converter
 Both projects can improve the other.
*/
mod common;
use common::{strict_process_execute, OutputType};
use image::io::Reader as ImageReader;
use log::debug;
use std::{
    fs::{self, File},
    io::{self, prelude::*, BufRead},
    net::TcpStream,
    process::{Child, Command, Stdio},
    sync::mpsc::channel,
    thread, time,
};
use uuid::Uuid;

fn convert_image(file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Start converting image");
    let number_pages: u16 = 1;
    io::stdout().write_all(&number_pages.to_le_bytes())?;
    io::stdout().write_all(&[OutputType::Image as u8])?;
    send_image(file_path)
}
fn convert_to_png_and_open(file_path: &str) -> image::DynamicImage {
    let png_file = format!("{}.png", file_path);
    strict_process_execute("gm", &["convert", file_path, &format!("png:{}", png_file)]);
    ImageReader::open(png_file)
        .unwrap()
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap()
}
fn send_image(file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Start send_image: {}", file_path);
    // Try to open the image with the image rust library. If it fail, convert it to png with GM and
    // retry.
    let png_image = match ImageReader::open(file_path)?.with_guessed_format() {
        Ok(img) => match img.decode() {
            Ok(supported) => supported,
            Err(_) => convert_to_png_and_open(file_path),
        },
        Err(_) => convert_to_png_and_open(file_path),
    };
    let rgba = png_image.into_rgba8();
    #[allow(clippy::cast_possible_truncation)]
    let height = rgba.height() as u16;
    #[allow(clippy::cast_possible_truncation)]
    let width = rgba.width() as u16;
    io::stdout().write_all(&width.to_le_bytes())?;
    io::stdout().write_all(&height.to_le_bytes())?;
    io::stdout().write_all(&rgba)?;
    fs::remove_file(&file_path)?;
    Ok(())
}

fn convert_pdf(
    temporary_directory_file: &str,
    default_password: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Start getting password");
    split_pdf_into_pages(temporary_directory_file, default_password);
    let mut pages: Vec<(String, Child)> = Vec::new();
    let max_number_pdftocairo_process = 25;
    let mut number_pdftocairo_process = 0;
    for entry in glob::glob(&format!("{}/pg_*.pdf", temporary_directory_file))
        .expect("Failed to read glob pattern")
    {
        while number_pdftocairo_process >= max_number_pdftocairo_process {
            number_pdftocairo_process = 0;
            for (pngfilename, pdftocairo_process) in &mut pages {
                match pdftocairo_process.try_wait() {
                    Ok(None) => {
                        number_pdftocairo_process += 1;
                        debug!("{}", pngfilename);
                    }
                    Ok(Some(_)) => {}
                    Err(_) => debug!(
                        "Impossible get pdftocairo process status. Assuming it is not a big issue."
                    ),
                }
            }
            debug!(
                "number of pdftocairo process running: {}",
                number_pdftocairo_process
            );
            if number_pdftocairo_process >= max_number_pdftocairo_process {
                let sleep_time = time::Duration::from_millis(100);
                thread::sleep(sleep_time);
            }
        }
        let path = entry.expect("glob error");
        let pngfilename = path.file_stem().unwrap().to_str().unwrap().to_string();
        let pdftocairo_process = Command::new("pdftocairo")
            .args(&[path.to_str().unwrap(), "-png", "-singlefile"])
            .current_dir(&temporary_directory_file)
            .spawn()
            .expect("Unable to launch pdftocairo process");
        pages.push((pngfilename, pdftocairo_process));
        number_pdftocairo_process += 1;
    }
    #[allow(clippy::cast_possible_truncation)]
    io::stdout().write_all(&(pages.len() as u16).to_le_bytes())?;
    io::stdout().write_all(&[OutputType::Pdf as u8])?;

    debug!("Start converting PDF pages");
    for (png_page, mut pdftocairo_process) in pages {
        debug!("sending {}", png_page);
        if !pdftocairo_process.wait().unwrap().success() {
            panic!("pdftocairo process failed");
        }
        send_image(&format!("{}/{}.png", temporary_directory_file, png_page))?;
    }
    Ok(())
}

fn prompt_password() -> String {
    let output = strict_process_execute(
        "zenity",
        &["--title", "File protected by password", "--password"],
    );
    return String::from_utf8(output.stdout)
        .expect("Password contains non-utf8 chars, should be impossible")
        .lines()
        .next()
        .unwrap()
        .to_string();
}
const TO_CONVERT_FILENAME: &str = "to_convert";
fn split_pdf_into_pages(temporary_directory_file: &str, password: &str) {
    let to_split = format!("{}/{}", temporary_directory_file, TO_CONVERT_FILENAME);
    let pdftk_process = Command::new("pdftk")
        .args(&[&to_split, "input_pw", password, "burst"])
        .stdout(Stdio::piped())
        .current_dir(temporary_directory_file)
        .output()
        .expect("Unable to start pdfinfo process");
    if !pdftk_process.status.success() {
        let password = prompt_password();
        split_pdf_into_pages(temporary_directory_file, &password);
    }
}
fn convert_office(
    temporary_directory: &str,
    default_password: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if convert_office_file_to_pdf_without_password(temporary_directory)? {
        convert_pdf(temporary_directory, default_password)?;
        return Ok(());
    }

    debug!("Launching the libreoffice server");
    let port = 2202;
    Command::new("libreoffice")
        .args(&[
            &format!("--accept=socket,host=localhost,port={};urp", port),
            "--headless",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .expect("Unable to start libreoffice server process");
    let one_seconds = time::Duration::from_millis(1_000);
    while TcpStream::connect(&format!("127.0.0.1:{}", port)).is_err() {
        thread::sleep(one_seconds);
    }
    debug!("Libreoffice server seems up and ready");
    let no_password_file = format!("{}/{}.nopassword", temporary_directory, TO_CONVERT_FILENAME);
    let source_file = format!("{}/{}", temporary_directory, TO_CONVERT_FILENAME);
    let mut is_success = if default_password.is_empty() {
        false
    } else {
        decrypt_office_file(&source_file, &no_password_file, port, default_password)
    };
    while !is_success {
        let password = prompt_password();
        is_success = decrypt_office_file(&source_file, &no_password_file, port, &password);
    }
    fs::rename(&no_password_file, &source_file).unwrap();
    if !convert_office_file_to_pdf_without_password(temporary_directory)? {
        panic!("Conversion should have succeeded ! Abording");
    }
    convert_pdf(temporary_directory, default_password)
}
fn decrypt_office_file(file_path: &str, no_password_file: &str, port: u16, password: &str) -> bool {
    /*
        Try to remove the password of a libreoffice-compatible file,
        and store the resulting file in INITIAL_NAME.nopassword.
        The steps are:
        - Connect to a libreoffice API server, listening on localhost on port 2202
        - Try to load a document with additionnal properties:
              - "Hidden" to not load any libreoffice GUI
              - "Password" to automatically try to decrypt the document
        - Store the document without additionnal properties [this remove the password]
    */
    debug!("Trying to remove password from {}", file_path);
    let python_process = Command::new("python3")
        .args(&[
            "-c",
            &format!(
                "
import uno

src = \"file://{}\"
dst = \"file://{}\"

local_context = uno.getComponentContext()
resolver = local_context.ServiceManager.createInstanceWithContext(\"com.sun.star.bridge.UnoUrlResolver\",local_context)
ctx = resolver.resolve(\"uno:socket,host=localhost,port={};urp;StarOffice.ComponentContext\")
smgr = ctx.ServiceManager
desktop = smgr.createInstanceWithContext(\"com.sun.star.frame.Desktop\", ctx)

hidden_property = uno.createUnoStruct(\"com.sun.star.beans.PropertyValue\")
hidden_property.Name = \"Hidden\"
hidden_property.Value = True

password_property = uno.createUnoStruct(\"com.sun.star.beans.PropertyValue\")
password_property.Name = \"Password\"
password_property.Value = \"{}\"

document = desktop.loadComponentFromURL(src,\"_blank\",0,(password_property, hidden_property,))
document.storeAsURL(dst, ())",
                file_path, no_password_file, port, password
            ),
        ])
        .stdout(Stdio::piped())
        .output()
        .expect("Unable to start python process");
    python_process.status.success()
}
fn convert_office_file_to_pdf_without_password(
    temporary_directory: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    /*
    The way libreoffice handle password changed with this commit
    https://github.com/LibreOffice/core/commit/0de0b1b64a1c122254bb821ea0eb9b038875e8d4
    Before this commit we could try to decrypt a non encrypted file, and it would be a success.
    After this commit, trying to decrypt a non encrypted file result in a failure.
    A patch could be applied to restore this behavior without breaking the other improvement.
    I suggested this patch https://bug-attachments.documentfoundation.org/attachment.cgi?id=170502
    However since I don't know if (or when) this patch (or a similar patch) will be
    accepted, I tried to write a workaroud
    1: Try to convert the office file to PDF
    2: If it succed: All good, EXIT
    3: If it fail: Assume it is because it is encrypted
    4: Try to decrypt it
    5: Convert the office file to PDF
    */
    let file_path = format!("{}/{}", temporary_directory, TO_CONVERT_FILENAME);
    strict_process_execute(
        "libreoffice",
        &[
            "--headless",
            "--convert-to",
            "pdf",
            &file_path,
            "--outdir",
            temporary_directory,
        ],
    );
    let converted_file = format!("{}.pdf", file_path);
    if std::path::Path::new(&converted_file).exists() {
        fs::rename(&converted_file, file_path)?;
        return Ok(true);
    }
    Ok(false)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let stdin = io::stdin();
    let temporary_directory = format!("/home/user/.temp_qubes_convert_{}", Uuid::new_v4());
    fs::create_dir_all(&temporary_directory)?;
    let default_password: String = stdin.lock().lines().next().unwrap()?;
    let number_files: u16 = stdin.lock().lines().next().unwrap()?.parse()?;
    let (tx, rx) = channel();
    let temporary_directory_clone = temporary_directory.clone();
    thread::spawn(move || {
        for file_id in 0..number_files {
            let temporary_directory_file = format!("{}/{}", &temporary_directory_clone, file_id);
            fs::create_dir_all(&temporary_directory_file).unwrap();
            let number_bytes: usize = stdin
                .lock()
                .lines()
                .next()
                .unwrap()
                .unwrap()
                .parse()
                .unwrap();
            debug!("Receiving file, size: {}", number_bytes);
            let mut buffer = vec![0; number_bytes];
            stdin.lock().read_exact(&mut buffer).unwrap();
            debug!("File received");
            let file_path = format!("{}/{}", &temporary_directory_file, TO_CONVERT_FILENAME);
            let mut file = File::create(&file_path).unwrap();
            file.write_all(&buffer).unwrap();
            debug!("File written to disk");
            let mimetype: mime::Mime = tree_magic::from_u8(&buffer)
                .parse()
                .expect("Incorrect detection of mimetype");
            debug!("Mime found: {:?}", mimetype);
            tx.send((temporary_directory_file, mimetype)).unwrap();
        }
    });
    for file_info in rx {
        let temporary_directory_file = file_info.0;
        let mimetype = file_info.1;
        match (mimetype.type_(), mimetype.subtype()) {
            (mime::AUDIO, _) => panic!("Audio convert not implemented"),
            (mime::VIDEO, _) => panic!("Video convert not implemented"),
            (mime::IMAGE, _) => convert_image(&format!(
                "{}/{}",
                &temporary_directory_file, &TO_CONVERT_FILENAME
            ))?,
            (_, mime::PDF) => convert_pdf(&temporary_directory_file, &default_password)?,
            _ => convert_office(&temporary_directory_file, &default_password)?,
        }
        fs::remove_dir_all(&temporary_directory_file)?;
    }
    fs::remove_dir_all(&temporary_directory)?;
    Ok(())
}
