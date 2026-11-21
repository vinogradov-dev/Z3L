//! Which light a sample asks, and the weight that keeps the estimator honest.
//!
//! A scene lights a point light by its radiant power in watts (ADR 0039), and a picture lit by one
//! lamp drawn uniformly out of eight spends seven eighths of its samples on lamps that contribute
//! little. The table here is Vose's alias method: one draw picks a light with probability proportional
//! to its power, so the number of shadow rays a bright lamp receives is the share of the light it
//! gives. It is O(1) per draw whatever the light count, and its own distribution is exact rather than
//! approximate, which the tests check by counting decisions instead of comparing noise.
//!
//! The second half of the module is the weight a light sample is taken with when the same path could
//! have been found by a BSDF sample: the power heuristic of Veach (1997), which is what multiple
//! importance sampling divides by. Today's frame carries a key light and point lights, and a point
//! light's contribution to a BSDF sample's density is zero — its direction is exact, a delta — so the
//! weight is one and the machinery is here for the area emitter that is not (L2's rectangular
//! window). Writing it as the general rule rather than a bare `1.0` means the formula is tested now and
//! not debugged inside a picture later.
//!
//! Refuses: a weight that is not a positive finite number is `None` instead of a table, and a table
//! with no light at all samples to `None` rather than to a light that does not exist.
//!
//! Changes when: how a light is chosen, or how two strategies are weighted, changes.

/// Vose's alias table over a set of weights.
///
/// Each draw is one column and one comparison, and the probability a column answers with its own
/// weight is stored rather than recomputed. The table is built once per frame, from the frame's lights
/// in their own order, so a draw's index names the light the frame uploaded at that index.
#[derive(Clone, Debug, PartialEq)]
pub struct AliasTable {
    /// The probability that column `i` answers with the weight it owns.
    probability: Vec<f32>,
    /// The column `i` answers with instead, when it does not answer with its own.
    alias: Vec<u32>,
    /// The share of the total weight each light carries, which is the density of a draw.
    share: Vec<f32>,
}

impl AliasTable {
    /// Builds a table over `weights`, or `None` when there is no positive weight to draw from.
    ///
    /// Every weight is clamped at zero and the non-finite ones are dropped, because a table built from
    /// a NaN would answer with a light whose share is NaN and divide an image by it.
    pub fn build(weights: &[f32]) -> Option<Self> {
        let count = weights.len();
        if count == 0 {
            return None;
        }
        let cleaned: Vec<f64> =
            weights.iter().map(|weight| if weight.is_finite() { f64::from(*weight).max(0.0) } else { 0.0 }).collect();
        let total: f64 = cleaned.iter().sum();
        if !total.is_finite() || total <= 0.0 {
            return None;
        }
        let share: Vec<f32> = cleaned.iter().map(|weight| (weight / total) as f32).collect();
        let mut scaled: Vec<f64> = cleaned.iter().map(|weight| weight * count as f64 / total).collect();
        let mut probability = vec![1.0f32; count];
        let mut alias: Vec<u32> = (0..count as u32).collect();
        let mut small: Vec<usize> = Vec::with_capacity(count);
        let mut large: Vec<usize> = Vec::with_capacity(count);
        for (index, value) in scaled.iter().enumerate() {
            if *value < 1.0 {
                small.push(index);
            } else {
                large.push(index);
            }
        }
        while let (Some(under), Some(over)) = (small.pop(), large.pop()) {
            let Some(value) = scaled.get(under).copied() else { break };
            probability[under] = value.clamp(0.0, 1.0) as f32;
            if let Some(slot) = alias.get_mut(under) {
                *slot = over as u32;
            }
            if let Some(rest) = scaled.get_mut(over) {
                *rest -= 1.0 - value;
                if *rest < 1.0 {
                    small.push(over);
                } else {
                    large.push(over);
                }
            }
        }
        // Whatever is left over owns its own column wholly; its probability is already one.
        for index in large.into_iter().chain(small) {
            if let Some(value) = scaled.get(index) {
                probability[index] = value.clamp(0.0, 1.0) as f32;
            }
        }
        Some(Self { probability, alias, share })
    }

    /// The number of lights the table draws from.
    pub fn len(&self) -> usize {
        self.probability.len()
    }

    /// Whether the table has no light in it.
    pub fn is_empty(&self) -> bool {
        self.probability.is_empty()
    }

    /// Draws a light index from one pair of uniform numbers.
    ///
    /// The first number picks the column, the second decides whether the column answers for itself, so
    /// two numbers in `[0, 1)` are one draw and no rejection loop.
    pub fn sample(&self, u: [f32; 2]) -> Option<usize> {
        let count = self.probability.len();
        if count == 0 {
            return None;
        }
        let first = f64::from(u[0]).clamp(0.0, 1.0);
        let column = ((first * count as f64) as usize).min(count - 1);
        let within = first * count as f64 - column as f64;
        let threshold = f64::from(self.probability.get(column).copied().unwrap_or(1.0));
        if within < threshold { Some(column) } else { self.alias.get(column).map(|index| *index as usize) }
    }

    /// The share of the total weight light `index` carries, which is the density of a draw.
    pub fn share(&self, index: usize) -> f64 {
        f64::from(self.share.get(index).copied().unwrap_or(0.0))
    }

    /// The column a device needs to draw the same way: the threshold it answers for itself with, and
    /// the column it answers with instead.
    pub fn column(&self, index: usize) -> (f32, u32) {
        (self.probability.get(index).copied().unwrap_or(0.0), self.alias.get(index).copied().unwrap_or(0))
    }
}

/// The power heuristic's weight for a sample both strategies could have found.
///
/// `first` is the density of the strategy the sample was drawn from and `second` the density the other
/// strategy would have given it. The two weights of one path add up to one, which is what makes the
/// combination unbiased, and a strategy that could not have found the path has density zero and gets
/// nothing.
pub fn balance(first: f64, second: f64) -> f64 {
    let a = if first.is_finite() { first.max(0.0) } else { 0.0 };
    let b = if second.is_finite() { second.max(0.0) } else { 0.0 };
    let squares = a * a + b * b;
    if squares <= 0.0 {
        return 0.0;
    }
    (a * a / squares).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    /// The share of `draws` draws that landed on each index, which is the distribution to compare.
    fn histogram(table: &AliasTable, draws: u32) -> Vec<f64> {
        let mut counts = vec![0u32; table.len()];
        for index in 0..draws {
            let u = crate::sampler::sample_2d(0, index, 0x2f1e_9c3b);
            if let Some(picked) = table.sample([u[0], u[1]])
                && let Some(slot) = counts.get_mut(picked)
            {
                *slot += 1;
            }
        }
        counts.iter().map(|count| f64::from(*count) / f64::from(draws)).collect()
    }

    #[test]
    fn a_lamp_of_half_the_power_is_asked_half_as_often() {
        let table = AliasTable::build(&[50.0, 200.0, 750.0]).expect("three lit lamps");
        assert_eq!(table.len(), 3);
        let seen = histogram(&table, 1 << 15);
        for (index, share) in seen.iter().enumerate() {
            let expected = table.share(index);
            assert!((share - expected).abs() < 0.005, "light {index}: drawn {share}, share {expected}");
        }
        // The extremes are the ones a picture shows: the 750 W lamp is asked fifteen times as often as
        // the 50 W one, which is what an alias table buys over a uniform choice.
        assert!(seen[2] > 14.0 * seen[0], "{seen:?}");
    }

    #[test]
    fn a_lamp_that_gives_no_light_is_never_asked() {
        let table = AliasTable::build(&[0.0, 120.0, 0.0]).expect("one lit lamp");
        let seen = histogram(&table, 1 << 12);
        assert_eq!(seen, vec![0.0, 1.0, 0.0], "a dead light is not a strategy");
        assert_eq!(table.share(1), 1.0);
        assert_eq!(table.share(0), 0.0);
    }

    #[test]
    fn a_table_of_one_light_and_a_table_of_none() {
        let single = AliasTable::build(&[3.0]).expect("one lamp");
        assert_eq!(single.len(), 1);
        for u in [[0.0, 0.0], [0.5, 0.5], [0.999_9, 0.5]] {
            assert_eq!(single.sample(u), Some(0));
        }
        assert!(AliasTable::build(&[]).is_none(), "no light is no table");
        assert!(AliasTable::build(&[0.0, 0.0]).is_none(), "no power is no table");
        assert!(AliasTable::build(&[f32::NAN, f32::INFINITY]).is_none(), "a hostile weight is refused");
        // A hostile column is clamped rather than sampled out of range.
        let table = AliasTable::build(&[10.0, f32::NAN]).expect("one live lamp");
        assert_eq!(table.sample([0.999_9, 0.999_9]), Some(0), "a NaN lamp carries no share");
    }

    #[test]
    fn two_strategies_of_one_path_share_it_without_double_counting() {
        assert!((balance(1.0, 0.0) - 1.0).abs() < 1.0e-12, "a light sample of a delta light is weighted one");
        assert_eq!(balance(0.0, 0.0), 0.0, "a path nobody could draw has no weight");
        for first in [0.1, 0.5, 1.0, 4.0] {
            for second in [0.1, 0.5, 1.0, 4.0] {
                let total = balance(first, second) + balance(second, first);
                assert!((total - 1.0).abs() < 1.0e-12, "{first} and {second} sum to {total}");
            }
        }
        // The stronger strategy keeps the path: the power heuristic prefers where it came from.
        assert!(balance(4.0, 1.0) > 0.9);
        assert!(balance(1.0, 4.0) < 0.1);
        // A hostile density is a zero rather than a NaN weight.
        assert_eq!(balance(f64::NAN, 1.0), 0.0);
        assert_eq!(balance(-1.0, 0.0), 0.0);
    }
}
