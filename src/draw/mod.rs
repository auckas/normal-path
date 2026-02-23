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

fn rewrite(raw: &mut [u8], init: u8, mut change: impl FnMut(u8) -> Option<u8>) -> u8 {
    let mut last = init;
    for b in raw.iter_mut() {
        if let Some(new) = change(last) {
            *b = new;
            last = new;
        } else {
            last = b'x';
        }
    }

    last
}

mod unix;
mod windows;
#[cfg(unix)]
pub use unix::*;
#[cfg(windows)]
pub use windows::*;
