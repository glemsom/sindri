use sindri_core::records::Status;

/// `OSC 52 ; c ; <base64> ST`: the terminal at the far end owns the clipboard, so a copy
/// needs no `xclip` and works over SSH. ST rather than BEL, so no terminal rings on a copy.
pub fn copy(text: &str) -> Vec<u8> {
    let mut sequence = Vec::from(&b"\x1b]52;c;"[..]);
    sequence.extend_from_slice(base64(text.as_bytes()).as_bytes());
    sequence.extend_from_slice(b"\x1b\\");
    sequence
}

/// `OSC 66 ; s=<scale> ; <words> ST`: the words drawn over `scale` cells each way, so a heading of
/// them stands twice the cell high and wide. The metadata names the scale alone, because a terminal
/// splits the words into cells as it would without the sequence, and the row was measured so.
pub fn sized(scale: u16, words: &str) -> Vec<u8> {
    sequence(format!("\x1b]66;s={scale};").as_bytes(), words)
}

/// `OSC 8 ; ; <destination> ST`: the cells written after it are a way to somewhere until the
/// same sequence with no destination closes them. A destination is words a wire wrote, so no
/// glyph of it is written as part of the sequence it is carried in.
pub fn link(destination: &str) -> Vec<u8> {
    sequence(b"\x1b]8;;", destination)
}

/// `OSC 2 ; <workspace> — <state> ST`: the title the terminal wears while a launch holds it, so a
/// reader with six windows open can tell which one a run is working in.
pub fn title(workspace: &str, state: &str) -> Vec<u8> {
    sequence(b"\x1b]2;", &format!("{workspace} \u{2014} {state}"))
}

/// `OSC 9 ; 4 ; 3 ST` for a run that is streaming and `; 0` for one that is not: the terminal's own
/// progress mark, indeterminate because a run's turns are a count and never a share of anything.
pub fn progress(marked: bool) -> Vec<u8> {
    match marked {
        true => MARK.to_vec(),
        false => CLEAR.to_vec(),
    }
}

/// `OSC 9 ; <words> ST`: the notification a terminal hands the desktop, which draws it beside a
/// reader who is looking at something else.
pub fn notification(words: &str) -> Vec<u8> {
    // The mark shares this sequence and is told apart by the state digit after it, so a body that
    // opened with one would be read as a mark: the space is what keeps the two apart.
    let lead = match words.starts_with(|glyph: char| glyph.is_ascii_digit()) {
        true => " ",
        false => "",
    };
    sequence(b"\x1b]9;", &format!("{lead}{words}"))
}

const MARK: &[u8] = b"\x1b]9;4;3\x1b\\";

const CLEAR: &[u8] = b"\x1b]9;4;0\x1b\\";

/// A sequence's opening, the words it carries, and `ST` after them. The words are filtered of every
/// control glyph, because a directory named with an `ESC` would otherwise end the sequence it is
/// carried in and begin one of its own.
fn sequence(opening: &[u8], words: &str) -> Vec<u8> {
    let mut sequence = Vec::from(opening);
    sequence.extend(
        words
            .chars()
            .filter(|glyph| !glyph.is_control())
            .collect::<String>()
            .bytes(),
    );
    sequence.extend_from_slice(b"\x1b\\");
    sequence
}

/// `OSC 7501 ; <document> ST`, the document being the JSON `status.json` holds: a reader takes
/// it whole after the single `;`, so a workspace named `weird;dir` cannot split it. Written
/// unqueried and always, because a detection query needs a stdin read a filter does not do.
pub fn status(report: &Status) -> Vec<u8> {
    let mut sequence = Vec::from(&b"\x1b]7501;"[..]);
    sequence.extend_from_slice(&report.json());
    sequence.extend_from_slice(b"\x1b\\");
    sequence
}

fn base64(text: &[u8]) -> String {
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
                // A group of one or two bytes is padded to four seats: the decoder counts seats.
                encoded.push('=');
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests;
