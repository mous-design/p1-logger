use std::fs::File;
use std::io::{self, Read};
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::time::Duration;

/// Opens `path` as a P1 data source. A character device (the real `/dev/serial0`)
/// is opened as a configured serial port; anything else (a plain file) is just
/// read as-is. This lets the same binary run against real hardware or a captured
/// telegram file, without the caller needing to know which one it got.
pub fn open_source(path: &Path) -> io::Result<Box<dyn Read>> {
    let file_type = std::fs::metadata(path)?.file_type();

    if file_type.is_char_device() {
        let path_str = path.to_str().expect("serial device path must be valid UTF-8");
        let port = serialport::new(path_str, 115_200)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .timeout(Duration::from_secs(10))
            .open()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(Box::new(port))
    } else {
        Ok(Box::new(File::open(path)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn regular_file_reads_back_bytes() {
        let mut path = std::env::temp_dir();
        path.push("p1-logger-io-source-test.txt");
        std::fs::File::create(&path).unwrap().write_all(b"hello telegram").unwrap();

        let mut source = open_source(&path).unwrap();
        let mut buf = Vec::new();
        source.read_to_end(&mut buf).unwrap();

        assert_eq!(buf, b"hello telegram");
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn char_device_is_routed_to_serial_open_not_plain_read() {
        // /dev/null is a real char device on any Unix, but not a serial
        // port: configuring it (baud/data bits/...) must fail, proving
        // detection picked the serial branch rather than silently doing
        // a plain file read.
        match open_source(Path::new("/dev/null")) {
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::Other),
            Ok(_) => panic!("expected serial configuration on /dev/null to fail"),
        }
    }
}
