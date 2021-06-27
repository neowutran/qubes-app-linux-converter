#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use log::debug;
use std::convert::TryFrom;

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
impl TryFrom<u8> for OutputType {
    type Error = &'static str;
    fn try_from(orig: u8) -> Result<Self, Self::Error> {
        if orig == Self::Image as u8 {
            return Ok(Self::Image);
        }
        if orig == Self::Pdf as u8 {
            return Ok(Self::Pdf);
        }
        Err("Impossible value")
    }
}
pub fn strict_process_execute(binary: &str, args: &[&str]) -> ProcessOutput {
    debug!("{}: {:?}", binary, args);
    let process = std::process::Command::new(binary)
        .args(args)
        .output()
        .expect("Unable to start process");
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
