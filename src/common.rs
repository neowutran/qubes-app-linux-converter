#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use log::debug;

pub const IMG_DEPTH: u8 = 8;
pub struct ProcessOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
#[repr(u8)]
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum OutputType {
    Image = 1,
    Pdf = 0,
}
impl From<u8> for OutputType {
    fn from(orig: u8) -> Self {
        if orig == OutputType::Image as u8 {
            return OutputType::Image;
        }
        if orig == OutputType::Pdf as u8 {
            return OutputType::Pdf;
        }
        panic!("Impossible value");
    }
}
impl OutputType {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Image => "png",
        }
    }
}
pub fn strict_process_execute(binary: &str, args: &[&str]) -> ProcessOutput {
    debug!("{}: {:?}", binary, args);
    let process = std::process::Command::new(binary)
        .args(args)
        .output()
        .expect(&format!("Unable to start {} process", binary));
    if !process.status.success() {
        debug!(
            "Following process failed: {} {:?}. Panicking ",
            binary, args
        );
        debug!("{}", String::from_utf8_lossy(&process.stderr));
    }
    ProcessOutput {
        stdout: process.stdout,
        stderr: process.stderr,
    }
}
