//! Account recovery codes: one-time codes a player keeps, which let them set a
//! new password without e-mail. Only SHA-256 hashes are stored; the codes have
//! 50 bits of randomness, so a fast hash is enough (they are not passwords).

use sha2::{Digest, Sha256};

/// How many codes a player gets each time they are generated.
pub const CODES_PER_SET: usize = 8;

/// Crockford's base32: no `I`, `L`, `O` or `U`, so codes survive being copied
/// by hand.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CODE_LEN: usize = 10;

/// A new random code, shown as `XXXXX-XXXXX`.
pub fn generate() -> String {
    let mut bytes = [0_u8; CODE_LEN];
    getrandom::fill(&mut bytes).expect("the OS random number generator is unavailable");
    let chars: Vec<char> = bytes.iter().map(|b| ALPHABET[(b & 31) as usize] as char).collect();
    format!(
        "{}-{}",
        chars[..CODE_LEN / 2].iter().collect::<String>(),
        chars[CODE_LEN / 2..].iter().collect::<String>()
    )
}

/// What a player typed, reduced to the canonical 10 characters: any case,
/// with or without the dash or spaces, and the usual look-alikes (`O`, `I`,
/// `L`) read as the digits they resemble. `None` if it cannot be a code.
pub fn normalize(input: &str) -> Option<String> {
    let code: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect();
    (code.len() == CODE_LEN && code.bytes().all(|b| ALPHABET.contains(&b))).then_some(code)
}

/// The value stored and looked up for a canonical code.
pub fn hash(code: &str) -> Vec<u8> {
    Sha256::digest(code.as_bytes()).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_codes_have_the_expected_shape_and_are_different() {
        let a = generate();
        let b = generate();
        assert_eq!(a.len(), CODE_LEN + 1);
        assert_eq!(a.as_bytes()[5], b'-');
        assert_ne!(a, b);
        assert!(normalize(&a).is_some(), "what we generate must be accepted back");
    }

    #[test]
    fn typing_tolerance_does_not_change_the_hash() {
        let canonical = normalize("ABCDE-12345").unwrap();
        for typed in ["abcde-12345", "ABCDE12345", " abcde 12345 ", "ABCDE-1234 5"] {
            assert_eq!(normalize(typed).as_deref(), Some(canonical.as_str()), "{typed}");
        }
        // Look-alikes read as the digits.
        assert_eq!(normalize("OOOOO-IIIII"), normalize("00000-11111"));
        assert_eq!(normalize("LLLLL-lllll"), normalize("11111-11111"));
        assert_eq!(hash(&canonical), hash("ABCDE12345"));
    }

    #[test]
    fn anything_that_is_not_a_code_is_refused() {
        for bad in [
            "",
            "ABCDE",
            "ABCDE-1234",
            "ABCDE-123456",
            "ABCDE-1234U",
            "ABCDE-1234!",
            "ñññññ-ñññññ",
        ] {
            assert_eq!(normalize(bad), None, "{bad}");
        }
    }
}
