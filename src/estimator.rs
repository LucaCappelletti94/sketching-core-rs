//! The [`CardinalityEstimator`] trait: the common interface for estimating set cardinalities and
//! their pairwise relations.
//!
//! Only [`estimate_cardinality`](CardinalityEstimator::estimate_cardinality) and
//! [`estimate_union_cardinality`](CardinalityEstimator::estimate_union_cardinality) are required;
//! the intersection, difference and Jaccard estimates are derived from those two by inclusion and
//! exclusion (clamped to remain non-negative).

use crate::constants::Zero;

/// Estimates set cardinalities and the cardinalities of pairwise set relations.
///
/// Only [`estimate_cardinality`](CardinalityEstimator::estimate_cardinality) and
/// [`estimate_union_cardinality`](CardinalityEstimator::estimate_union_cardinality) are required;
/// the intersection, difference and Jaccard estimates are derived from those two by inclusion and
/// exclusion (clamped to remain non-negative).
pub trait CardinalityEstimator {
    /// Estimates the cardinality of this set.
    fn estimate_cardinality(&self) -> f64;

    /// Estimates the cardinality of the union of this set and `other`.
    fn estimate_union_cardinality(&self, other: &Self) -> f64;

    /// Estimates the cardinality of the intersection, derived as `max(0, |A| + |B| - |A union B|)`.
    #[inline]
    fn estimate_intersection_cardinality(&self, other: &Self) -> f64 {
        let self_cardinality = self.estimate_cardinality();
        let other_cardinality = other.estimate_cardinality();
        let union_cardinality = self.estimate_union_cardinality(other);
        if self_cardinality + other_cardinality < union_cardinality {
            f64::ZERO
        } else {
            self_cardinality + other_cardinality - union_cardinality
        }
    }

    /// Estimates the Jaccard index, derived as `intersection / union` (zero when the union is empty
    /// or the inclusion-exclusion intersection would be negative).
    #[inline]
    fn estimate_jaccard_index(&self, other: &Self) -> f64 {
        let self_cardinality = self.estimate_cardinality();
        let other_cardinality = other.estimate_cardinality();
        let union_cardinality = self.estimate_union_cardinality(other);
        if self_cardinality + other_cardinality < union_cardinality
            || union_cardinality == f64::ZERO
        {
            f64::ZERO
        } else {
            (self_cardinality + other_cardinality - union_cardinality) / union_cardinality
        }
    }

    /// Estimates the cardinality of this set minus `other`, derived as `max(0, |A union B| - |B|)`.
    #[inline]
    fn estimate_difference_cardinality(&self, other: &Self) -> f64 {
        let union_cardinality = self.estimate_union_cardinality(other);
        let other_cardinality = other.estimate_cardinality();
        if union_cardinality < other_cardinality {
            f64::ZERO
        } else {
            union_cardinality - other_cardinality
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Mock estimator that computes union cardinality from both sets' cardinalities
    /// and a configurable overlap factor. This ensures mutants that replace the
    /// function body with constants are caught.
    struct OracleEstimator {
        cardinality: f64,
        /// Overlap fraction: 0.0 = disjoint, 1.0 = identical
        overlap: f64,
    }

    impl OracleEstimator {
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

    impl CardinalityEstimator for OracleEstimator {
        fn estimate_cardinality(&self) -> f64 {
            self.cardinality
        }

        fn estimate_union_cardinality(&self, other: &Self) -> f64 {
            self.expected_union(other)
        }
    }

    // --- Intersection tests ---

    #[test]
    fn test_intersection_partial_overlap() {
        let a = OracleEstimator::new(100.0, 0.3);
        let b = OracleEstimator::new(80.0, 0.3);
        // intersection = min(100, 80) * min(0.3, 0.3) = 80 * 0.3 = 24
        // union = 100 + 80 - 24 = 156
        // estimate_intersection = 100 + 80 - 156 = 24
        assert_eq!(a.estimate_intersection_cardinality(&b), 24.0);
    }

    #[test]
    fn test_intersection_disjoint() {
        let a = OracleEstimator::new(100.0, 0.0);
        let b = OracleEstimator::new(80.0, 0.0);
        assert_eq!(a.estimate_intersection_cardinality(&b), 0.0);
    }

    #[test]
    fn test_intersection_identical() {
        let a = OracleEstimator::new(100.0, 1.0);
        let b = OracleEstimator::new(100.0, 1.0);
        assert_eq!(a.estimate_intersection_cardinality(&b), 100.0);
    }

    #[test]
    fn test_intersection_asymmetric_overlap() {
        let a = OracleEstimator::new(100.0, 0.5);
        let b = OracleEstimator::new(80.0, 0.3);
        // intersection = min(100, 80) * min(0.5, 0.3) = 80 * 0.3 = 24
        // union = 100 + 80 - 24 = 156
        // estimate_intersection = 100 + 80 - 156 = 24
        assert_eq!(a.estimate_intersection_cardinality(&b), 24.0);
    }

    #[test]
    fn test_intersection_clamped_to_zero() {
        // When union > sum of cardinalities, intersection is clamped to 0
        let a = OracleEstimator::new(50.0, 0.0);
        let b = OracleEstimator::new(60.0, 0.0);
        // intersection = 0, union = 110
        // estimate_intersection = 50 + 60 - 110 = 0
        assert_eq!(a.estimate_intersection_cardinality(&b), 0.0);
    }

    // --- Jaccard tests ---

    #[test]
    fn test_jaccard_partial_overlap() {
        let a = OracleEstimator::new(100.0, 0.3);
        let b = OracleEstimator::new(80.0, 0.3);
        // intersection = 24, union = 156
        // J = 24 / 156
        assert_eq!(a.estimate_jaccard_index(&b), 24.0 / 156.0);
    }

    #[test]
    fn test_jaccard_disjoint() {
        let a = OracleEstimator::new(100.0, 0.0);
        let b = OracleEstimator::new(80.0, 0.0);
        assert_eq!(a.estimate_jaccard_index(&b), 0.0);
    }

    #[test]
    fn test_jaccard_identical() {
        let a = OracleEstimator::new(100.0, 1.0);
        let b = OracleEstimator::new(100.0, 1.0);
        assert_eq!(a.estimate_jaccard_index(&b), 1.0);
    }

    #[test]
    fn test_jaccard_zero_union() {
        let a = OracleEstimator::new(0.0, 0.0);
        let b = OracleEstimator::new(0.0, 0.0);
        assert_eq!(a.estimate_jaccard_index(&b), 0.0);
    }

    #[test]
    fn test_jaccard_negative_intersection_clamped() {
        let a = OracleEstimator::new(10.0, 0.0);
        let b = OracleEstimator::new(20.0, 0.0);
        assert_eq!(a.estimate_jaccard_index(&b), 0.0);
    }

    // --- Difference tests ---

    #[test]
    fn test_difference_partial_overlap() {
        let a = OracleEstimator::new(100.0, 0.3);
        let b = OracleEstimator::new(80.0, 0.3);
        // union = 156, |B| = 80
        // |A - B| = 156 - 80 = 76
        assert_eq!(a.estimate_difference_cardinality(&b), 76.0);
    }

    #[test]
    fn test_difference_disjoint() {
        let a = OracleEstimator::new(100.0, 0.0);
        let b = OracleEstimator::new(80.0, 0.0);
        // union = 180, |B| = 80
        // |A - B| = 180 - 80 = 100
        assert_eq!(a.estimate_difference_cardinality(&b), 100.0);
    }

    #[test]
    fn test_difference_identical() {
        let a = OracleEstimator::new(100.0, 1.0);
        let b = OracleEstimator::new(100.0, 1.0);
        // union = 100, |B| = 100
        // |A - B| = 100 - 100 = 0
        assert_eq!(a.estimate_difference_cardinality(&b), 0.0);
    }

    #[test]
    fn test_difference_clamped_to_zero() {
        let a = OracleEstimator::new(50.0, 0.0);
        let b = OracleEstimator::new(100.0, 0.0);
        // union = 150, |B| = 100
        // |A - B| = 150 - 100 = 50
        assert_eq!(a.estimate_difference_cardinality(&b), 50.0);
    }

    // --- Consistency tests ---

    #[test]
    fn test_inclusion_exclusion_consistency() {
        let a = OracleEstimator::new(100.0, 0.3);
        let b = OracleEstimator::new(80.0, 0.3);
        let intersection = a.estimate_intersection_cardinality(&b);
        let difference = a.estimate_difference_cardinality(&b);
        assert_eq!(difference + intersection, a.estimate_cardinality());
    }

    #[test]
    fn test_jaccard_consistency_with_intersection_and_union() {
        let a = OracleEstimator::new(100.0, 0.5);
        let b = OracleEstimator::new(80.0, 0.3);
        let jaccard = a.estimate_jaccard_index(&b);
        let intersection = a.estimate_intersection_cardinality(&b);
        let union = a.estimate_union_cardinality(&b);
        assert_eq!(jaccard, intersection / union);
    }

    #[test]
    fn test_difference_consistency() {
        let a = OracleEstimator::new(100.0, 0.5);
        let b = OracleEstimator::new(80.0, 0.3);
        let diff_a_b = a.estimate_difference_cardinality(&b);
        let diff_b_a = b.estimate_difference_cardinality(&a);
        let intersection = a.estimate_intersection_cardinality(&b);
        // |A - B| + |B - A| + |A intersect B| = |A union B|
        let union = a.estimate_union_cardinality(&b);
        assert_eq!(diff_a_b + diff_b_a + intersection, union);
    }
}
