//! Joint sketching algorithms for estimating overlap and difference cardinalities between sets of
//! sketches.
//!
//! The [`HyperSpheresSketch`] trait provides [`joint_sketch`](HyperSpheresSketch::joint_sketch)
//! and [`normalized_joint_sketch`](HyperSpheresSketch::normalized_joint_sketch) for computing
//! the disjoint-cell decomposition of nested set pairs. The result is a [`JointSketch`] carrying
//! the exclusive overlap grid and the left and right margins.

use crate::constants::Zero;
use crate::estimator::CardinalityEstimator;
use crate::number::{FloatOps, Number};

/// The disjoint-cell cardinalities of a hypersphere sketch over `M` nested left sets and `N` nested
/// right sets: the `M*N` exclusive overlap grid plus the `M` left and `N` right margins. Build one
/// with [`JointSketch::estimate`] (or the trait method [`HyperSpheresSketch::joint_sketch`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JointSketch<const M: usize, const N: usize> {
    /// `overlap[i][j]` — the exclusive overlap grid.
    pub overlap: [[f64; N]; M],
    /// `left_diff[i]` — the left margins.
    pub left_diff: [f64; M],
    /// `right_diff[j]` — the right margins.
    pub right_diff: [f64; N],
}

/// The per-cell theoretical standard error of a [`JointSketch`], same shape as the sketch it
/// accompanies: a standard error for every overlap cell and every margin. Each entry is one standard
/// deviation of that cell's estimate in absolute (cardinality) units, so the relative error of a
/// cell is its standard error divided by its estimated value. Small overlap cells between large sets
/// carry a large relative error, which is exactly what this surfaces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JointSketchError<const M: usize, const N: usize> {
    /// Standard error of each `overlap[i][j]` cell.
    pub overlap_se: [[f64; N]; M],
    /// Standard error of each `left_diff[i]` margin.
    pub left_diff_se: [f64; M],
    /// Standard error of each `right_diff[j]` margin.
    pub right_diff_se: [f64; N],
}

impl<const M: usize, const N: usize> JointSketch<M, N> {
    /// Estimates the joint sketch of `M` nested left counters and `N` nested right counters by
    /// delegating to [`HyperSpheresSketch::joint_sketch`]. `M` and `N` are inferred from the arrays.
    #[inline]
    pub fn estimate<E: HyperSpheresSketch>(lefts: &[E; M], rights: &[E; N]) -> Self {
        E::joint_sketch(lefts, rights)
    }

    /// The total union cardinality: the sum of every disjoint cell.
    #[inline]
    #[must_use]
    pub fn union(&self) -> f64 {
        self.overlap.iter().flatten().copied().sum::<f64>()
            + self.left_diff.iter().sum::<f64>()
            + self.right_diff.iter().sum::<f64>()
    }

    /// Decomposes into the raw `(overlap, left_diff, right_diff)` arrays.
    #[inline]
    #[must_use]
    pub fn into_parts(self) -> ([[f64; N]; M], [f64; M], [f64; N]) {
        (self.overlap, self.left_diff, self.right_diff)
    }

    /// The per-cell shell maxima: the largest value each differential cell could take given the
    /// reconstructed marginals. These are exactly the denominators [`normalize`](Self::normalize)
    /// divides by: for an overlap cell the maximal differential overlap (the increment of
    /// `|A \ B| + |B|` across the shell), for a margin the differential shell size
    /// (`|A_i| - |A_{i-1}|`). Returned as a `JointSketch` of the same shape. Reconstructed purely from
    /// the differential cells, so it works on any decomposition (estimated or exact).
    #[must_use]
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::needless_range_loop
    )]
    pub fn shell_maxima(&self) -> Self {
        let mut card_left = [f64::ZERO; M];
        for i in 0..M {
            let shell = self.overlap[i].iter().copied().sum::<f64>() + self.left_diff[i];
            card_left[i] = shell + if i > 0 { card_left[i - 1] } else { f64::ZERO };
        }
        let mut card_right = [f64::ZERO; N];
        for j in 0..N {
            let shell = (0..M).map(|i| self.overlap[i][j]).sum::<f64>() + self.right_diff[j];
            card_right[j] = shell + if j > 0 { card_right[j - 1] } else { f64::ZERO };
        }
        let mut inter = [[f64::ZERO; N]; M];
        for i in 0..M {
            for j in 0..N {
                let up = if i > 0 { inter[i - 1][j] } else { f64::ZERO };
                let left = if j > 0 { inter[i][j - 1] } else { f64::ZERO };
                let diag = if i > 0 && j > 0 {
                    inter[i - 1][j - 1]
                } else {
                    f64::ZERO
                };
                inter[i][j] = self.overlap[i][j] + up + left - diag;
            }
        }

        let cl = |i: isize| {
            if i < 0 {
                f64::ZERO
            } else {
                card_left[i as usize]
            }
        };
        let cr = |j: isize| {
            if j < 0 {
                f64::ZERO
            } else {
                card_right[j as usize]
            }
        };
        let it = |i: isize, j: isize| {
            if i < 0 || j < 0 {
                f64::ZERO
            } else {
                inter[i as usize][j as usize]
            }
        };

        let mut overlap = [[f64::ZERO; N]; M];
        for i in 0..M {
            for j in 0..N {
                let a_minus_b = cl(i as isize).saturating_zero_sub(it(i as isize, j as isize));
                let prev_a_minus_b =
                    cl(i as isize - 1).saturating_zero_sub(it(i as isize - 1, j as isize));
                overlap[i][j] = (a_minus_b + cr(j as isize))
                    .saturating_zero_sub(prev_a_minus_b + cr(j as isize - 1));
            }
        }
        let mut left_diff = [f64::ZERO; M];
        for i in 0..M {
            left_diff[i] = cl(i as isize).saturating_zero_sub(cl(i as isize - 1));
        }
        let mut right_diff = [f64::ZERO; N];
        for j in 0..N {
            right_diff[j] = cr(j as isize).saturating_zero_sub(cr(j as isize - 1));
        }
        Self {
            overlap,
            left_diff,
            right_diff,
        }
    }

    /// Converts this absolute decomposition into normalized shell fractions in `[0, 1]`, matching
    /// [`HyperSpheresSketch::normalized_joint_sketch`]: each cell divided by its
    /// [`shell_maxima`](Self::shell_maxima). Unlike `normalized_joint_sketch` (which reads the
    /// operands), this works on any [`JointSketch`] — the joint-MLE output or an exact ground-truth
    /// decomposition. On the output of [`HyperSpheresSketch::joint_sketch`] it reproduces
    /// `normalized_joint_sketch` exactly.
    #[must_use]
    #[allow(clippy::needless_range_loop)]
    pub fn normalize(&self) -> Self {
        let smax = self.shell_maxima();
        let mut overlap = [[f64::ZERO; N]; M];
        for i in 0..M {
            for j in 0..N {
                overlap[i][j] = self.overlap[i][j]
                    .max(f64::ZERO)
                    .saturating_one_div(smax.overlap[i][j]);
            }
        }
        let mut left_diff = [f64::ZERO; M];
        for i in 0..M {
            left_diff[i] = self.left_diff[i]
                .max(f64::ZERO)
                .saturating_one_div(smax.left_diff[i]);
        }
        let mut right_diff = [f64::ZERO; N];
        for j in 0..N {
            right_diff[j] = self.right_diff[j]
                .max(f64::ZERO)
                .saturating_one_div(smax.right_diff[j]);
        }
        Self {
            overlap,
            left_diff,
            right_diff,
        }
    }
}

/// Holds the three cardinalities needed for inclusion-exclusion arithmetic.
struct EstimatedUnionCardinalities<F> {
    left: F,
    right: F,
    union: F,
}

impl<F: Number> EstimatedUnionCardinalities<F> {
    fn get_intersection_cardinality(&self) -> F {
        let intersection = self.left + self.right - self.union;
        debug_assert!(
            intersection >= F::ZERO,
            "Expected intersection to be larger than zero, but it is not. Got: intersection: {intersection:?}."
        );
        intersection
    }

    fn get_left_difference_cardinality(&self) -> F {
        self.union - self.right
    }

    fn get_right_difference_cardinality(&self) -> F {
        self.union - self.left
    }
}

/// The default pairwise inclusion-exclusion joint sketch: estimates each cumulative intersection
/// `|A_i intersect B_j|` from the marginal and union cardinalities, then differences them into the
/// disjoint cells. Shared by the [`HyperSpheresSketch`] default implementation.
pub fn inclusion_exclusion_joint_sketch<E: CardinalityEstimator, const L: usize, const R: usize>(
    lefts: &[E; L],
    rights: &[E; R],
) -> JointSketch<L, R> {
    let mut last_row = [f64::ZERO; R];
    let mut differential_overlap_cardinality_matrix = [[f64::ZERO; R]; L];
    let mut left_difference_cardinality_vector = [f64::ZERO; L];
    let mut right_cardinalities = [f64::ZERO; R];

    rights
        .iter()
        .zip(right_cardinalities.iter_mut())
        .for_each(|(right, right_cardinality)| {
            *right_cardinality = right.estimate_cardinality();
        });

    let mut right_difference_cardinality_vector = [f64::ZERO; R];
    let mut euc: EstimatedUnionCardinalities<f64> = EstimatedUnionCardinalities {
        left: f64::ZERO,
        right: f64::ZERO,
        union: f64::ZERO,
    };
    let mut last_left_difference = f64::ZERO;

    for (i, left) in lefts.iter().enumerate() {
        let mut last_right_difference = f64::ZERO;
        let left_cardinality = left.estimate_cardinality();
        let mut cumulative_row = f64::ZERO;
        for (j, (right, right_cardinality)) in rights.iter().zip(right_cardinalities).enumerate() {
            let union_cardinality = left.estimate_union_cardinality(right);
            euc = EstimatedUnionCardinalities {
                left: left_cardinality,
                right: right_cardinality,
                union: union_cardinality,
            };
            let delta = last_row[j] + cumulative_row;
            differential_overlap_cardinality_matrix[i][j] = euc
                .get_intersection_cardinality()
                .saturating_zero_sub(delta);
            last_row[j] = if euc.get_intersection_cardinality() > delta {
                euc.get_intersection_cardinality()
            } else {
                delta
            };

            cumulative_row += differential_overlap_cardinality_matrix[i][j];
            debug_assert!(cumulative_row >= f64::ZERO, "Expected cumulative_row to be larger than zero, but it is not. Got: cumulative_row: {cumulative_row:?}, delta: {delta:?}");

            right_difference_cardinality_vector[j] = euc
                .get_right_difference_cardinality()
                .saturating_zero_sub(last_right_difference);

            last_right_difference = euc.get_right_difference_cardinality();
        }
        left_difference_cardinality_vector[i] = euc
            .get_left_difference_cardinality()
            .saturating_zero_sub(last_left_difference);
        last_left_difference = euc.get_left_difference_cardinality();
    }

    JointSketch {
        overlap: differential_overlap_cardinality_matrix,
        left_diff: left_difference_cardinality_vector,
        right_diff: right_difference_cardinality_vector,
    }
}

/// Trait for sketching algorithms that provide the overlap and differences cardinality matrices.
/// The required cardinality and union estimators are inherited from [`CardinalityEstimator`].
///
/// The default [`joint_sketch`](HyperSpheresSketch::joint_sketch) implementation uses pairwise
/// inclusion-exclusion over the cardinality and union estimates. Implementors can override it for
/// more accurate joint estimation (e.g. joint maximum-likelihood).
pub trait HyperSpheresSketch: CardinalityEstimator + Sized {
    /// Returns the overlap and differences cardinality matrices of two lists of sets.
    ///
    /// # Arguments
    /// * `left` - The first list of sets.
    /// * `right` - The second list of sets.
    ///
    /// # Returns
    /// * `overlap_cardinality_matrix` - Matrix of estimated overlapping cardinalities between the elements of the left and right arrays.
    /// * `left_difference_cardinality_vector` - Vector of estimated difference cardinalities between the elements of the left array and the last element of the right array.
    /// * `right_difference_cardinality_vector` - Vector of estimated difference cardinalities between the elements of the right array and the last element of the left array.
    ///
    /// # Implementation details
    /// The elements of the left and right arrays are expected to be increasingly contained in the
    /// next one (nested sets). The default implementation uses pairwise inclusion-exclusion over
    /// [`CardinalityEstimator`] estimates.
    fn joint_sketch<const L: usize, const R: usize>(
        lefts: &[Self; L],
        rights: &[Self; R],
    ) -> JointSketch<L, R> {
        inclusion_exclusion_joint_sketch(lefts, rights)
    }

    /// Returns the normalized overlap and differences cardinality matrices of two lists of sets.
    ///
    /// Each cell is divided by its shell maximum to produce a value in `[0, 1]`.
    ///
    /// # Arguments
    /// * `left` - The first list of sets.
    /// * `right` - The second list of sets.
    fn normalized_joint_sketch<const L: usize, const R: usize>(
        lefts: &[Self; L],
        rights: &[Self; R],
    ) -> JointSketch<L, R> {
        let mut last_row = [f64::ZERO; R];
        let mut differential_overlap_cardinality_matrix = [[f64::ZERO; R]; L];
        let mut left_difference_cardinality_vector = [f64::ZERO; L];
        let mut right_cardinalities = [f64::ZERO; R];

        rights
            .iter()
            .zip(right_cardinalities.iter_mut())
            .for_each(|(right, right_cardinality)| {
                *right_cardinality = right.estimate_cardinality();
            });

        debug_assert!(
            right_cardinalities
                .iter()
                .zip(right_cardinalities.iter().skip(1))
                .all(|(left, right)| left <= right),
            "The right cardinalities should be sorted in ascending order."
        );

        let mut right_difference_cardinality_vector = [f64::ZERO; R];
        let mut euc: EstimatedUnionCardinalities<f64> = EstimatedUnionCardinalities {
            left: f64::ZERO,
            right: f64::ZERO,
            union: f64::ZERO,
        };
        let mut last_left_difference = f64::ZERO;
        let mut last_inner_left_differences = [f64::ZERO; R];
        let mut last_left_cardinality = f64::ZERO;

        for (i, left) in lefts.iter().enumerate() {
            let mut last_right_difference = f64::ZERO;
            let left_cardinality = left.estimate_cardinality();
            let mut cumulative_row = f64::ZERO;
            let mut last_right_cardinality = f64::ZERO;
            for (j, (right, (right_cardinality, last_inner_left_difference))) in rights
                .iter()
                .zip(
                    right_cardinalities
                        .iter()
                        .copied()
                        .zip(last_inner_left_differences.iter_mut()),
                )
                .enumerate()
            {
                let union_cardinality = left.estimate_union_cardinality(right);
                euc = EstimatedUnionCardinalities {
                    left: left_cardinality,
                    right: right_cardinality,
                    union: union_cardinality,
                };
                let delta = last_row[j] + cumulative_row;
                let differential_intersection = euc
                    .get_intersection_cardinality()
                    .saturating_zero_sub(delta);

                debug_assert!(
                    differential_intersection >= f64::ZERO,
                    concat!(
                        "Expected differential_intersection to be larger than zero, but it is not. ",
                        "Got: differential_intersection: {:?}, delta: {:?}",
                    ),
                    differential_intersection,
                    delta
                );

                let maximal_differential_intersection_cardinality =
                    (euc.get_left_difference_cardinality() + right_cardinality)
                        .saturating_zero_sub(*last_inner_left_difference + last_right_cardinality);
                *last_inner_left_difference = euc.get_left_difference_cardinality();

                differential_overlap_cardinality_matrix[i][j] = differential_intersection
                    .saturating_one_div(maximal_differential_intersection_cardinality);
                last_row[j] = if euc.get_intersection_cardinality() > delta {
                    euc.get_intersection_cardinality()
                } else {
                    delta
                };
                cumulative_row += differential_intersection;

                let differential_right_difference = euc
                    .get_right_difference_cardinality()
                    .saturating_zero_sub(last_right_difference);

                right_difference_cardinality_vector[j] = differential_right_difference
                    .saturating_one_div(
                        right_cardinality.saturating_zero_sub(last_right_cardinality),
                    );
                last_right_difference = euc.get_right_difference_cardinality();
                last_right_cardinality = right_cardinality;
            }
            left_difference_cardinality_vector[i] = euc
                .get_left_difference_cardinality()
                .saturating_zero_sub(last_left_difference)
                .saturating_one_div(left_cardinality.saturating_zero_sub(last_left_cardinality));
            last_left_cardinality = left_cardinality;
            last_left_difference = euc.get_left_difference_cardinality();
        }

        JointSketch {
            overlap: differential_overlap_cardinality_matrix,
            left_diff: left_difference_cardinality_vector,
            right_diff: right_difference_cardinality_vector,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    struct MockSketch {
        cardinality: f64,
        union_with: std::collections::HashMap<u64, f64>,
        id: u64,
    }

    impl MockSketch {
        fn new(cardinality: f64, id: u64) -> Self {
            Self {
                cardinality,
                union_with: std::collections::HashMap::new(),
                id,
            }
        }

        fn with_union(mut self, other_id: u64, union_card: f64) -> Self {
            self.union_with.insert(other_id, union_card);
            self
        }
    }

    impl CardinalityEstimator for MockSketch {
        fn estimate_cardinality(&self) -> f64 {
            self.cardinality
        }

        fn estimate_union_cardinality(&self, other: &Self) -> f64 {
            self.union_with
                .get(&other.id)
                .copied()
                .or_else(|| other.union_with.get(&self.id).copied())
                .unwrap_or(self.cardinality + other.cardinality)
        }
    }

    impl HyperSpheresSketch for MockSketch {}

    /// Mock estimator using the `OracleEstimator` pattern: computes union from
    /// cardinalities and a configurable overlap factor.
    #[derive(Clone, Copy)]
    struct OracleSketch {
        cardinality: f64,
        /// Overlap fraction: 0.0 = disjoint, 1.0 = identical
        overlap: f64,
    }

    impl OracleSketch {
        fn new(cardinality: f64, overlap: f64) -> Self {
            Self {
                cardinality,
                overlap,
            }
        }

        fn expected_intersection(&self, other: &Self) -> f64 {
            let min_card = self.cardinality.min(other.cardinality);
            min_card * self.overlap.min(other.overlap)
        }

        fn expected_union(&self, other: &Self) -> f64 {
            self.cardinality + other.cardinality - self.expected_intersection(other)
        }
    }

    impl CardinalityEstimator for OracleSketch {
        fn estimate_cardinality(&self) -> f64 {
            self.cardinality
        }

        fn estimate_union_cardinality(&self, other: &Self) -> f64 {
            self.expected_union(other)
        }
    }

    impl HyperSpheresSketch for OracleSketch {}

    #[test]
    fn test_joint_sketch_single_pair() {
        let a = MockSketch::new(100.0, 1).with_union(2, 150.0);
        let b = MockSketch::new(80.0, 2).with_union(1, 150.0);

        let result = inclusion_exclusion_joint_sketch(&[a], &[b]);

        // Overlap: |A intersect B| = 100 + 80 - 150 = 30
        assert_eq!(result.overlap[0][0], 30.0);
        // left_diff = |A union B| - |B| = 150 - 80 = 70
        assert_eq!(result.left_diff[0], 70.0);
        // right_diff = |A union B| - |A| = 150 - 100 = 50
        assert_eq!(result.right_diff[0], 50.0);
    }

    #[test]
    fn test_joint_sketch_disjoint() {
        let a = MockSketch::new(100.0, 1);
        let b = MockSketch::new(80.0, 2);

        let result = inclusion_exclusion_joint_sketch(&[a], &[b]);

        // Disjoint: union = 180, intersection = 0
        assert_eq!(result.overlap[0][0], 0.0);
        assert_eq!(result.left_diff[0], 100.0);
        assert_eq!(result.right_diff[0], 80.0);
    }

    #[test]
    fn test_joint_sketch_identical() {
        let a = MockSketch::new(100.0, 1).with_union(2, 100.0);
        let b = MockSketch::new(100.0, 2).with_union(1, 100.0);

        let result = inclusion_exclusion_joint_sketch(&[a], &[b]);

        // Identical: intersection = 100, left_diff = 0, right_diff = 0
        assert_eq!(result.overlap[0][0], 100.0);
        assert_eq!(result.left_diff[0], 0.0);
        assert_eq!(result.right_diff[0], 0.0);
    }

    #[test]
    fn test_joint_sketch_2x2() {
        let a1 = MockSketch::new(50.0, 1);
        let a2 = MockSketch::new(100.0, 2);
        let b1 = MockSketch::new(40.0, 3);
        let b2 = MockSketch::new(80.0, 4);

        let result = inclusion_exclusion_joint_sketch(&[a1, a2], &[b1, b2]);

        let total = result.union();
        assert!(total > 0.0, "Union should be positive");
    }

    #[test]
    fn test_joint_sketch_into_parts() {
        let a = MockSketch::new(100.0, 1).with_union(2, 150.0);
        let b = MockSketch::new(80.0, 2).with_union(1, 150.0);

        let result = inclusion_exclusion_joint_sketch(&[a], &[b]);
        let (overlap, left_diff, right_diff) = result.into_parts();

        assert_eq!(overlap[0][0], 30.0);
        assert_eq!(left_diff[0], 70.0);
        assert_eq!(right_diff[0], 50.0);
    }

    #[test]
    fn test_joint_sketch_union() {
        let a = MockSketch::new(100.0, 1).with_union(2, 150.0);
        let b = MockSketch::new(80.0, 2).with_union(1, 150.0);

        let result = inclusion_exclusion_joint_sketch(&[a], &[b]);

        // Union = overlap + left_diff + right_diff = 30 + 70 + 50 = 150
        assert_eq!(result.union(), 150.0);
    }

    #[test]
    fn test_shell_maxima() {
        let sketch = JointSketch::<2, 2> {
            overlap: [[10.0, 5.0], [3.0, 2.0]],
            left_diff: [7.0, 4.0],
            right_diff: [6.0, 3.0],
        };

        let maxima = sketch.shell_maxima();

        // Verify maxima are non-negative
        for i in 0..2 {
            for j in 0..2 {
                assert!(
                    maxima.overlap[i][j] >= 0.0,
                    "Overlap maxima[{i}][{j}] should be non-negative"
                );
            }
            assert!(
                maxima.left_diff[i] >= 0.0,
                "Left diff maxima[{i}] should be non-negative"
            );
        }
        for j in 0..2 {
            assert!(
                maxima.right_diff[j] >= 0.0,
                "Right diff maxima[{j}] should be non-negative"
            );
        }
    }

    #[test]
    fn test_normalize() {
        let sketch = JointSketch::<1, 1> {
            overlap: [[30.0]],
            left_diff: [70.0],
            right_diff: [50.0],
        };

        let normalized = sketch.normalize();

        // All normalized values should be in [0, 1]
        assert!(normalized.overlap[0][0] >= 0.0 && normalized.overlap[0][0] <= 1.0);
        assert!(normalized.left_diff[0] >= 0.0 && normalized.left_diff[0] <= 1.0);
        assert!(normalized.right_diff[0] >= 0.0 && normalized.right_diff[0] <= 1.0);
    }

    #[test]
    fn test_normalize_zero_maxima() {
        let sketch = JointSketch::<1, 1> {
            overlap: [[0.0]],
            left_diff: [0.0],
            right_diff: [0.0],
        };

        let normalized = sketch.normalize();

        // saturating_one_div(0.0, 0.0) returns 1.0 (self >= other)
        assert_eq!(normalized.overlap[0][0], 1.0);
        assert_eq!(normalized.left_diff[0], 1.0);
        assert_eq!(normalized.right_diff[0], 1.0);
    }

    #[test]
    fn test_hyper_spheres_joint_sketch_default() {
        let a = MockSketch::new(100.0, 1).with_union(2, 150.0);
        let b = MockSketch::new(80.0, 2).with_union(1, 150.0);

        let result = <MockSketch as HyperSpheresSketch>::joint_sketch(&[a], &[b]);

        assert_eq!(result.overlap[0][0], 30.0);
    }

    #[test]
    fn test_estimated_union_cardinalities() {
        let euc = EstimatedUnionCardinalities {
            left: 100.0,
            right: 80.0,
            union: 150.0,
        };

        assert_eq!(euc.get_intersection_cardinality(), 30.0);
        assert_eq!(euc.get_left_difference_cardinality(), 70.0);
        assert_eq!(euc.get_right_difference_cardinality(), 50.0);
    }

    #[test]
    fn test_joint_sketch_copy() {
        let a = MockSketch::new(100.0, 1).with_union(2, 150.0);
        let b = MockSketch::new(80.0, 2).with_union(1, 150.0);

        let result = inclusion_exclusion_joint_sketch(&[a], &[b]);
        let copied = result;

        assert_eq!(result.overlap[0][0], copied.overlap[0][0]);
        assert_eq!(result.left_diff[0], copied.left_diff[0]);
        assert_eq!(result.right_diff[0], copied.right_diff[0]);
    }

    #[test]
    fn test_joint_sketch_partial_eq() {
        let sketch1 = JointSketch::<1, 1> {
            overlap: [[30.0]],
            left_diff: [70.0],
            right_diff: [50.0],
        };
        let sketch2 = JointSketch::<1, 1> {
            overlap: [[30.0]],
            left_diff: [70.0],
            right_diff: [50.0],
        };
        let sketch3 = JointSketch::<1, 1> {
            overlap: [[31.0]],
            left_diff: [70.0],
            right_diff: [50.0],
        };

        assert_eq!(sketch1, sketch2);
        assert_ne!(sketch1, sketch3);
    }

    #[test]
    fn test_normalized_joint_sketch() {
        let a = MockSketch::new(100.0, 1).with_union(2, 150.0);
        let b = MockSketch::new(80.0, 2).with_union(1, 150.0);

        let result = <MockSketch as HyperSpheresSketch>::normalized_joint_sketch(&[a], &[b]);

        // All normalized values should be in [0, 1]
        assert!(result.overlap[0][0] >= 0.0 && result.overlap[0][0] <= 1.0);
        assert!(result.left_diff[0] >= 0.0 && result.left_diff[0] <= 1.0);
        assert!(result.right_diff[0] >= 0.0 && result.right_diff[0] <= 1.0);
    }

    // --- OracleSketch: 2x2 joint sketch with exact values ---

    // --- OracleSketch: 3x3 joint sketch with exact values ---

    // --- Nested sets with increasing cardinalities ---

    // --- Normalized joint sketch with various overlap patterns ---

    #[test]
    fn test_normalized_joint_sketch_1x1_exact() {
        let a = OracleSketch::new(100.0, 0.5);
        let b = OracleSketch::new(80.0, 0.5);

        let result = <OracleSketch as HyperSpheresSketch>::normalized_joint_sketch(&[a], &[b]);

        // intersection=40, union=140
        // left_diff=60, right_diff=40
        // max_diff_intersection=140, max_left_shell=100, max_right_shell=80
        // overlap[0][0]=40/140=0.2857142857142857
        assert_eq!(result.overlap[0][0], 40.0 / 140.0);
        assert_eq!(result.left_diff[0], 60.0 / 100.0);
        assert_eq!(result.right_diff[0], 40.0 / 80.0);
    }

    #[test]
    fn test_normalized_joint_sketch_disjoint() {
        let a0 = OracleSketch::new(50.0, 0.0);
        let a1 = OracleSketch::new(100.0, 0.0);
        let b0 = OracleSketch::new(40.0, 0.0);
        let b1 = OracleSketch::new(80.0, 0.0);

        let result =
            <OracleSketch as HyperSpheresSketch>::normalized_joint_sketch(&[a0, a1], &[b0, b1]);

        // All overlaps are zero, all left/right diffs are 1.0
        assert_eq!(result.overlap[0][0], 0.0);
        assert_eq!(result.overlap[0][1], 0.0);
        assert_eq!(result.overlap[1][0], 0.0);
        assert_eq!(result.overlap[1][1], 0.0);
        assert_eq!(result.left_diff[0], 1.0);
        assert_eq!(result.left_diff[1], 1.0);
        assert_eq!(result.right_diff[0], 1.0);
        assert_eq!(result.right_diff[1], 1.0);
    }

    #[test]
    fn test_normalized_joint_sketch_identical() {
        let a = OracleSketch::new(100.0, 1.0);
        let b = OracleSketch::new(100.0, 1.0);

        let result = <OracleSketch as HyperSpheresSketch>::normalized_joint_sketch(&[a], &[b]);

        // overlap=100/100=1.0, left_diff=0/100=0.0, right_diff=0/100=0.0
        assert_eq!(result.overlap[0][0], 1.0);
        assert_eq!(result.left_diff[0], 0.0);
        assert_eq!(result.right_diff[0], 0.0);
    }

    // --- JointSketch::shell_maxima with exact value assertions ---

    #[test]
    fn test_shell_maxima_1x1_exact() {
        let sketch = JointSketch::<1, 1> {
            overlap: [[30.0]],
            left_diff: [70.0],
            right_diff: [50.0],
        };

        let maxima = sketch.shell_maxima();

        // card_left[0]=100, card_right[0]=80, inter[0][0]=30
        // a_minus_b=70, prev=0, max_overlap=(70+80)-(0+0)=150
        // max_left_diff=100-0=100, max_right_diff=80-0=80
        assert_eq!(maxima.overlap[0][0], 150.0);
        assert_eq!(maxima.left_diff[0], 100.0);
        assert_eq!(maxima.right_diff[0], 80.0);
    }

    #[test]
    fn test_shell_maxima_all_zeros() {
        let sketch = JointSketch::<1, 1> {
            overlap: [[0.0]],
            left_diff: [0.0],
            right_diff: [0.0],
        };

        let maxima = sketch.shell_maxima();

        assert_eq!(maxima.overlap[0][0], 0.0);
        assert_eq!(maxima.left_diff[0], 0.0);
        assert_eq!(maxima.right_diff[0], 0.0);
    }

    // --- JointSketch::normalize with exact value assertions ---

    #[test]
    fn test_normalize_1x1_exact() {
        let sketch = JointSketch::<1, 1> {
            overlap: [[30.0]],
            left_diff: [70.0],
            right_diff: [50.0],
        };

        let normalized = sketch.normalize();

        // shell_maxima: overlap=150, left_diff=100, right_diff=80
        assert_eq!(normalized.overlap[0][0], 30.0 / 150.0);
        assert_eq!(normalized.left_diff[0], 70.0 / 100.0);
        assert_eq!(normalized.right_diff[0], 50.0 / 80.0);
    }

    #[test]
    fn test_normalize_consistency_with_normalized_joint_sketch() {
        let a0 = OracleSketch::new(50.0, 0.4);
        let a1 = OracleSketch::new(100.0, 0.4);
        let b0 = OracleSketch::new(40.0, 0.4);
        let b1 = OracleSketch::new(80.0, 0.4);

        let joint = inclusion_exclusion_joint_sketch(&[a0, a1], &[b0, b1]);
        let normalized_from_joint = joint.normalize();
        let normalized_direct =
            <OracleSketch as HyperSpheresSketch>::normalized_joint_sketch(&[a0, a1], &[b0, b1]);

        assert_eq!(normalized_from_joint.overlap, normalized_direct.overlap);
        assert_eq!(normalized_from_joint.left_diff, normalized_direct.left_diff);
        assert_eq!(
            normalized_from_joint.right_diff,
            normalized_direct.right_diff
        );
    }

    // --- JointSketch::union with exact value assertions ---

    #[test]
    fn test_union_1x1_exact() {
        let sketch = JointSketch::<1, 1> {
            overlap: [[30.0]],
            left_diff: [70.0],
            right_diff: [50.0],
        };

        assert_eq!(sketch.union(), 150.0);
    }

    #[test]
    fn test_union_all_zeros() {
        let sketch = JointSketch::<1, 1> {
            overlap: [[0.0]],
            left_diff: [0.0],
            right_diff: [0.0],
        };

        assert_eq!(sketch.union(), 0.0);
    }

    // --- inclusion_exclusion_joint_sketch edge cases ---

    #[test]
    fn test_inclusion_exclusion_all_zeros() {
        let a = OracleSketch::new(0.0, 0.0);
        let b = OracleSketch::new(0.0, 0.0);

        let result = inclusion_exclusion_joint_sketch(&[a], &[b]);

        assert_eq!(result.overlap[0][0], 0.0);
        assert_eq!(result.left_diff[0], 0.0);
        assert_eq!(result.right_diff[0], 0.0);
        assert_eq!(result.union(), 0.0);
    }

    #[test]
    fn test_inclusion_exclusion_3x3_all_identical() {
        let a0 = OracleSketch::new(100.0, 1.0);
        let a1 = OracleSketch::new(100.0, 1.0);
        let a2 = OracleSketch::new(100.0, 1.0);
        let b0 = OracleSketch::new(100.0, 1.0);
        let b1 = OracleSketch::new(100.0, 1.0);
        let b2 = OracleSketch::new(100.0, 1.0);

        let result = inclusion_exclusion_joint_sketch(&[a0, a1, a2], &[b0, b1, b2]);

        // All identical: intersection always 100
        // overlap[0][0]=100, all others=0
        assert_eq!(result.overlap[0][0], 100.0);
        for i in 0..3 {
            for j in 0..3 {
                if i == 0 && j == 0 {
                    continue;
                }
                assert_eq!(result.overlap[i][j], 0.0);
            }
        }
        assert_eq!(result.left_diff[0], 0.0);
        assert_eq!(result.left_diff[1], 0.0);
        assert_eq!(result.left_diff[2], 0.0);
        assert_eq!(result.right_diff[0], 0.0);
        assert_eq!(result.right_diff[1], 0.0);
        assert_eq!(result.right_diff[2], 0.0);
        assert_eq!(result.union(), 100.0);
    }

    #[test]
    fn test_inclusion_exclusion_all_disjoint() {
        let a0 = OracleSketch::new(50.0, 0.0);
        let a1 = OracleSketch::new(100.0, 0.0);
        let b0 = OracleSketch::new(40.0, 0.0);
        let b1 = OracleSketch::new(80.0, 0.0);

        let result = inclusion_exclusion_joint_sketch(&[a0, a1], &[b0, b1]);

        // All overlaps zero, left/right diffs equal cardinalities
        assert_eq!(result.overlap[0][0], 0.0);
        assert_eq!(result.overlap[0][1], 0.0);
        assert_eq!(result.overlap[1][0], 0.0);
        assert_eq!(result.overlap[1][1], 0.0);
        assert_eq!(result.left_diff[0], 50.0);
        assert_eq!(result.left_diff[1], 50.0);
        assert_eq!(result.right_diff[0], 40.0);
        assert_eq!(result.right_diff[1], 40.0);
        assert_eq!(result.union(), 180.0);
    }

    #[test]
    fn test_joint_sketch_estimate_method() {
        let a = OracleSketch::new(100.0, 0.5);
        let b = OracleSketch::new(80.0, 0.5);

        let result = JointSketch::estimate(&[a], &[b]);

        assert_eq!(result.overlap[0][0], 40.0);
        assert_eq!(result.left_diff[0], 60.0);
        assert_eq!(result.right_diff[0], 40.0);
    }
    #[test]
    fn test_shell_maxima_inclusion_exclusion_2x2() {
        // Verify shell_maxima correctly computes inclusion-exclusion
        // for non-trivial overlap patterns
        let sketch = JointSketch::<2, 2> {
            overlap: [[10.0, 20.0], [30.0, 40.0]],
            left_diff: [10.0, 20.0],
            right_diff: [5.0, 15.0],
        };

        let maxima = sketch.shell_maxima();

        // Verify maxima values are non-negative
        for i in 0..2 {
            for j in 0..2 {
                assert!(
                    maxima.overlap[i][j] >= 0.0,
                    "maxima.overlap[{}][{}] should be non-negative",
                    i,
                    j
                );
            }
        }
        for i in 0..2 {
            assert!(
                maxima.left_diff[i] >= 0.0,
                "maxima.left_diff[{}] should be non-negative",
                i
            );
            assert!(
                maxima.right_diff[i] >= 0.0,
                "maxima.right_diff[{}] should be non-negative",
                i
            );
        }

        // Verify shell_maxima union is >= original union
        assert!(maxima.union() >= sketch.union());
    }

    #[test]
    fn test_shell_maxima_non_negative() {
        // Shell maxima should produce non-negative values
        let sketch = JointSketch::<3, 2> {
            overlap: [[10.0, 5.0], [20.0, 15.0], [30.0, 25.0]],
            left_diff: [10.0, 20.0, 30.0],
            right_diff: [5.0, 15.0],
        };

        let maxima = sketch.shell_maxima();

        // Verify all values are non-negative
        for i in 0..3 {
            for j in 0..2 {
                assert!(
                    maxima.overlap[i][j] >= 0.0,
                    "maxima.overlap[{}][{}] should be non-negative",
                    i,
                    j
                );
            }
            assert!(
                maxima.left_diff[i] >= 0.0,
                "maxima.left_diff[{}] should be non-negative",
                i
            );
        }
        for j in 0..2 {
            assert!(
                maxima.right_diff[j] >= 0.0,
                "maxima.right_diff[{}] should be non-negative",
                j
            );
        }

        // Verify shell_maxima union is >= original union
        assert!(maxima.union() >= sketch.union());
    }
}
