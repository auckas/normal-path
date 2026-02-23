#![cfg(windows)]

fn maybe_special(last: u8) -> Option<u8> {
    match last as char {
        '/' | '\\' => match fastrand::u8(0..12) {
            0..4 => Some(b'.'),
            4..10 => None,
            10 => Some(b'/'),
            _ => Some(b'\\'),
        },
        '.' => match fastrand::u8(0..6) {
            0 | 1 => Some(b'.'),
            2 | 3 => None,
            4 => Some(b'/'),
            _ => Some(b'\\'),
        },
        _ => match fastrand::u8(0..12) {
            0 => Some(b'/'),
            1 => Some(b'\\'),
            _ => None,
        },
    }
}

fn maybe_norm_special(last: u8) -> Option<u8> {
    match last as char {
        '\\' => match fastrand::u8(0..3) {
            0 => Some(b'.'),
            _ => None,
        },
        '.' => match fastrand::bool() {
            true => Some(b'.'),
            false => None,
        },
        _ => match fastrand::u8(0..6) {
            0 => Some(b'\\'),
            _ => None,
        },
    }
}

fn any_slash() -> u8 {
    match fastrand::bool() {
        true => b'/',
        false => b'\\',
    }
}

fn any_letter() -> u8 {
    let number = fastrand::u8(0..52);
    if number < 26 {
        b'A' + number
    } else {
        b'a' + (number - 26)
    }
}

pub fn relative(len: usize) -> String {
    let mut raw = super::alphanumeric(len).into_bytes();
    let start = if len > 1 && fastrand::bool() {
        raw[0] = any_letter();
        raw[1] = b':';
        2
    } else {
        0
    };

    if start != len {
        let init = match fastrand::bool() {
            true => b'.',
            false => b'x',
        };
        raw[start] = init;

        if let Some(raw) = raw.get_mut(start + 1..) {
            super::rewrite(raw, init, maybe_special);
        }
    }

    String::from_utf8(raw).unwrap()
}

pub fn absolute(len: usize) -> String {
    assert!(len > 2);

    enum Choice {
        Absolute,
        Device,
        Unc,
    }
    use Choice::*;

    let mut raw = super::alphanumeric(len).into_bytes();
    let choice = match fastrand::u8(0..3) {
        0 if len > 5 => Unc,
        1 if len > 4 => Device,
        _ => Absolute,
    };

    let start = match choice {
        Absolute => {
            raw[0] = any_letter();
            raw[1] = b':';
            raw[2] = any_slash();
            3
        }
        Device => {
            raw[0] = any_slash();
            raw[1] = any_slash();
            raw[2] = b'.';
            raw[3] = any_slash();
            5
        }
        Unc => {
            raw[0] = any_slash();
            raw[1] = any_slash();

            let slash2 = fastrand::usize(5..len);
            let slash1 = fastrand::usize(3..(slash2 - 1));
            raw[slash1] = any_slash();
            raw[slash2] = any_slash();
            slash2 + 1
        }
    };

    let init = raw[start - 1];
    if let Some(raw) = raw.get_mut(start..) {
        super::rewrite(raw, init, maybe_special);
    }

    String::from_utf8(raw).unwrap()
}

pub fn common(len: usize) -> String {
    match len > 2 && fastrand::bool() {
        true => absolute(len),
        false => relative(len),
    }
}

pub fn verbatim(len: usize) -> String {
    assert!(len > 6);

    let mut prefix = String::with_capacity(len);
    prefix.push_str(r"\\?\");

    prefix + &absolute(len - 4)
}

pub fn normal(len: usize) -> String {
    assert!(len > 2);

    enum Choice {
        Absolute,
        Device,
        Unc,
    }
    use Choice::*;

    let mut raw = super::alphanumeric(len).into_bytes();
    let choice = match fastrand::u8(0..3) {
        0 if len > 5 => Unc,
        1 if len > 4 => Device,
        _ => Absolute,
    };

    let start = match choice {
        Absolute => {
            raw[0] = fastrand::u8(b'A'..=b'Z');
            raw[1] = b':';
            raw[2] = b'\\';
            3
        }
        Device => {
            raw[0] = b'\\';
            raw[1] = b'\\';
            raw[2] = b'.';
            raw[3] = b'\\';
            5
        }
        Unc => {
            raw[0] = b'\\';
            raw[1] = b'\\';

            let slash2 = fastrand::usize(5..len);
            let slash1 = fastrand::usize(3..(slash2 - 1));
            raw[slash1] = b'\\';

            if slash2 != len - 1 {
                raw[slash2] = b'\\';
                slash2 + 1
            } else {
                len
            }
        }
    };

    let init = raw[start - 1];
    let last = match raw.get_mut(start..) {
        Some(raw) => super::rewrite(raw, init, maybe_norm_special),
        None => init,
    };

    if len != start && matches!(last, b'\\' | b'.') {
        raw[len - 1] = b'x';
    }

    String::from_utf8(raw).unwrap()
}

pub fn root_only(len: usize) -> String {
    assert!(len > 0);

    let mut raw = super::alphanumeric(len).into_bytes();
    raw[0] = any_slash();

    if let Some(raw) = raw.get_mut(1..) {
        super::rewrite(raw, b'\\', maybe_special);
    }

    String::from_utf8(raw).unwrap()
}

pub fn disk_only() -> String {
    let mut raw = [0; 2];
    raw[0] = any_letter();
    raw[1] = b':';
    String::from_utf8(raw.to_vec()).unwrap()
}
