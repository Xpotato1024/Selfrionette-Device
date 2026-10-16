use super::{InputError, Matrix, N, Vector};

pub(super) fn apply(matrix: &Matrix, values: &Vector) -> Result<Vector, InputError> {
    let result = std::array::from_fn(|row| {
        matrix[row]
            .iter()
            .zip(values)
            .map(|(a, b)| a * b)
            .sum::<f64>()
    });
    if result.iter().all(|v| v.is_finite()) {
        Ok(result)
    } else {
        Err(InputError::NonFinite)
    }
}

pub(super) fn norm(matrix: &Matrix) -> f64 {
    matrix
        .iter()
        .map(|row| row.iter().map(|v| v.abs()).sum::<f64>())
        .fold(0.0, f64::max)
}

// Small fixed-size, partially pivoted inverse. Never regularize silently: a
// rank-deficient response is evidence against independent seven-axis recovery.
pub(super) fn inverse(matrix: &Matrix) -> Result<Matrix, InputError> {
    let scale = norm(matrix);
    if !scale.is_finite() || scale == 0.0 {
        return Err(InputError::CoupledUnavailable("singular response"));
    }
    let mut a = [[0.0; 2 * N]; N];
    for row in 0..N {
        a[row][..N].copy_from_slice(&matrix[row]);
        a[row][N + row] = 1.0;
    }
    for col in 0..N {
        let pivot = (col..N)
            .max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))
            .ok_or(InputError::CoupledUnavailable("missing pivot"))?;
        if a[pivot][col].abs() <= 64.0 * f64::EPSILON * scale {
            return Err(InputError::CoupledUnavailable("singular response"));
        }
        a.swap(col, pivot);
        let divisor = a[col][col];
        for value in &mut a[col] {
            *value /= divisor;
        }
        let pivot_row = a[col];
        for (row, values) in a.iter_mut().enumerate() {
            if row == col {
                continue;
            }
            let multiplier = values[col];
            for (value, pivot_value) in values.iter_mut().zip(pivot_row) {
                *value -= multiplier * pivot_value;
            }
        }
    }
    let result: Matrix = std::array::from_fn(|row| std::array::from_fn(|col| a[row][N + col]));
    if result.iter().flatten().any(|v| !v.is_finite()) {
        return Err(InputError::NonFinite);
    }
    // Backward residual check in addition to the caller's condition/noise gates.
    for (row, values) in matrix.iter().enumerate() {
        for (col, _) in result[0].iter().enumerate() {
            let product: f64 = values
                .iter()
                .zip(result.iter())
                .map(|(a, b)| a * b[col])
                .sum();
            let expected = if row == col { 1.0 } else { 0.0 };
            if !product.is_finite() || (product - expected).abs() > 1e-8 {
                return Err(InputError::CoupledUnavailable("inverse residual"));
            }
        }
    }
    Ok(result)
}
