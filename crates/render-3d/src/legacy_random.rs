//! Local .NET Framework seeded Random compatibility; presentation use only.
pub(crate) struct LegacyRandom {
    seed: [i32; 56],
    next: usize,
    next_p: usize,
}
impl LegacyRandom {
    pub(crate) fn new(seed: i32) -> Self {
        let mut values = [0; 56];
        let subtraction = if seed == i32::MIN {
            i32::MAX
        } else {
            seed.abs()
        };
        let mut mj = 161803398 - subtraction;
        values[55] = mj;
        let mut mk = 1;
        for i in 1..55 {
            let ii = (21 * i) % 55;
            values[ii] = mk;
            mk = mj.wrapping_sub(mk);
            if mk < 0 {
                mk += i32::MAX;
            }
            mj = values[ii];
        }
        for _ in 0..4 {
            for i in 1..56 {
                // The legacy CLR algorithm uses unchecked signed subtraction.
                values[i] = values[i].wrapping_sub(values[1 + (i + 30) % 55]);
                if values[i] < 0 {
                    values[i] += i32::MAX;
                }
            }
        }
        Self {
            seed: values,
            next: 0,
            next_p: 21,
        }
    }
    fn sample(&mut self) -> i32 {
        self.next += 1;
        if self.next >= 56 {
            self.next = 1;
        }
        self.next_p += 1;
        if self.next_p >= 56 {
            self.next_p = 1;
        }
        let mut result = self.seed[self.next].wrapping_sub(self.seed[self.next_p]);
        if result == i32::MAX {
            result -= 1;
        }
        if result < 0 {
            result += i32::MAX;
        }
        self.seed[self.next] = result;
        result
    }
    pub(crate) fn next(&mut self, max: i32) -> i32 {
        (self.unit() * f64::from(max)) as i32
    }
    pub(crate) fn unit(&mut self) -> f64 {
        f64::from(self.sample()) / f64::from(i32::MAX)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seeded_legacy_sequence_is_independent_of_render_cadence() {
        let mut r = LegacyRandom::new(1);
        assert_eq!(r.next(i32::MAX), 534011718);
        assert_eq!(r.next(i32::MAX), 237820880);
        assert_eq!(r.next(i32::MAX), 1002897798);
        let mut negative = LegacyRandom::new(-1);
        assert_eq!(negative.next(i32::MAX), 534011718);
    }
    #[test]
    fn extreme_legacy_seeds_stay_bounded_with_overflow_checks() {
        for seed in [i32::MIN, i32::MAX, 161803399, -161803399] {
            let mut random = LegacyRandom::new(seed);
            for _ in 0..128 {
                let n = random.next(6);
                assert!((0..6).contains(&n));
            }
        }
    }
}
