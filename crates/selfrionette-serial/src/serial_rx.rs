//! Serial chunks retain incomplete lines and later frames within one fixed bound.
use crate::{MAX_LINE_BYTES, TransportError};

pub(super) const READ_CHUNK_BYTES: usize = 256;

pub(super) struct LineBuffer(Vec<u8>);
impl LineBuffer {
    pub fn new() -> Self {
        Self(Vec::with_capacity(MAX_LINE_BYTES))
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn remaining(&self) -> usize {
        MAX_LINE_BYTES - self.0.len()
    }
    pub fn append(&mut self, chunk: &[u8]) -> Result<(), TransportError> {
        if chunk.len() > self.remaining() {
            self.clear();
            return Err(TransportError::LineTooLong {
                max_bytes: MAX_LINE_BYTES,
            });
        }
        self.0
            .extend(chunk.iter().copied().filter(|value| *value != b'\r'));
        Ok(())
    }
    pub fn take_line(&mut self) -> Result<Option<String>, TransportError> {
        if let Some(newline) = self.0.iter().position(|byte| *byte == b'\n') {
            let mut bytes = self.0.drain(..=newline).collect::<Vec<_>>();
            bytes.pop();
            return String::from_utf8(bytes)
                .map(Some)
                .map_err(|_| TransportError::InvalidUtf8);
        }
        if self.remaining() == 0 {
            self.clear();
            return Err(TransportError::LineTooLong {
                max_bytes: MAX_LINE_BYTES,
            });
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chunks_preserve_split_frames_multiple_lines_and_timeout_state() {
        let mut buffer = LineBuffer::new();
        buffer.append(b"sample3,1,2\r").unwrap();
        assert!(buffer.take_line().unwrap().is_none());
        buffer.append(b"\nnext\npartial").unwrap();
        assert_eq!(buffer.take_line().unwrap().as_deref(), Some("sample3,1,2"));
        assert_eq!(buffer.take_line().unwrap().as_deref(), Some("next"));
        assert!(buffer.take_line().unwrap().is_none());
        buffer.append(b"-end\n").unwrap();
        assert_eq!(buffer.take_line().unwrap().as_deref(), Some("partial-end"));
    }
    #[test]
    fn lines_remain_bounded_and_invalid_utf8_does_not_become_a_sample() {
        let mut buffer = LineBuffer::new();
        buffer.append(&[b'x'; MAX_LINE_BYTES]).unwrap();
        assert!(matches!(
            buffer.take_line(),
            Err(TransportError::LineTooLong { .. })
        ));
        assert_eq!(buffer.remaining(), MAX_LINE_BYTES);
        buffer.append(&[0xff, b'\n']).unwrap();
        assert!(matches!(
            buffer.take_line(),
            Err(TransportError::InvalidUtf8)
        ));
        let mut last = vec![b'x'; MAX_LINE_BYTES];
        last[MAX_LINE_BYTES - 1] = b'\n';
        buffer.append(&last).unwrap();
        assert_eq!(
            buffer.take_line().unwrap().unwrap().len(),
            MAX_LINE_BYTES - 1
        );
        buffer.append(b"not retained").unwrap();
        buffer.clear();
        assert!(buffer.take_line().unwrap().is_none());
    }
}
