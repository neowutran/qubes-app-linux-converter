use std::process::Command;
pub const IMG_DEPTH: u8 = 8;
pub struct ProcessOutput{
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>
}
#[repr(u8)]
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum OutputType{
    Image = 1,
    Pdf = 0
}
impl From<u8> for OutputType{
    fn from(orig: u8) -> Self{
        if orig == OutputType::Image as u8{
            return OutputType::Image;
        }
        if orig == OutputType::Pdf as u8{
            return OutputType::Pdf;
        }
        panic!("Impossible value");
    }
}
impl OutputType{
    pub fn extension(self) -> &'static str{
        match self{
            Self::Pdf => "pdf",
            Self::Image => "png"
        }
    }
}
pub fn strict_process_execute(binary: &str, args: &[&str]) -> ProcessOutput{
    let process = Command::new(binary)
        .args(args)
        .output()
        .expect(&format!("Unable to start {} process", binary));
    if !process.status.success() {
        eprintln!("Following process failed: {} {:?}. Panicking ", binary, args);
        eprintln!("{}", String::from_utf8_lossy(&process.stderr));
        panic!("process execution unsuccessfull");
    }
    ProcessOutput{stdout: process.stdout, stderr: process.stderr}
}
