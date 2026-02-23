#![cfg(unix)]

fn maybe_special(last: u8) -> Option<u8> {
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

fn maybe_norm_special(last: u8) -> Option<u8> {
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

pub fn relative(len: usize) -> String {
    if len == 0 {
        return String::new();
    }

    let mut raw = super::alphanumeric(len).into_bytes();
    let init = match fastrand::bool() {
        true => b'.',
        false => b'x',
    };
    raw[0] = init;

    if let Some(raw) = raw.get_mut(1..) {
        super::rewrite(raw, init, maybe_special);
    }

    String::from_utf8(raw).unwrap()
}

pub fn absolute(len: usize) -> String {
    assert!(len > 0);

    let mut raw = super::alphanumeric(len).into_bytes();
    raw[0] = b'/';

    if let Some(raw) = raw.get_mut(1..) {
        super::rewrite(raw, b'/', maybe_special);
    }

    String::from_utf8(raw).unwrap()
}

pub fn common(len: usize) -> String {
    match fastrand::bool() {
        true => absolute(len),
        false => relative(len),
    }
}

pub fn normal(len: usize) -> String {
    assert!(len > 0);

    let mut raw = super::alphanumeric(len).into_bytes();
    raw[0] = b'/';

    let last = match raw.get_mut(1..) {
        Some(raw) => super::rewrite(raw, b'/', maybe_norm_special),
        None => b'/',
    };

    if len != 1 && matches!(last, b'/' | b'.') {
        raw[len - 1] = b'x';
    }

    String::from_utf8(raw).unwrap()
}
