//! Implements the matrix trait for arrays.

use core::fmt::Debug;

/// Trait for matrices.
pub trait Matrix<T, const ROWS: usize> {
    /// Returns the column of the matrix.
    fn column(&self, column: usize) -> [T; ROWS];
}

impl<const ROWS: usize, T: Copy + Default + Debug, R> Matrix<T, ROWS> for [R; ROWS]
where
    R: AsRef<[T]> + Debug,
{
    #[inline]
    #[allow(unsafe_code)]
    /// Returns the column of the matrix.
    ///
    /// # Safety
    /// We are guaranteed that the length of the row is equal to the number of columns,
    /// hence we can safely use `get_unchecked`.
    fn column(&self, column: usize) -> [T; ROWS] {
        let mut result = [T::default(); ROWS];
        for (i, row) in self.iter().enumerate() {
            result[i] = unsafe { *row.as_ref().get_unchecked(column) };
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_single_row() {
        let matrix: [[u32; 3]; 1] = [[1, 2, 3]];
        assert_eq!(matrix.column(0), [1]);
        assert_eq!(matrix.column(1), [2]);
        assert_eq!(matrix.column(2), [3]);
    }

    #[test]
    fn test_column_multiple_rows() {
        let matrix: [[u32; 3]; 3] = [[1, 2, 3], [4, 5, 6], [7, 8, 9]];
        assert_eq!(matrix.column(0), [1, 4, 7]);
        assert_eq!(matrix.column(1), [2, 5, 8]);
        assert_eq!(matrix.column(2), [3, 6, 9]);
    }

    #[test]
    fn test_column_f64() {
        let matrix: [[f64; 2]; 2] = [[1.0, 2.0], [3.0, 4.0]];
        assert_eq!(matrix.column(0), [1.0, 3.0]);
        assert_eq!(matrix.column(1), [2.0, 4.0]);
    }

    #[test]
    fn test_column_wide_matrix() {
        let matrix: [[u8; 5]; 2] = [[10, 20, 30, 40, 50], [60, 70, 80, 90, 100]];
        assert_eq!(matrix.column(0), [10, 60]);
        assert_eq!(matrix.column(4), [50, 100]);
    }

    #[test]
    fn test_column_tall_matrix() {
        let matrix: [[u16; 1]; 4] = [[1], [2], [3], [4]];
        assert_eq!(matrix.column(0), [1, 2, 3, 4]);
    }
}
