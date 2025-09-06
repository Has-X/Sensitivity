use anyhow::Result;
use md5::{Digest, Md5};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

pub fn md5_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Md5::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(format!("{:x}", digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_md5_file() {
        let mut tf = NamedTempFile::new().unwrap();
        write!(tf, "hello world").unwrap();
        let got = md5_file(tf.path()).unwrap();
        assert_eq!(got, "5eb63bbbe01eeed093cb22bb8f5acdc3");
    }
}

