#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use std::convert::TryFrom;

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
