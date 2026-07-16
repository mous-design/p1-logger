use crate::parser::{self, ParseError, Telegram};
use std::io::{self, BufRead};

/// One thing that happened while framing telegrams out of a P1 stream.
/// A '/' line starts a telegram, a '!' line (CRC footer) ends it; the
/// meter sends telegrams back-to-back, so anything else is worth a
/// caller's attention rather than silent discarding.
pub enum TelegramEvent {
    /// A full telegram was framed, in its raw (lossily-decoded) text and
    /// its parse result -- a caller can dump the raw text for inspection
    /// even when parsing failed.
    Parsed { raw_text: String, result: Result<Telegram, ParseError> },
    /// A telegram was mid-accumulation (no footer yet) when a new '/'
    /// arrived; the old lines are discarded and accumulation restarts.
    /// Expected occasionally (line noise, attaching mid-transmission at
    /// startup) -- worth a trace, not an alarm.
    Resynced { lines: Vec<String> },
    /// The stream ended while a telegram (or unresolved post-footer data,
    /// see `HeaderLost`/`UnexplainedData`) was mid-accumulation. Same
    /// severity as `Resynced`: normal on a real shutdown, still worth a
    /// trace.
    Truncated { lines: Vec<String> },
    /// Data and a footer arrived right after a previous footer, with no
    /// header of its own in between: the telegram's opening '/' line
    /// apparently got lost, but its body and footer made it through.
    /// Expected occasionally -- exactly what line noise eating one
    /// specific line looks like. Worth a trace, not an alarm.
    HeaderLost { lines: Vec<String> },
    /// Data arrived after a footer and was interrupted by a new header
    /// before ever getting a footer of its own -- so it was never even a
    /// headerless telegram (see `HeaderLost`), just unaccounted-for data
    /// sitting in a gap where the meter never sends anything. This should
    /// not be able to happen at all. Worth an error.
    UnexplainedData { lines: Vec<String> },
}

enum State {
    /// Before the very first header: anything else is discarded silently
    /// (attaching to a live stream mid-transmission is normal).
    AwaitingFirstHeader,
    /// Between a '/' and (hopefully) its '!'.
    Accumulating(Vec<String>),
    /// Just after a footer. Nothing should arrive before the next '/';
    /// if something does, it's held here until a footer or a new header
    /// resolves what it was.
    AfterFooter { stray_lines: Vec<String> },
}

pub struct TelegramReader<R> {
    reader: R,
    state: State,
}

impl<R: BufRead> TelegramReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            state: State::AwaitingFirstHeader,
        }
    }

    /// Reads until one event has happened, or the source is exhausted (return `Ok(None)`).
    ///
    /// Raw bytes, not `.lines()`: a live serial line is noise-prone (bit
    /// flips, torn reads across a restart), and `.lines()` panics on the
    /// first byte sequence that isn't valid UTF-8 -- turning ordinary line
    /// noise into a crash instead of just a skipped, CRC-rejected telegram
    /// like any other corruption.
    pub fn next_telegram(&mut self) -> io::Result<Option<TelegramEvent>> {
        loop {
            let mut line_buf = Vec::<u8>::new();
            let bytes_read = self.reader.read_until(b'\n', &mut line_buf)?;
            if bytes_read == 0 || line_buf.last() != Some(&b'\n') {
                return Ok(self.handle_eof());
            }
            if line_buf.len() < 2 || line_buf[line_buf.len() - 2] != b'\r' {
                log::warn!("line missing \\r before \\n (DSMR requires CRLF); accepting anyway");
            }
            let line = String::from_utf8_lossy(&line_buf).into_owned();

            let event = if line.starts_with('/') {
                self.handle_header(line)
            } else if line.starts_with('!') {
                self.handle_footer(line)
            } else {
                self.handle_body_line(line);
                None
            };
            if event.is_some() {
                return Ok(event);
            }
        }
    }

    fn handle_header(&mut self, line: String) -> Option<TelegramEvent> {
        let event = match &mut self.state {
            State::AwaitingFirstHeader => None,
            State::Accumulating(lines) => Some(TelegramEvent::Resynced { lines: std::mem::take(lines) }),
            State::AfterFooter { stray_lines } if stray_lines.is_empty() => None,
            State::AfterFooter { stray_lines } => {
                Some(TelegramEvent::UnexplainedData { lines: std::mem::take(stray_lines) })
            }
        };
        self.state = State::Accumulating(vec![line]);
        event
    }

    fn handle_footer(&mut self, line: String) -> Option<TelegramEvent> {
        match &mut self.state {
            State::AwaitingFirstHeader => None,
            State::Accumulating(lines) => {
                lines.push(line);
                let telegram_text = lines.concat();
                let result = parser::parse_telegram(&telegram_text);
                self.state = State::AfterFooter { stray_lines: Vec::new() };
                Some(TelegramEvent::Parsed { raw_text: telegram_text, result })
            }
            State::AfterFooter { stray_lines } if stray_lines.is_empty() => None,
            State::AfterFooter { stray_lines } => {
                // Taking just the Vec resets it to empty in place -- no
                // separate whole-state reassignment needed.
                Some(TelegramEvent::HeaderLost { lines: std::mem::take(stray_lines) })
            }
        }
    }

    fn handle_body_line(&mut self, line: String) {
        match &mut self.state {
            State::AwaitingFirstHeader => {}
            State::Accumulating(lines) => lines.push(line),
            State::AfterFooter { stray_lines } => stray_lines.push(line),
        }
    }

    fn handle_eof(&mut self) -> Option<TelegramEvent> {
        // Unlike header/footer handling, this always resets to
        // AwaitingFirstHeader: a repeat call after real EOF must not
        // re-report the same truncation.
        match &mut self.state {
            State::AwaitingFirstHeader => None,
            State::Accumulating(lines) => {
                let lines = std::mem::take(lines);
                self.state = State::AwaitingFirstHeader;
                Some(TelegramEvent::Truncated { lines })
            }
            State::AfterFooter { stray_lines } if stray_lines.is_empty() => None,
            State::AfterFooter { stray_lines } => {
                let lines = std::mem::take(stray_lines);
                self.state = State::AwaitingFirstHeader;
                Some(TelegramEvent::Truncated { lines })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn reader_over(input: &str) -> TelegramReader<Cursor<&[u8]>> {
        TelegramReader::new(Cursor::new(input.as_bytes()))
    }

    const VALID_TELEGRAM: &str = "/ISK5\\2M550T-1013\r\n\r\n0-0:1.0.0(260712163115S)\r\n1-0:1.8.1(005081.531*kWh)\r\n!F1DE\r\n";

    fn expect_parsed(event: TelegramEvent) -> (String, Result<Telegram, ParseError>) {
        match event {
            TelegramEvent::Parsed { raw_text, result } => (raw_text, result),
            _ => panic!("expected Parsed event"),
        }
    }

    #[test]
    fn returns_none_on_empty_source() {
        let mut reader = reader_over("");
        assert!(reader.next_telegram().unwrap().is_none());
    }

    #[test]
    fn ignores_noise_before_the_first_telegram() {
        // No telegram has completed yet, so this is startup skew (attaching
        // mid-transmission) -- must not surface any event, not even
        // UnexplainedData.
        let input = format!("garbage before anything starts\r\n{VALID_TELEGRAM}");
        let mut reader = reader_over(&input);
        let (text, result) = expect_parsed(reader.next_telegram().unwrap().unwrap());
        assert!(result.is_ok());
        assert!(!text.contains("garbage"));
    }

    #[test]
    fn resyncs_on_a_stray_slash_mid_telegram() {
        // An incomplete telegram (no footer) followed immediately by a
        // real one: the second '/' must discard the first attempt rather
        // than merging into it, and report the abandonment.
        let input = format!("/ISK5\\2M550T-1013\r\n\r\n1-0:1.8.1(dangling)\r\n{VALID_TELEGRAM}");
        let mut reader = reader_over(&input);

        match reader.next_telegram().unwrap().unwrap() {
            TelegramEvent::Resynced { lines } => assert_eq!(lines.len(), 3),
            _ => panic!("expected Resynced event"),
        }

        let (text, result) = expect_parsed(reader.next_telegram().unwrap().unwrap());
        assert!(result.is_ok());
        assert!(!text.contains("dangling"));
    }

    #[test]
    fn a_telegram_with_no_footer_at_eof_reports_truncation_then_ends() {
        let input = "/ISK5\\2M550T-1013\r\n\r\n1-0:1.8.1(005081.531*kWh)\r\n";
        let mut reader = reader_over(input);

        match reader.next_telegram().unwrap().unwrap() {
            TelegramEvent::Truncated { lines } => assert_eq!(lines.len(), 3),
            _ => panic!("expected Truncated event"),
        }
        assert!(reader.next_telegram().unwrap().is_none());
    }

    #[test]
    fn data_then_a_footer_with_no_header_is_reported_as_header_lost() {
        // footer, [data, no header], footer: the second telegram's '/'
        // apparently got lost, but its body and footer made it through.
        let input = format!("{VALID_TELEGRAM}1-0:1.8.1(headerless)\r\n!DEAD\r\n");
        let mut reader = reader_over(&input);

        assert!(expect_parsed(reader.next_telegram().unwrap().unwrap()).1.is_ok());

        match reader.next_telegram().unwrap().unwrap() {
            TelegramEvent::HeaderLost { lines } => {
                assert_eq!(lines, vec!["1-0:1.8.1(headerless)\r\n".to_string()]);
            }
            _ => panic!("expected HeaderLost event"),
        }
    }

    #[test]
    fn data_then_a_new_header_with_no_footer_is_reported_as_unexplained() {
        // footer, [data, no footer], header: that data was never even a
        // headerless telegram -- it just sat in a gap that should be
        // empty. This must never happen on a healthy stream.
        let input = format!("{VALID_TELEGRAM}this should never be here\r\n{VALID_TELEGRAM}");
        let mut reader = reader_over(&input);

        assert!(expect_parsed(reader.next_telegram().unwrap().unwrap()).1.is_ok());

        match reader.next_telegram().unwrap().unwrap() {
            TelegramEvent::UnexplainedData { lines } => {
                assert_eq!(lines, vec!["this should never be here\r\n".to_string()]);
            }
            _ => panic!("expected UnexplainedData event"),
        }

        assert!(expect_parsed(reader.next_telegram().unwrap().unwrap()).1.is_ok());
    }

    #[test]
    fn a_torn_final_line_with_no_newline_is_not_treated_as_a_real_line() {
        // Stream ends mid-line (no trailing \n at all): read_until returns
        // the partial bytes, but they must not be mistaken for a complete
        // line -- e.g. a torn header fragment must not silently start a
        // "telegram" that then goes on to report bogus abandoned lines
        // from before it. Treated exactly like a clean EOF.
        let input = format!("{VALID_TELEGRAM}1-0:1.8.1(dangling)\r\n/torn-no-newl");
        let mut reader = reader_over(&input);

        assert!(expect_parsed(reader.next_telegram().unwrap().unwrap()).1.is_ok());

        match reader.next_telegram().unwrap().unwrap() {
            TelegramEvent::Truncated { lines } => assert_eq!(lines.len(), 1),
            _ => panic!("expected Truncated event"),
        }
        assert!(reader.next_telegram().unwrap().is_none());
    }

    #[test]
    fn a_line_missing_cr_is_still_accepted_and_its_crc_reflects_the_real_bytes() {
        // DSMR requires CRLF; a bare \n must still be accepted (just
        // warned about, which isn't asserted here -- no test log sink),
        // not treated as a framing error. The CRC below was computed over
        // these exact bytes -- including the missing \r on the content
        // line -- rather than assuming a full \r\n: reconstructing the
        // telegram by re-joining lines with an invented "\r\n" would
        // silently corrupt the CRC input for a line that never had one.
        let input = "/ISK5\\2M550T-1013\r\n\r\n0-0:1.0.0(260712163115S)\r\n1-0:1.8.1(005081.531*kWh)\n!B020\r\n";
        let mut reader = reader_over(input);
        let (_, result) = expect_parsed(reader.next_telegram().unwrap().unwrap());
        assert!(result.is_ok());
    }

    #[test]
    fn invalid_utf8_bytes_are_lossily_decoded_not_fatal() {
        // A telegram containing a raw invalid-UTF-8 byte must not error out
        // of read_until/next_telegram -- it should surface as a parse
        // failure (bad CRC) on lossily-decoded text, same as any other
        // corruption, and the stream must resync onto the next telegram.
        let mut input = b"/ISK5\\2M550T-1013\r\n\r\n1-0:1.8.1(005\xFF\xFE081.531*kWh)\r\n!0000\r\n".to_vec();
        input.extend_from_slice(VALID_TELEGRAM.as_bytes());
        let mut reader = TelegramReader::new(Cursor::new(input.as_slice()));

        let (_, first) = expect_parsed(reader.next_telegram().unwrap().unwrap());
        assert!(matches!(first, Err(ParseError::CrcMismatch { .. })));

        let (_, second) = expect_parsed(reader.next_telegram().unwrap().unwrap());
        assert!(second.is_ok());
    }
}
