//! How good a passphrase is, following the usual public guidance (for example
//! the German BSI): long enough, several kinds of characters, and no
//! predictable patterns or personal details.
//!
//! This is advice only. Any passphrase that is not empty is accepted.

/// The rating shown to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rating {
    Weak,
    Fair,
    Good,
}

/// Which of the criteria a passphrase meets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Assessment {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub special: bool,
    /// A keyboard row such as "qwertz" or "asdf".
    pub keyboard_pattern: bool,
    /// Running letters or digits such as "12345" or "abcd", or one character repeated.
    pub sequence: bool,
    /// A year, a date, or a very common password word.
    pub predictable: bool,
    /// Contains a given personal detail (user or computer name).
    pub personal: bool,
}

/// Long enough on its own ("at least 12 to 16 characters").
pub const GOOD_LENGTH: usize = 16;
pub const FAIR_LENGTH: usize = 12;
/// Long passphrases need fewer kinds of characters (BSI: 20+ characters with two kinds).
const LONG_LENGTH: usize = 20;

impl Assessment {
    pub fn classes(&self) -> usize {
        [self.lowercase, self.uppercase, self.digits, self.special]
            .iter()
            .filter(|&&b| b)
            .count()
    }

    pub fn has_pattern(&self) -> bool {
        self.keyboard_pattern || self.sequence || self.predictable || self.personal
    }

    pub fn rating(&self) -> Rating {
        let length_points = if self.length >= GOOD_LENGTH {
            2
        } else if self.length >= FAIR_LENGTH {
            1
        } else {
            0
        };
        let classes = self.classes();
        let variety_points = if classes == 4 || (self.length >= LONG_LENGTH && classes >= 2) {
            2
        } else if classes == 3 {
            1
        } else {
            0
        };
        let mut points: u8 = length_points + variety_points;
        if self.has_pattern() {
            points = points.saturating_sub(1).min(3);
        }
        match points {
            4 => Rating::Good,
            2 | 3 => Rating::Fair,
            _ => Rating::Weak,
        }
    }
}

const KEYBOARD_ROWS: [&str; 6] = [
    "1234567890",
    "qwertzuiopü",
    "qwertyuiop",
    "asdfghjklöä",
    "yxcvbnm",
    "zxcvbnm",
];

const COMMON_WORDS: [&str; 14] = [
    "password",
    "passwort",
    "letmein",
    "welcome",
    "willkommen",
    "admin",
    "hallo",
    "hello",
    "secret",
    "geheim",
    "iloveyou",
    "master",
    "login",
    "sommer",
];

pub fn assess(passphrase: &str, personal: &[&str]) -> Assessment {
    let lower = passphrase.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    Assessment {
        length: passphrase.chars().count(),
        lowercase: passphrase.chars().any(char::is_lowercase),
        uppercase: passphrase.chars().any(char::is_uppercase),
        digits: passphrase.chars().any(|c| c.is_ascii_digit()),
        // Spaces between words count as special characters, too.
        special: passphrase.chars().any(|c| !c.is_alphanumeric()),
        keyboard_pattern: keyboard_pattern(&chars),
        sequence: sequence(&chars),
        predictable: predictable(&lower),
        personal: personal
            .iter()
            .map(|p| p.trim().to_lowercase())
            .filter(|p| p.chars().count() >= 3)
            .any(|p| lower.contains(&p)),
    }
}

/// Four or more neighbouring keys of one keyboard row, in either direction.
fn keyboard_pattern(chars: &[char]) -> bool {
    const RUN: usize = 4;
    if chars.len() < RUN {
        return false;
    }
    chars.windows(RUN).any(|window| {
        let text: String = window.iter().collect();
        let reversed: String = window.iter().rev().collect();
        KEYBOARD_ROWS
            .iter()
            .any(|row| row.contains(&text) || row.contains(&reversed))
    })
}

/// Four or more running letters/digits ("1234", "dcba") or one character four times.
fn sequence(chars: &[char]) -> bool {
    const RUN: usize = 4;
    chars.windows(RUN).any(|w| {
        let alnum = w.iter().all(|c| c.is_ascii_alphanumeric());
        let steps: Vec<i32> = w.windows(2).map(|p| p[1] as i32 - p[0] as i32).collect();
        let same = steps.iter().all(|&s| s == 0);
        let up = steps.iter().all(|&s| s == 1);
        let down = steps.iter().all(|&s| s == -1);
        same || (alnum && (up || down))
    })
}

/// A year from 1900 to 2099, a date like 24.12. or 2412, or a common password word.
fn predictable(lower: &str) -> bool {
    if COMMON_WORDS.iter().any(|w| lower.contains(w)) {
        return true;
    }
    let number = |digits: &str| digits.parse::<u32>().unwrap_or(0);
    digit_runs(lower).iter().any(|run| {
        let year = run
            .as_bytes()
            .windows(4)
            .any(|w| std::str::from_utf8(w).is_ok_and(|w| (1900..=2099).contains(&number(w))));
        // DDMM, DDMMYY or DDMMYYYY
        let date = matches!(run.len(), 4 | 6 | 8) && looks_like_day_month(number(&run[..4]));
        year || date
    }) || has_written_date(lower)
}

fn looks_like_day_month(n: u32) -> bool {
    let (day, month) = (n / 100, n % 100);
    (1..=31).contains(&day) && (1..=12).contains(&month)
}

/// "24.12.", "24-12", "24/12" and the like.
fn has_written_date(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    bytes.windows(5).any(|w| {
        let digit = |b: u8| b.is_ascii_digit();
        digit(w[0])
            && digit(w[1])
            && matches!(w[2], b'.' | b'-' | b'/')
            && digit(w[3])
            && digit(w[4])
            && looks_like_day_month(
                u32::from(w[0] - b'0') * 1000
                    + u32::from(w[1] - b'0') * 100
                    + u32::from(w[3] - b'0') * 10
                    + u32::from(w[4] - b'0'),
            )
    })
}

fn digit_runs(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .collect()
}

/// User and computer name, which should not appear in a passphrase.
pub fn personal_details() -> Vec<String> {
    ["USERNAME", "COMPUTERNAME"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .filter(|v| !v.trim().is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(p: &str) -> Rating {
        assess(p, &["Anna"]).rating()
    }

    #[test]
    fn short_or_simple_passphrases_are_weak() {
        assert_eq!(rate("123"), Rating::Weak);
        assert_eq!(rate("hallo"), Rating::Weak);
        assert_eq!(rate("qwertz123"), Rating::Weak);
        assert_eq!(rate("Sommer2024!"), Rating::Weak);
    }

    #[test]
    fn criteria_raise_the_rating() {
        // 14 characters, four kinds, no pattern.
        assert_eq!(rate("Kx9!mPv2#qLr7w"), Rating::Fair);
        // 16+ characters and four kinds.
        assert_eq!(rate("Kx9!mPv2#qLr7wTe"), Rating::Good);
        // Long passphrase with two kinds of characters.
        assert_eq!(rate("rabe ofen wolke tinte"), Rating::Good);
        // Same length, but with a keyboard row it drops.
        assert_eq!(rate("Kx9!qwertz#qLr7wTe"), Rating::Fair);
    }

    #[test]
    fn detects_patterns() {
        let a = assess("my-asdf-pass", &[]);
        assert!(a.keyboard_pattern);
        assert!(assess("xx4321yy", &[]).sequence);
        assert!(assess("aaaa", &[]).sequence);
        assert!(assess("born1987", &[]).predictable);
        assert!(assess("am 24.12. geboren", &[]).predictable);
        assert!(assess("AnnaSecure", &["anna"]).personal);
        assert!(!assess("Kx9!mPv2#qLr7wTe", &["anna"]).has_pattern());
    }
}
