use std::io::{self, BufRead, BufReader, Read};

const INITIAL_BUF_SIZE: usize = 64 * 1024;
const MAX_LINE_SIZE: usize = 64 * 1024 * 1024;

/// LineReader reads JSONL files line by line, skipping lines that exceed
/// MAX_LINE_SIZE rather than aborting. Tracks total bytes read for offset tracking.
pub struct LineReader<R: Read> {
    reader: BufReader<R>,
    buf: String,
    err: Option<io::Error>,
    bytes_read: u64,
}

impl<R: Read> LineReader<R> {
    pub fn new(reader: R) -> Self {
        LineReader {
            reader: BufReader::with_capacity(INITIAL_BUF_SIZE, reader),
            buf: String::with_capacity(INITIAL_BUF_SIZE),
            err: None,
            bytes_read: 0,
        }
    }

    /// Returns the next non-empty line (without trailing newline) and true,
    /// or None at EOF or I/O error.
    pub fn next_line(&mut self) -> Option<&str> {
        loop {
            self.buf.clear();
            match self.reader.read_line(&mut self.buf) {
                Ok(0) => return None, // EOF
                Ok(n) => {
                    self.bytes_read += n as u64;
                    let trimmed = self.buf.trim_end_matches('\n').trim_end_matches('\r');
                    if trimmed.is_empty() {
                        continue;
                    }
                    if trimmed.len() > MAX_LINE_SIZE {
                        continue; // skip oversized lines
                    }
                    // Return the trimmed line - need to truncate buf to match
                    let len = trimmed.len();
                    self.buf.truncate(len);
                    return Some(&self.buf);
                }
                Err(e) => {
                    self.err = Some(e);
                    return None;
                }
            }
        }
    }

    /// Returns the first non-EOF I/O error encountered, or None.
    pub fn err(&self) -> Option<&io::Error> {
        self.err.as_ref()
    }

    /// Total bytes consumed from the reader.
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }
}
