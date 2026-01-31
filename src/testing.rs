#![cfg(test)]

fn alphanumeric(len: usize) -> String {
    const ALPHANUMERIC: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

    let out = (0..len)
        .map(|_| {
            let idx = fastrand::usize(..ALPHANUMERIC.len());
            ALPHANUMERIC[idx]
        })
        .collect();

    String::from_utf8(out).unwrap()
}

fn path_byte_change(last: u8) -> Option<u8> {
    match last as char {
        '/' => match fastrand::u8(0..6) {
            0..2 => Some(b'.'),
            2..5 => None,
            _ => Some(b'/'),
        },
        '.' => match fastrand::u8(0..3) {
            0 => Some(b'.'),
            1 => None,
            _ => Some(b'/'),
        },
        _ => match fastrand::u8(0..6) {
            0 => Some(b'/'),
            _ => None,
        },
    }
}

fn repath_byte_change(last: u8) -> Option<u8> {
    match last as char {
        '/' => match fastrand::u8(0..3) {
            0 => Some(b'.'),
            _ => None,
        },
        '.' => match fastrand::bool() {
            true => Some(b'.'),
            false => None,
        },
        _ => match fastrand::u8(0..6) {
            0 => Some(b'/'),
            _ => None,
        },
    }
}

pub fn draw_rel(len: usize) -> String {
    assert!(len > 0);

    let mut raw = alphanumeric(len).into_bytes();
    let mut last = match fastrand::bool() {
        true => b'.',
        false => b'x',
    };
    raw[0] = last;

    for b in raw.iter_mut().skip(1) {
        if let Some(change) = path_byte_change(last) {
            *b = change;
            last = change;
        } else {
            last = b'x';
        }
    }

    String::from_utf8(raw).unwrap()
}

pub fn draw_abs(len: usize) -> String {
    assert!(len > 0);

    let mut raw = alphanumeric(len).into_bytes();
    raw[0] = b'/';

    let mut last = b'/';
    for b in raw.iter_mut().skip(1) {
        if let Some(change) = path_byte_change(last) {
            *b = change;
            last = change;
        } else {
            last = b'x';
        }
    }

    String::from_utf8(raw).unwrap()
}

pub fn draw_any(len: usize) -> String {
    match fastrand::bool() {
        true => draw_abs(len),
        false => draw_rel(len),
    }
}

pub fn draw_re(len: usize) -> String {
    assert!(len > 0);

    let mut raw = alphanumeric(len).into_bytes();
    raw[0] = b'/';

    let mut last = b'/';
    for b in raw.iter_mut().skip(1) {
        if let Some(change) = repath_byte_change(last) {
            *b = change;
            last = change;
        } else {
            last = b'x';
        }
    }

    if len != 1 && matches!(last, b'/' | b'.') {
        raw[len - 1] = b'x';
    }

    String::from_utf8(raw).unwrap()
}
