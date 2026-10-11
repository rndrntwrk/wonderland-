// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Source card names, circular shoes, blackjack rules, and bounded poker ranks.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Card(pub u8);
impl Card {
    pub fn rank(self) -> u8 {
        self.0 % 13 + 1
    }
    pub fn suit(self) -> u8 {
        self.0 / 13
    }
    pub fn value(self) -> u8 {
        self.rank().min(10)
    }
    pub fn name(self) -> String {
        format!(
            "{}_{}",
            rank_name(self.rank()),
            ["Clubs", "Diamonds", "Hearts", "Spades"][self.suit() as usize]
        )
    }
}
pub(super) fn rank_name(rank: u8) -> &'static str {
    [
        "Ace", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten", "Jack",
        "Queen", "King",
    ][(if rank == 14 { 1 } else { rank }) as usize - 1]
}

#[derive(Clone)]
pub(super) struct Rng {
    state: u64,
    draws: u64,
}
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed,
            draws: 0,
        }
    }
    pub fn below(&mut self, upper: usize) -> Result<usize, Error> {
        if upper == 0 || upper > 4096 {
            return Err(Error::InvalidPluginInput);
        }
        let b = upper as u64;
        let threshold = b.wrapping_neg() % b;
        for _ in 0..32 {
            self.draws = self
                .draws
                .checked_add(1)
                .filter(|x| *x != u64::MAX)
                .ok_or(Error::CounterExhausted)?;
            self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
            let mut x = self.state;
            x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
            x ^= x >> 31;
            if x >= threshold {
                return Ok((x % b) as usize);
            }
        }
        Err(Error::InvalidPluginInput)
    }
    pub fn save(&self, w: &mut Writer) {
        w.u64(self.state);
        w.u64(self.draws)
    }
    pub fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let s = Self {
            state: r.u64()?,
            draws: r.u64()?,
        };
        if s.draws == u64::MAX {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(s)
    }
}
#[derive(Clone)]
pub(super) struct Shoe {
    pub cards: Vec<Card>,
    pub at: usize,
}
impl Shoe {
    pub fn new(decks: usize, rng: &mut Rng) -> Result<Self, Error> {
        let mut s = Self {
            cards: (0..52 * decks).map(|n| Card((n % 52) as u8)).collect(),
            at: 0,
        };
        s.shuffle(rng, 2)?;
        Ok(s)
    }
    pub fn draw(&mut self) -> Card {
        let c = self.cards[self.at];
        self.at = (self.at + 1) % self.cards.len();
        c
    }
    pub fn shuffle(&mut self, rng: &mut Rng, iterations: usize) -> Result<(), Error> {
        self.cards.rotate_left(self.at);
        self.at = 0;
        for _ in 0..iterations {
            for i in (1..self.cards.len()).rev() {
                let j = rng.below(i + 1)?;
                self.cards.swap(i, j);
            }
        }
        Ok(())
    }
    pub fn save(&self, w: &mut Writer) {
        w.u32(self.at as u32);
        w.bytes(&self.cards.iter().map(|c| c.0).collect::<Vec<_>>())
    }
    pub fn restore(r: &mut Reader<'_>, decks: usize) -> Result<Self, Error> {
        let at = r.u32()? as usize;
        let bytes = r.bytes(312)?;
        if bytes.len() != 52 * decks || at >= bytes.len() {
            return Err(Error::InvalidCheckpoint);
        }
        let mut counts = [0; 52];
        for c in &bytes {
            let n = counts
                .get_mut(*c as usize)
                .ok_or(Error::InvalidCheckpoint)?;
            *n += 1;
        }
        if counts.iter().any(|n| *n != decks) {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(Self {
            cards: bytes.into_iter().map(Card).collect(),
            at,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Poker {
    pub category: u8,
    pub ranks: [u8; 5],
    pub suit: u8,
}
impl Poker {
    pub fn score(&self) -> (u8, [u8; 5]) {
        (self.category, self.ranks)
    }
    pub fn qualified(&self) -> bool {
        self.category > 1 || (self.category == 1 && self.ranks[0] >= 4)
    }
    pub fn side_payout(&self, bet: u32) -> u32 {
        let ratio = match self.category {
            5..=8 => 25,
            2..=4 => 7,
            1 if self.ranks[0] == 14 => 7,
            _ => 0,
        };
        if ratio > 0 { bet * (ratio + 1) } else { 0 }
    }
    pub fn payout(&self, dealer: &Self, ante: u32, call: u32) -> (u32, char) {
        if dealer.qualified() && self.score() < dealer.score() {
            return (0, ' ');
        }
        if dealer.qualified() && self.score() == dealer.score() {
            return (ante + call, 'p');
        }
        let ratio = match self.category {
            8 => 25,
            7 => 12,
            6 => 3,
            5 => 2,
            _ => 1,
        };
        (
            (ratio + 1) * ante + if dealer.qualified() { call * 2 } else { call },
            if dealer.qualified() { ' ' } else { 'q' },
        )
    }
    pub fn description(&self) -> Vec<String> {
        let label = [
            "HighCard",
            "Pair",
            "TwoPair",
            "Trips",
            "Straight",
            "Flush",
            "FullHouse",
            "FourOfAKind",
            "StraightFlush",
        ][self.category as usize];
        let mut out = vec![label.to_owned()];
        if self.category == 5 || self.category == 8 {
            out.push(["Clubs", "Diamonds", "Hearts", "Spades"][self.suit as usize].to_owned())
        } else {
            out.push(rank_name(self.ranks[0]).to_owned());
            if self.category == 2 || self.category == 6 {
                out.push(rank_name(self.ranks[1]).to_owned())
            }
        }
        out
    }
}
pub(super) fn poker(cards: &[Card]) -> Result<Poker, Error> {
    if !(5..=7).contains(&cards.len()) || cards.iter().any(|c| c.0 >= 52) {
        return Err(Error::InvalidPluginInput);
    }
    for (i, c) in cards.iter().enumerate() {
        if cards[..i].contains(c) {
            return Err(Error::InvalidPluginInput);
        }
    }
    let mut best = None;
    for a in 0..cards.len() - 4 {
        for b in a + 1..cards.len() - 3 {
            for c in b + 1..cards.len() - 2 {
                for d in c + 1..cards.len() - 1 {
                    for e in d + 1..cards.len() {
                        let rank = five([cards[a], cards[b], cards[c], cards[d], cards[e]]);
                        if best
                            .as_ref()
                            .is_none_or(|old: &Poker| rank.score() > old.score())
                        {
                            best = Some(rank)
                        }
                    }
                }
            }
        }
    }
    best.ok_or(Error::InvalidPluginInput)
}
fn five(cards: [Card; 5]) -> Poker {
    let mut counts = [0u8; 15];
    let mut sorted = [0u8; 5];
    for (i, c) in cards.iter().enumerate() {
        let r = if c.rank() == 1 { 14 } else { c.rank() };
        counts[r as usize] += 1;
        sorted[i] = r;
    }
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let flush = cards.iter().all(|c| c.suit() == cards[0].suit());
    let straight = if sorted == [14, 5, 4, 3, 2] {
        5
    } else if sorted.windows(2).all(|p| p[0] == p[1] + 1) {
        sorted[0]
    } else {
        0
    };
    let mut groups: Vec<(u8, u8)> = (2..=14)
        .filter_map(|r| (counts[r] > 0).then_some((counts[r], r as u8)))
        .collect();
    groups.sort_unstable_by(|a, b| b.cmp(a));
    let mut ranks = [0; 5];
    let category = if flush && straight > 0 {
        ranks[0] = straight;
        8
    } else if groups[0].0 == 4 {
        ranks[0] = groups[0].1;
        ranks[1] = groups[1].1;
        7
    } else if groups[0].0 == 3 && groups[1].0 == 2 {
        ranks[0] = groups[0].1;
        ranks[1] = groups[1].1;
        6
    } else if flush {
        ranks = sorted;
        5
    } else if straight > 0 {
        ranks[0] = straight;
        4
    } else if groups[0].0 == 3 {
        for (i, g) in groups.iter().enumerate() {
            ranks[i] = g.1;
        }
        3
    } else if groups[0].0 == 2 && groups[1].0 == 2 {
        for (i, g) in groups.iter().enumerate() {
            ranks[i] = g.1;
        }
        2
    } else if groups[0].0 == 2 {
        for (i, g) in groups.iter().enumerate() {
            ranks[i] = g.1;
        }
        1
    } else {
        ranks = sorted;
        0
    };
    Poker {
        category,
        ranks,
        suit: if flush { cards[0].suit() } else { 0 },
    }
}

#[derive(Clone, Debug)]
pub(super) struct BjHand {
    pub cards: Vec<Card>,
    pub kind: u8,
}
impl BjHand {
    pub fn new(a: Card, b: Card) -> Self {
        let kind = if (a.rank() == 1 && b.value() == 10) || (b.rank() == 1 && a.value() == 10) {
            21
        } else if a.value() == b.value() {
            20
        } else {
            2
        };
        Self {
            cards: vec![a, b],
            kind,
        }
    }
    pub fn total(&self) -> (u8, bool) {
        let mut sum = self.cards.iter().map(|c| c.value()).sum::<u8>();
        let soft = self.cards.iter().any(|c| c.rank() == 1) && sum <= 11;
        if soft {
            sum += 10;
        }
        (sum, soft)
    }
    pub fn playable(&self) -> bool {
        matches!(self.kind, 1 | 2 | 20)
    }
    pub fn stand(&mut self) {
        if self.playable() {
            self.kind = 3
        }
    }
    pub fn hit(&mut self, c: Card, double: bool) -> Result<(), Error> {
        if self.cards.len() >= 22 {
            return Err(Error::InvalidPluginInput);
        }
        self.cards.push(c);
        self.kind = match (self.total().0 > 21, double) {
            (true, true) => 5,
            (false, true) => 4,
            (true, false) => 22,
            (false, false) => 1,
        };
        Ok(())
    }
    pub fn names(&self) -> Vec<String> {
        self.cards.iter().map(|c| c.name()).collect()
    }
    pub fn payout(&self, dealer: &Self, bet: u32) -> u32 {
        if dealer.kind == 21 {
            return if self.kind == 21 { bet } else { 0 };
        }
        let (total, _) = self.total();
        let (d, _) = dealer.total();
        if total > 21 {
            return 0;
        }
        if self.kind == 21 {
            return bet + bet * 3 / 2;
        }
        if d > 21 || total > d {
            return if self.kind == 4 {
                4 * bet
            } else if self.kind == 3 {
                2 * bet
            } else {
                0
            };
        }
        if d == total {
            return if self.kind == 4 { 2 * bet } else { bet };
        }
        0
    }
    pub fn save(&self, w: &mut Writer) {
        w.u8(self.kind);
        w.bytes(&self.cards.iter().map(|c| c.0).collect::<Vec<_>>())
    }
    pub fn restore(r: &mut Reader<'_>) -> Result<Self, Error> {
        let kind = r.u8()?;
        let bytes = r.bytes(22)?;
        if bytes.len() < 2
            || bytes.iter().any(|c| *c >= 52)
            || !matches!(kind, 1 | 2 | 3 | 4 | 5 | 20 | 21 | 22)
        {
            return Err(Error::InvalidCheckpoint);
        }
        let h = Self {
            kind,
            cards: bytes.into_iter().map(Card).collect(),
        };
        let total = h.total().0;
        if (matches!(kind, 5 | 22)) != (total > 21)
            || matches!(kind, 2 | 20 | 21)
                && (h.cards.len() != 2 || Self::new(h.cards[0], h.cards[1]).kind != kind)
            || matches!(kind, 4 | 5) && h.cards.len() != 3
        {
            return Err(Error::InvalidCheckpoint);
        }
        Ok(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn c(rank: u8, suit: u8) -> Card {
        Card(suit * 13 + rank - 1)
    }
    #[test]
    fn poker_literals_cover_categories_and_ties() {
        let cases = [
            ([c(1, 0), c(13, 0), c(12, 0), c(11, 0), c(10, 0)], 8, 14),
            ([c(9, 0), c(9, 1), c(9, 2), c(9, 3), c(1, 0)], 7, 9),
            ([c(4, 0), c(4, 1), c(4, 2), c(13, 0), c(13, 1)], 6, 4),
            ([c(1, 0), c(3, 0), c(6, 0), c(9, 0), c(11, 0)], 5, 14),
            ([c(1, 0), c(2, 1), c(3, 2), c(4, 3), c(5, 0)], 4, 5),
            ([c(3, 0), c(3, 1), c(3, 2), c(4, 3), c(5, 0)], 3, 3),
            ([c(3, 0), c(3, 1), c(12, 2), c(12, 3), c(5, 0)], 2, 12),
            ([c(1, 0), c(1, 1), c(12, 2), c(8, 3), c(5, 0)], 1, 14),
            ([c(1, 0), c(2, 1), c(12, 2), c(8, 3), c(5, 0)], 0, 14),
        ];
        for (cards, category, high) in cases {
            let p = poker(&cards).unwrap();
            assert_eq!((p.category, p.ranks[0]), (category, high));
        }
    }
    #[test]
    fn side_and_ante_charts_preserve_source_qualification() {
        let aces = poker(&[c(1, 0), c(1, 1), c(12, 2), c(8, 3), c(5, 0)]).unwrap();
        assert_eq!(aces.side_payout(5), 40);
        let twos = poker(&[c(2, 0), c(2, 1), c(12, 2), c(8, 3), c(5, 0)]).unwrap();
        assert!(!twos.qualified());
        assert_eq!(aces.payout(&twos, 10, 20), (40, 'q'));
        assert_eq!(aces.payout(&aces, 10, 20), (30, 'p'));
        assert_eq!(twos.side_payout(1000), 0);
    }
    #[test]
    fn blackjack_source_naturals_soft17_and_odd_bet_rounding() {
        let natural = BjHand::new(c(1, 0), c(13, 0));
        assert_eq!(natural.kind, 21);
        let mut dealer = BjHand::new(c(1, 2), c(6, 3));
        assert_eq!(dealer.total(), (17, true));
        dealer.hit(c(10, 2), false).unwrap();
        assert_eq!(dealer.total(), (17, false));
        dealer.stand();
        assert_eq!(natural.payout(&dealer, 5), 12);
        assert_eq!(BjHand::new(c(13, 1), c(10, 1)).kind, 20);
    }
    #[test]
    fn every_five_card_hand_matches_independent_combinatorial_category_counts() {
        // These expectations count rank/suit combinations, independently of
        // the evaluator's sorting, grouping and category decision algorithm.
        fn choose(n: u64, k: u64) -> u64 {
            (0..k).fold(1, |a, i| a * (n - i) / (i + 1))
        }
        let expected = [
            (choose(13, 5) - 10) * (4u64.pow(5) - 4),
            13 * choose(4, 2) * choose(12, 3) * 4u64.pow(3),
            choose(13, 2) * choose(4, 2).pow(2) * 11 * 4,
            13 * choose(4, 3) * choose(12, 2) * 4u64.pow(2),
            10 * (4u64.pow(5) - 4),
            4 * choose(13, 5) - 40,
            13 * choose(4, 3) * 12 * choose(4, 2),
            13 * 48,
            4 * 10,
        ];
        let mut actual = [0u64; 9];
        for a in 0..48 {
            for b in a + 1..49 {
                for c in b + 1..50 {
                    for d in c + 1..51 {
                        for e in d + 1..52 {
                            actual[five([Card(a), Card(b), Card(c), Card(d), Card(e)]).category
                                as usize] += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(actual, expected);
        assert_eq!(actual.iter().sum::<u64>(), choose(52, 5));
    }
    #[test]
    fn seven_card_selection_uses_best_full_house_kickers_and_wheel() {
        let two_trips = poker(&[
            c(8, 0),
            c(8, 1),
            c(8, 2),
            c(13, 0),
            c(13, 1),
            c(13, 2),
            c(1, 3),
        ])
        .unwrap();
        assert_eq!(two_trips.score(), (6, [13, 8, 0, 0, 0]));
        let wheel = poker(&[
            c(1, 0),
            c(2, 1),
            c(3, 2),
            c(4, 3),
            c(5, 0),
            c(13, 1),
            c(13, 2),
        ])
        .unwrap();
        assert_eq!(wheel.score(), (4, [5, 0, 0, 0, 0]));
        let same_ranks = poker(&[
            c(1, 1),
            c(2, 2),
            c(3, 3),
            c(4, 0),
            c(5, 1),
            c(12, 0),
            c(12, 1),
        ])
        .unwrap();
        assert_eq!(
            wheel.score(),
            same_ranks.score(),
            "suits and unused pocket pairs do not break a shared straight tie"
        );
    }
}
