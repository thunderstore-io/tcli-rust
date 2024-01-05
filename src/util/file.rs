use std::fs::File;
use std::io;
use std::path::Path;
use md5::{Digest, Md5};
use md5::digest::FixedOutput;
use crate::error::Error;

pub fn md5(file: &Path) -> Result<String, Error> {
    let mut md5 = Md5::new();
    let mut file = File::open(file)?;
    io::copy(&mut file, &mut md5)?;

    Ok(format!("{:x}", md5.finalize_fixed()))
}