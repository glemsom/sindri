//! Escape sequences that carry words out of the terminal are written here, so
//! the interface never spells one out at a call site.
//!
//! A sequence is built as bytes and handed back, never written: the writing belongs to
//! whoever owns the tty (§14), which is this crate's terminal and not the core. That is
//! also what makes a copy work over SSH — the bytes go down the same wire the frame does,
//! and the terminal at the near end owns the clipboard, so no `xclip`, `wl-copy` or
//! `pbcopy` has to be installed, found or trusted (§12, §17).

/// The bytes a copy leaves by: `OSC 52 ; c ; <base64> ST`, which every terminal that
/// speaks OSC 52 reads as "put these words in the reader's clipboard" (§12). `c` is the
/// system clipboard; the payload is base64 because the sequence carries no raw bytes of
/// its own, and it ends with ST rather than BEL so no terminal rings on a copy.
pub fn copy(text: &str) -> Vec<u8> {
    let mut sequence = Vec::from(&b"\x1b]52;c;"[..]);
    sequence.extend_from_slice(base64(text.as_bytes()).as_bytes());
    sequence.extend_from_slice(b"\x1b\\");
    sequence
}

/// `text` as base64, the standard alphabet, padded: the one encoding an OSC 52 reader
/// decodes, written here rather than taken as a dependency because it is twelve lines and
/// a crate for them would be a crate for everything.
fn base64(text: &[u8]) -> String {
    /// The alphabet RFC 4648 names, in the order it names it.
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut encoded = String::with_capacity(text.len().div_ceil(3) * 4);
    for chunk in text.chunks(3) {
        let byte = |at: usize| u32::from(chunk.get(at).copied().unwrap_or(0));
        let triple = (byte(0) << 16) | (byte(1) << 8) | byte(2);
        for seat in 0..4u32 {
            if usize::try_from(seat).unwrap_or(4) <= chunk.len() {
                let index = (triple >> (18 - seat * 6)) & 0x3f;
                encoded.push(char::from(ALPHABET[index as usize]));
            } else {
                // A group of one or two bytes is padded out to four seats: the decoder a
                // terminal uses counts seats and drops the padding.
                encoded.push('=');
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    //! What a copy really writes, in bytes: the sequence is a wire format, and a wire
    //! format checked against a re-run of its own encoder would pass whatever it did.

    use super::*;

    #[test]
    fn a_copy_leaves_as_the_sequence_that_carries_words_out() {
        // "sindri" is c2luZHJp in base64, and six bytes are two whole groups: no padding,
        // which is the case a wrong one of the two would hide.
        assert_eq!(
            copy("sindri"),
            b"\x1b]52;c;c2luZHJp\x1b\\".to_vec(),
            "OSC 52, the system clipboard, the words as base64, ST"
        );
    }

    #[test]
    fn the_words_are_padded_out_to_whole_groups_and_survive_their_bytes() {
        // "hi" is aGk= and "why" d3h5, so one and two leftover bytes are both padded out;
        // the newline is the byte a line break really is, encoded and not eaten.
        assert_eq!(base64(b"hi"), "aGk=");
        assert_eq!(base64(b"why"), "d2h5");
        assert_eq!(base64(b"one\ntwo"), "b25lCnR3bw==");
        assert_eq!(base64(b""), "");
    }
}
