//! The secret each account keeps (Claude's refresh token), in the system's
//! credential store
//! (Windows' Credential Manager, the Secret Service on Linux), as Flow keeps
//! its sign-ins: split over several entries where it's long, since Windows
//! holds 2,560 bytes an entry, kept as UTF-16.

use crate::APP_ID;

/// How much of a long secret goes in each entry.
const PIECE: usize = 1000;
/// At most so many pieces are read, written or forgotten.
const MOST_PIECES: usize = 32;

/// The credential store's account for the secret of the account with
/// `key` (`Account::key`: its provider's and its own).
pub fn token_account(key: &str) -> String {
    format!("token-{key}")
}

/// The account of a long secret's piece `n`: its own name for the first.
fn piece(account: &str, n: usize) -> String {
    if n == 0 {
        account.to_string()
    } else {
        format!("{account}.{}", n + 1)
    }
}

/// The pieces `text` is kept in.
fn pieces(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars.chunks(PIECE).map(|c| c.iter().collect()).collect()
}

/// A long secret kept under `account`, put back together from its pieces;
/// `Ok(None)` where there's none, `Err` where the store can't be read.
pub fn load_long(account: &str) -> Result<Option<String>, String> {
    let mut text = String::new();
    for n in 0..MOST_PIECES {
        let entry = keyring::Entry::new(APP_ID, &piece(account, n)).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(part) => text.push_str(&part),
            Err(keyring::Error::NoEntry) => break,
            Err(err) => return Err(err.to_string()),
        }
    }
    Ok((!text.is_empty()).then_some(text))
}

/// Keeps `text` under `account`, in pieces where it's long, the pieces of a
/// longer one before taken away.
pub fn store_long(account: &str, text: &str) -> Result<(), String> {
    let pieces = pieces(text);
    if pieces.len() > MOST_PIECES {
        return Err(format!("{account} is too long to keep"));
    }
    for (n, part) in pieces.iter().enumerate() {
        let entry = keyring::Entry::new(APP_ID, &piece(account, n)).map_err(|e| e.to_string())?;
        entry.set_password(part).map_err(|e| e.to_string())?;
    }
    forget_pieces(account, pieces.len())
}

/// Forgets a long secret, every piece of it.
pub fn forget_long(account: &str) -> Result<(), String> {
    forget_pieces(account, 0)
}

fn forget_pieces(account: &str, from: usize) -> Result<(), String> {
    for n in from..MOST_PIECES {
        let entry = keyring::Entry::new(APP_ID, &piece(account, n)).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) => {}
            Err(keyring::Error::NoEntry) => break,
            Err(err) => return Err(err.to_string()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_secret_is_kept_in_pieces_that_fit_an_entry_named_in_order() {
        let text = "é".repeat(2500);
        let parts = pieces(&text);
        assert_eq!(parts.iter().map(|p| p.chars().count()).collect::<Vec<_>>(), [1000, 1000, 500]);
        assert!(parts.iter().all(|p| p.encode_utf16().count() * 2 <= 2560), "Windows' 2,560 bytes an entry");
        assert_eq!(parts.concat(), text);
        assert_eq!((piece("refresh-x", 0), piece("refresh-x", 1), piece("refresh-x", 2)), ("refresh-x".into(), "refresh-x.2".into(), "refresh-x.3".into()));
        assert!(pieces("").is_empty());
    }
}
