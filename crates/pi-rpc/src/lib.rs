//! Client for `pi --mode rpc`: strict LF-framed JSONL over the child's stdio.
//!
//! The GUI never embeds Node. It spawns `pi --mode rpc` with
//! `--no-builtin-tools` and talks to it here: request/response correlation,
//! the event stream, and the extension-UI sub-protocol (confirm cards).
//! See docs/ARCHITECTURE.md.

/// One JSONL frame: a single line, without the trailing `\n`.
pub type Frame = String;

/// Split a byte buffer into complete frames, returning the unconsumed tail.
#[must_use]
pub fn split_frames(buffer: &[u8]) -> (Vec<Frame>, Vec<u8>) {
    let mut frames = Vec::new();
    let mut rest = Vec::new();
    for chunk in buffer.split_inclusive(|byte| *byte == b'\n') {
        if chunk.last() == Some(&b'\n') {
            let line = String::from_utf8_lossy(&chunk[..chunk.len() - 1]).into_owned();
            if !line.is_empty() {
                frames.push(line);
            }
        } else {
            rest.extend_from_slice(chunk);
        }
    }
    (frames, rest)
}

#[cfg(test)]
mod tests {
    use super::split_frames;

    #[test]
    fn keeps_partial_tail_for_next_read() {
        let (frames, rest) = split_frames(b"{\"a\":1}\n{\"b\":");
        assert_eq!(frames, vec!["{\"a\":1}".to_owned()]);
        assert_eq!(rest, b"{\"b\":");
    }
}
