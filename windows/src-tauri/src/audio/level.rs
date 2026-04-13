/// Calculate RMS level from i16 PCM samples, normalized to 0.0..1.0.
///
/// Returns 0.0 for empty input. The result is clamped to [0.0, 1.0].
pub fn calculate_rms_level(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_sq: f64 = samples
        .iter()
        .map(|&s| {
            let v = s as f64;
            v * v
        })
        .sum();

    let rms = (sum_sq / samples.len() as f64).sqrt();
    // Normalize against i16 max (32768.0)
    let normalized = (rms / 32768.0) as f32;
    normalized.clamp(0.0, 1.0)
}

/// Apply exponential moving average smoothing.
///
/// `alpha` controls how fast the output tracks the input:
/// - 1.0 = no smoothing (output = current)
/// - 0.0 = frozen (output = previous)
/// - Typical value: 0.3..0.5
pub fn smooth_level(current: f32, previous: f32, alpha: f32) -> f32 {
    alpha * current + (1.0 - alpha) * previous
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_samples_returns_zero() {
        assert_eq!(calculate_rms_level(&[]), 0.0);
    }

    #[test]
    fn silence_returns_zero() {
        assert_eq!(calculate_rms_level(&[0, 0, 0, 0]), 0.0);
    }

    #[test]
    fn max_amplitude_returns_near_one() {
        let samples = vec![i16::MAX; 100];
        let level = calculate_rms_level(&samples);
        // i16::MAX = 32767, so 32767/32768 ≈ 0.99997
        assert!(level > 0.99, "expected near 1.0, got {}", level);
    }

    #[test]
    fn smoothing_identity() {
        // alpha=1.0 means no smoothing
        assert_eq!(smooth_level(0.8, 0.2, 1.0), 0.8);
    }

    #[test]
    fn smoothing_frozen() {
        // alpha=0.0 means output tracks previous
        assert_eq!(smooth_level(0.8, 0.2, 0.0), 0.2);
    }

    #[test]
    fn smoothing_blended() {
        let result = smooth_level(1.0, 0.0, 0.3);
        assert!((result - 0.3).abs() < 1e-6);
    }
}
