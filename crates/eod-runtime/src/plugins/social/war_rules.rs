// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! Source order: Artillery, Cavalry, Command, Infantry, Intelligence.

pub(super) fn defeated_player(blue: u8, red: u8) -> Option<u8> {
    const DEFEATS: [[u8; 2]; 5] = [[3, 1], [4, 2], [0, 4], [1, 2], [3, 0]];
    if blue == red || blue > 4 || red > 4 {
        None
    } else if DEFEATS[usize::from(red)].contains(&blue) {
        Some(0)
    } else if DEFEATS[usize::from(blue)].contains(&red) {
        Some(1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_twenty_five_source_piece_pairings() {
        // Literal truth table derived independently from the original Defeats
        // child lists. -1 is a tie, 0 loses Blue, 1 loses Red.
        let expected: [[i8; 5]; 5] = [
            [-1, 1, 0, 1, 0],
            [0, -1, 1, 0, 1],
            [1, 0, -1, 0, 1],
            [0, 1, 1, -1, 0],
            [1, 0, 0, 1, -1],
        ];
        for (blue, row) in expected.iter().enumerate() {
            for (red, expected) in row.iter().enumerate() {
                let actual = defeated_player(blue as u8, red as u8);
                assert_eq!(
                    actual.map(i16::from).unwrap_or(-1),
                    i16::from(*expected),
                    "blue={blue}, red={red}"
                );
            }
        }
    }
}
