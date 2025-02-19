use std::fmt::{Display, Formatter};

use crate::{tensor::Tensor, MlResult};

#[derive(Debug, Clone)]
pub enum LossError {
    InvalidShape {
        expected: Vec<usize>,
        got: Vec<usize>,
    },
    InvalidOperation {
        op: &'static str,
        reason: String,
    },
}
impl std::error::Error for LossError {}

impl Display for LossError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LossError::InvalidShape { expected, got } => {
                write!(f, "Invalid shape: expected {:?}, got {:?}", expected, got)
            }
            LossError::InvalidOperation { op, reason } => {
                write!(f, "Invalid operation: {} ({})", op, reason)
            }
        }
    }
}

pub fn calculate_mse_loss(predictions: &Tensor, labels: &Tensor) -> MlResult<f32> {
    if predictions.shape() != labels.shape() {
        return Err(LossError::InvalidShape {
            expected: predictions.shape().to_vec(),
            got: labels.shape().to_vec(),
        }
        .into());
    }

    let diff = predictions.sub(labels)?;
    let squared = diff.data().iter().map(|&x| x * x).sum::<f32>();
    Ok(squared / (predictions.data().len() as f32))
}

pub fn calculate_cross_entropy_loss(predictions: &Tensor, targets: &Tensor) -> MlResult<f32> {
    let epsilon = 1e-15; // Small constant to prevent log(0)

    // Clip predictions to prevent numerical instability
    let clipped_preds = predictions.clip(epsilon, 1.0 - epsilon)?;

    // Calculate -y * log(p) - (1-y) * log(1-p)
    let log_probs = clipped_preds.log()?;
    let log_neg_probs = clipped_preds.neg()?.add_scalar(1.0)?.log()?;

    let term1 = targets.mul(&log_probs)?;
    let term2 = targets.neg()?.add_scalar(1.0)?.mul(&log_neg_probs)?;

    let losses = term1.add(&term2)?;
    let mean_loss = losses.neg()?.mean()?;

    Ok(mean_loss)
}

/// Computes the Binary Cross Entropy Loss between predictions and targets
/// predictions: predicted probabilities (should be between 0 and 1)
/// targets: binary labels (0 or 1)
pub fn calculate_binary_cross_entropy_loss(
    predictions: &Tensor,
    targets: &Tensor,
) -> MlResult<f32> {
    if predictions.shape() != targets.shape() {
        return Err(LossError::InvalidShape {
            expected: predictions.shape().to_vec(),
            got: targets.shape().to_vec(),
        }
        .into());
    }

    let epsilon = 1e-15; // Small constant to prevent log(0)

    // Clip predictions to prevent numerical instability
    let clipped_preds = predictions.clip(epsilon, 1.0 - epsilon)?;

    // BCE formula: -1/N * Σ(y * log(p) + (1-y) * log(1-p))
    let log_probs = clipped_preds.log()?;
    let neg_preds = clipped_preds.neg()?.add_scalar(1.0)?;
    let log_neg_probs = neg_preds.log()?;

    let neg_targets = targets.neg()?.add_scalar(1.0)?;

    let term1 = targets.mul(&log_probs)?;
    let term2 = neg_targets.mul(&log_neg_probs)?;

    let sum = term1.add(&term2)?;
    let mean_loss = sum.mean()?;

    Ok(-mean_loss)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tensor::Tensor;

    // MSE Loss Tests
    #[test]
    fn test_mse_perfect_prediction() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![1.0, 0.0, 1.0]])?;
        let targets = Tensor::new(vec![vec![1.0, 0.0, 1.0]])?;

        let loss = calculate_mse_loss(&predictions, &targets)?;
        assert!((loss - 0.0).abs() < 1e-5);
        Ok(())
    }

    #[test]
    fn test_mse_worst_prediction() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![1.0, 1.0]])?;
        let targets = Tensor::new(vec![vec![0.0, 0.0]])?;

        let loss = calculate_mse_loss(&predictions, &targets)?;
        assert!((loss - 1.0).abs() < 1e-5); // Should be 1.0 for completely wrong predictions
        Ok(())
    }

    #[test]
    fn test_mse_partial_error() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![0.5, 0.5]])?;
        let targets = Tensor::new(vec![vec![1.0, 0.0]])?;

        let loss = calculate_mse_loss(&predictions, &targets)?;
        assert!((loss - 0.25).abs() < 1e-5); // (0.5^2 + 0.5^2) / 2 = 0.25
        Ok(())
    }

    #[test]
    fn test_mse_invalid_shapes() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![1.0, 0.0]])?;
        let targets = Tensor::new(vec![vec![1.0]])?;

        let result = calculate_mse_loss(&predictions, &targets);
        assert!(result.is_err());
        Ok(())
    }

    // Cross Entropy Loss Tests
    #[test]
    fn test_cross_entropy_perfect_prediction() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![0.9999, 0.0001]])?;
        let targets = Tensor::new(vec![vec![1.0, 0.0]])?;

        let loss = calculate_cross_entropy_loss(&predictions, &targets)?;
        assert!((loss - 0.0).abs() < 1e-3);
        Ok(())
    }

    #[test]
    fn test_cross_entropy_worst_prediction() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![0.0001, 0.9999]])?;
        let targets = Tensor::new(vec![vec![1.0, 0.0]])?;

        let loss = calculate_cross_entropy_loss(&predictions, &targets)?;
        assert!(loss > 5.0); // Should be a large number for wrong predictions
        Ok(())
    }

    #[test]
    fn test_cross_entropy_uncertain_prediction() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![0.5, 0.5]])?;
        let targets = Tensor::new(vec![vec![1.0, 0.0]])?;

        let loss = calculate_cross_entropy_loss(&predictions, &targets)?;
        assert!((loss - 0.693).abs() < 1e-3); // ln(2) ≈ 0.693
        Ok(())
    }

    // Binary Cross Entropy Loss Tests (existing tests)
    #[test]
    fn test_binary_cross_entropy_perfect_prediction() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![0.9999, 0.0001]])?;
        let targets = Tensor::new(vec![vec![1.0, 0.0]])?;

        let loss = calculate_binary_cross_entropy_loss(&predictions, &targets)?;
        assert!((loss - 0.0).abs() < 1e-3);
        Ok(())
    }

    #[test]
    fn test_binary_cross_entropy_worst_prediction() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![0.0, 1.0]])?;
        let targets = Tensor::new(vec![vec![1.0, 0.0]])?;

        let loss = calculate_binary_cross_entropy_loss(&predictions, &targets)?;
        assert!(loss > 10.0); // Should be a large number for completely wrong predictions
        Ok(())
    }

    #[test]
    fn test_binary_cross_entropy_uncertain_prediction() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![0.5, 0.5]])?;
        let targets = Tensor::new(vec![vec![1.0, 0.0]])?;

        let loss = calculate_binary_cross_entropy_loss(&predictions, &targets)?;
        assert!((loss - 0.693).abs() < 1e-3); // ln(2) ≈ 0.693
        Ok(())
    }

    #[test]
    fn test_binary_cross_entropy_invalid_shapes() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![1.0, 0.0]])?;
        let targets = Tensor::new(vec![vec![1.0]])?;

        let result = calculate_binary_cross_entropy_loss(&predictions, &targets);
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn test_binary_cross_entropy_batch() -> MlResult<()> {
        let predictions = Tensor::new(vec![vec![0.9, 0.1], vec![0.1, 0.9]])?;
        let targets = Tensor::new(vec![vec![1.0, 0.0], vec![0.0, 1.0]])?;

        let loss = calculate_binary_cross_entropy_loss(&predictions, &targets)?;
        assert!(loss > 0.0 && loss < 0.5); // Loss should be small but positive
        Ok(())
    }
}
