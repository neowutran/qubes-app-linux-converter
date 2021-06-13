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

use qubes_app_linux_converter_common::{strict_process_execute, OutputType, IMG_DEPTH};
use std::{
    env::temp_dir,
    fs::{self, File},
    io::{self, prelude::*, BufRead},
    net::TcpStream,
    path::Path,
    process::Command,
    thread, time,
};
use uuid::Uuid;
use log::debug;

fn convert_image(file_path: &str, file_id: u16) {
    debug!("Start converting image");
    let number_pages: u16 = 1;
    io::stdout().write_all(&number_pages.to_le_bytes()).unwrap();
    io::stdout().write_all(&[OutputType::Image as u8]).unwrap();
    send_image(file_path, file_id);
    debug!("End converting image");
}
fn send_image(file_path: &str, _file_id: u16) {
    debug!("Start send_image");
    debug!("Start getting image dimension");
    let dimension_output =
        strict_process_execute("gm", &["identify", "-format", "%w %h", file_path]);
    let dimension_string = String::from_utf8(dimension_output.stdout).unwrap();
    let dimension_parts: Vec<&str> = dimension_string.split_whitespace().collect();
    let width: u16 = dimension_parts.get(0).unwrap().parse().unwrap();
    let height: u16 = dimension_parts.get(1).unwrap().parse().unwrap();
    io::stdout().write_all(&width.to_le_bytes()).unwrap();
    io::stdout().write_all(&height.to_le_bytes()).unwrap();
    debug!("Start converting image to RGBA");
    strict_process_execute(
        "gm",
        &[
            "convert",
            file_path,
            "-depth",
            &format!("{}", IMG_DEPTH),
            &format!("rgba:{}.rgba", file_path),
        ],
    );
    debug!("End converting image to RGBA");
    let mut rgba_file = File::open(&format!("{}.rgba", file_path)).unwrap();
    let mut buffer = Vec::new();
    rgba_file.read_to_end(&mut buffer).unwrap();
    fs::remove_file(&file_path).unwrap();
    fs::remove_file(&format!("{}.rgba", file_path)).unwrap();
    io::stdout().write_all(&buffer).unwrap();
    debug!("End send_image");
}

fn convert_pdf(file_path: &str, file_id: u16) {
    debug!("Start convert_pdf");
    debug!("Start getting password");
    let (password, number_pages) = get_pdf_password_and_pages(file_path, "");
    io::stdout().write_all(&number_pages.to_le_bytes()).unwrap();
    io::stdout().write_all(&[OutputType::Pdf as u8]).unwrap();

    debug!("Start converting PDF pages");
    for current_page in 1..number_pages+1 {
        let png_path = format!("{}.{}", file_path, current_page);
        strict_process_execute(
            "pdftocairo",
            &[
                "-opw",
                &password,
                "-upw",
                &password,
                file_path,
                "-png",
                "-f",
                &format!("{}", current_page),
                "-l",
                &format!("{}", current_page),
                "-singlefile",
                &png_path,
            ],
        );
        send_image(&format!("{}.png", png_path), file_id);
    }
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
fn get_pdf_password_and_pages(file_path: &str, password: &str) -> (String, u16) {
    let pdfinfo_process = Command::new("pdfinfo")
        .args(&["-opw", password, "-upw", password, file_path])
        .output()
        .expect("Unable to start pdfinfo process");
    if pdfinfo_process.status.success() {
        let stdout = String::from_utf8_lossy(&pdfinfo_process.stdout);
        for line in stdout.lines() {
            if line.starts_with("Pages:") {
                let line_parts: Vec<&str> = line.split(':').collect();
                let pages: u16 = line_parts
                    .get(1)
                    .expect("pdfinfo issue: no value for 'Pages:' attribut")
                    .trim()
                    .parse()
                    .expect("pdfinfo issue: 'Pages:' attribute value is not a number");
                return (password.to_string(), pages);
            }
        }
        panic!("pdfinfo issue: no 'Pages:' attribut");
    } else {
        let password = prompt_password();
        get_pdf_password_and_pages(file_path, &password)
    }
}
fn convert_office(file_path: &str, file_id: u16, temporary_directory: &str) {
    let no_password_file = format!("{}.nopassword", file_path);
    if convert_office_file_to_pdf_without_password(
        file_path,
        temporary_directory,
        &no_password_file,
    ) {
        convert_pdf(file_path, file_id);
        return;
    }
    let port = 2202;
    let mut libreoffice_server_process = Command::new("libreoffice")
        .args(&[
            &format!("--accept=socket,host=localhost,port={};urp", port),
            "--headless",
        ])
        .spawn()
        .expect("Unable to start libreoffice server process");
    let one_seconds = time::Duration::from_millis(1_000);
    while TcpStream::connect(&format!("127.0.0.1:{}", port)).is_err() {
        thread::sleep(one_seconds);
    }
    let mut is_success = false;
    while !is_success {
        let password = prompt_password();
        is_success = decrypt_office_file(file_path, &no_password_file, port, &password);
    }
    debug!("{}", libreoffice_server_process.id());
    libreoffice_server_process.kill().unwrap();
    if !convert_office_file_to_pdf_without_password(
        file_path,
        temporary_directory,
        &no_password_file,
    ) {
        panic!("Conversion should have succeeded ! Abording");
    }
    convert_pdf(file_path, file_id);
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
        .output()
        .expect("Unable to start python process");
    //debug!("{}", String::from_utf8(python_process.stderr).unwrap());
    python_process.status.success()
}
fn convert_office_file_to_pdf_without_password(
    file_path: &str,
    temporary_directory: &str,
    no_password_file: &str,
) -> bool {
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
    if !std::path::Path::new(&no_password_file).exists() {
        fs::copy(file_path, &no_password_file).unwrap();
    }
    debug!("converting {} to pdf in directory {}", no_password_file, temporary_directory);
    let _libreoffice_process_output = strict_process_execute(
        "libreoffice",
        &[
            "--headless",
            "--convert-to",
            "pdf",
            &no_password_file,
            "--outdir",
            temporary_directory,
        ],
    );
    fs::remove_file(&no_password_file).unwrap();
    let converted_file = format!("{}.pdf", file_path);
    if std::path::Path::new(&converted_file).exists(){
        fs::rename(&converted_file, file_path).unwrap();
        return true;
    }
    false
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let stdin = io::stdin();
    let dir = format!("{}/qubes_convert_{}", temp_dir().to_str().unwrap(), Uuid::new_v4());
    fs::create_dir_all(&dir)?;
    let number_files: u16 = stdin.lock().lines().next().unwrap()?.parse()?;
    for file_id in 0..number_files {
        let number_bytes: usize = stdin.lock().lines().next().unwrap()?.parse()?;
        debug!("Receiving file");
        let mut buffer = vec![0; number_bytes];
        stdin.lock().read_exact(&mut buffer)?;
        debug!("File received");
        let file_path = format!("{}/{}", &dir, &file_id);
        let mut file = File::create(&file_path)?;
        file.write_all(&buffer)?;
        debug!("File written to disk");
        let mimetype: mime::Mime = tree_magic::from_filepath(Path::new(&file_path))
            .parse()
            .expect("Incorrect detection of mimetype");
        debug!("Mime found: {:?}", mimetype);
        match (mimetype.type_(), mimetype.subtype()) {
            (mime::AUDIO, _) => panic!("Audio convert not implemented"),
            (mime::VIDEO, _) => panic!("Video convert not implemented"),
            (mime::IMAGE, _) => convert_image(&file_path, file_id),
            (_, mime::PDF) => convert_pdf(&file_path, file_id),
            _ => convert_office(&file_path, file_id, &dir),
        }
    }
    Ok(())
}
