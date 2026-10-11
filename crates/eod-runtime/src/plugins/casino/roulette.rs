// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! American roulette layout. Double zero uses the original wire number 100.
use super::*;
pub(super) const MAX_BETS: usize = 160;
const DENOMS: [u32; 5] = [1, 5, 10, 25, 100];
const NAMES: [&str; 14] = [
    "ST8", "SPL", "STR", "COR", "SUC", "LIN", "DOZ", "COL", "ODD", "EVE", "LOW", "HIG", "RED",
    "BLA",
];
const RED: [u8; 18] = [
    1, 3, 5, 7, 9, 12, 14, 16, 18, 19, 21, 23, 25, 27, 30, 32, 34, 36,
];
const BLACK: [u8; 18] = [
    2, 4, 6, 8, 10, 11, 13, 15, 17, 20, 22, 24, 26, 28, 29, 31, 33, 35,
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Bet {
    pub kind: u8,
    pub numbers: Vec<u8>,
    pub chips: [u8; 5],
}
impl Bet {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > 128 {
            return Err(Error::InvalidPluginInput);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| Error::InvalidPluginInput)?;
        let mut fields = text.split('%');
        let amount = fields
            .next()
            .ok_or(Error::InvalidPluginInput)?
            .parse::<u32>()
            .map_err(|_| Error::InvalidPluginInput)?;
        let denom = DENOMS
            .iter()
            .position(|n| *n == amount)
            .ok_or(Error::InvalidPluginInput)?;
        let label = fields.next().ok_or(Error::InvalidPluginInput)?;
        let kind = NAMES
            .iter()
            .position(|n| *n == label)
            .ok_or(Error::InvalidPluginInput)? as u8;
        let mut numbers = Vec::new();
        for field in fields {
            if numbers.len() >= 18 {
                return Err(Error::InvalidPluginInput);
            }
            numbers.push(field.parse::<u8>().map_err(|_| Error::InvalidPluginInput)?);
        }
        if !valid_numbers(kind, &numbers) {
            return Err(Error::InvalidPluginInput);
        }
        let mut chips = [0; 5];
        chips[denom] = 1;
        Ok(Self {
            kind,
            numbers,
            chips,
        })
    }
    pub fn amount(&self) -> u32 {
        self.chips
            .iter()
            .zip(DENOMS)
            .map(|(n, d)| *n as u32 * d)
            .sum()
    }
    pub fn same(&self, other: &Self) -> bool {
        self.kind == other.kind && self.numbers == other.numbers
    }
    pub fn add(&mut self, other: &Self) -> bool {
        if !self.same(other) || self.chips.iter().map(|c| *c as u16).sum::<u16>() >= 20 {
            return false;
        }
        for (i, c) in other.chips.iter().enumerate() {
            self.chips[i] += c;
        }
        true
    }
    pub fn remove(&mut self, other: &Self) -> bool {
        if !self.same(other) || self.chips.iter().zip(other.chips).any(|(a, b)| *a < b) {
            return false;
        }
        for (i, c) in other.chips.iter().enumerate() {
            self.chips[i] -= c;
        }
        true
    }
    pub fn payout(&self, number: u8) -> u32 {
        if !self.numbers.contains(&number) {
            return 0;
        }
        self.amount() * [36, 18, 12, 9, 7, 6, 3, 3, 2, 2, 2, 2, 2, 2][self.kind as usize]
    }
    pub fn save(&self, w: &mut Writer) {
        w.u8(self.kind);
        w.bytes(&self.numbers);
        w.fixed(&self.chips)
    }
    pub fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let b = Self {
            kind: r.u8()?,
            numbers: r.bytes(18)?,
            chips: r
                .take(5)?
                .try_into()
                .map_err(|_| Error::InvalidCheckpoint)?,
        };
        let count = b.chips.iter().map(|n| *n as u16).sum::<u16>();
        if !valid_numbers(b.kind, &b.numbers) || !(1..=20).contains(&count) {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(b)
    }
}
pub(super) fn valid_numbers(kind: u8, n: &[u8]) -> bool {
    if n.iter().any(|n| *n > 36 && *n != 100)
        || n.iter().enumerate().any(|(i, n0)| n[..i].contains(n0))
    {
        return false;
    }
    match kind {
        0 => n.len() == 1,
        1 => {
            n == [0, 100]
                || n == [0, 1]
                || n == [100, 3]
                || n.len() == 2
                    && n[0] > 0
                    && n[0] <= 36
                    && ((!n[0].is_multiple_of(3) && n[0] + 1 == n[1]) || n[0] + 3 == n[1])
        }
        2 => {
            n == [0, 1, 2]
                || n == [0, 100, 2]
                || n == [100, 2, 3]
                || n.len() == 3
                    && n[0] > 0
                    && n[0] % 3 == 1
                    && n.windows(2).all(|w| w[0] + 1 == w[1])
        }
        3 => {
            n.len() == 4
                && n[0] > 0
                && n[0] <= 32
                && !n[0].is_multiple_of(3)
                && n[0] + 1 == n[1]
                && n[0] + 3 == n[2]
                && n[0] + 4 == n[3]
        }
        4 => n == [0, 100, 1, 2, 3],
        5 => {
            n.len() == 6
                && n[0] > 0
                && n[0] <= 31
                && n[0] % 3 == 1
                && n.windows(2).all(|w| w[0] + 1 == w[1])
        }
        6 => n.len() == 12 && [1, 13, 25].contains(&n[0]) && n.windows(2).all(|w| w[0] + 1 == w[1]),
        7 => n.len() == 12 && [1, 2, 3].contains(&n[0]) && n.windows(2).all(|w| w[0] + 3 == w[1]),
        8 => n.len() == 18 && n.iter().enumerate().all(|(i, n)| *n == i as u8 * 2 + 1),
        9 => n.len() == 18 && n.iter().enumerate().all(|(i, n)| *n == i as u8 * 2 + 2),
        10 => n.len() == 18 && n.iter().enumerate().all(|(i, n)| *n == i as u8 + 1),
        11 => n.len() == 18 && n.iter().enumerate().all(|(i, n)| *n == i as u8 + 19),
        12 => n == RED,
        13 => n == BLACK,
        _ => false,
    }
}
pub(super) fn sync<'a>(bets: impl Iterator<Item = &'a Bet>) -> Result<String, Error> {
    use std::fmt::Write;
    let mut count = 0;
    let mut body = String::new();
    for b in bets {
        for d in (0..5).rev() {
            for _ in 0..b.chips[d] {
                if body.len() + 128 > 64 * 1024 {
                    return Err(Error::MessageTooLarge);
                }
                write!(&mut body, "%{}%{}", DENOMS[d], NAMES[b.kind as usize])
                    .map_err(|_| Error::InvalidPluginInput)?;
                for n in &b.numbers {
                    write!(&mut body, "%{n}").map_err(|_| Error::InvalidPluginInput)?;
                }
                count += 1;
            }
        }
    }
    if count == 0 {
        Ok("0%0".to_owned())
    } else {
        Ok(format!("{count}{body}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_rejects_source_loopholes_and_keeps_double_zero_order() {
        for good in [
            "100%ST8%100",
            "1%SPL%0%100",
            "1%STR%0%100%2",
            "5%SUC%0%100%1%2%3",
            "10%COR%32%33%35%36",
        ] {
            assert!(Bet::parse(good.as_bytes()).is_ok(), "{good}")
        }
        for bad in [
            "1%SPL%3%4",
            "1%STR%2%3%4",
            "1%COR%3%4%6%7",
            "1%LIN%2%3%4%5%6%7",
            "1%DOZ%1%2%3%4%5%6%7%8%9%10%11%36",
            "1%COL%1%4%7%10%13%16%19%22%25%28%31%35",
            "1%ODD%1%1%1%1%1%1%1%1%1%1%1%1%1%1%1%1%1%1",
        ] {
            assert!(Bet::parse(bad.as_bytes()).is_err(), "{bad}")
        }
    }
    #[test]
    fn chip_limit_remove_sync_and_payout_include_returned_stake() {
        let chip = Bet::parse(b"5%ST8%7").unwrap();
        let mut b = chip.clone();
        for _ in 1..20 {
            assert!(b.add(&chip));
        }
        assert!(!b.add(&chip));
        assert_eq!(b.payout(7), 3600);
        assert_eq!(b.payout(8), 0);
        assert!(b.remove(&chip));
        assert_eq!(
            sync([&b].into_iter()).unwrap(),
            format!("19{}", "%5%ST8%7".repeat(19))
        );
        assert_eq!(sync(std::iter::empty()).unwrap(), "0%0");
    }
}
