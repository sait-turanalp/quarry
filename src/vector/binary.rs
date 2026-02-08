//! Binary quantization for fast approximate nearest neighbor pre-filtering.
//!
//! Converts 384-dim float embeddings to 384-bit binary vectors (48 bytes vs 1536 bytes).
//! Uses Hamming distance (XOR + popcount) for ~32x memory reduction and fast pre-filtering
//! before full cosine similarity on top-K candidates.

/// Binary quantized vector using sign-bit quantization.
///
/// Each f32 dimension maps to one bit: positive → 1, non-positive → 0.
/// Stored as packed u64 words for efficient SIMD-friendly Hamming distance.
#[derive(Debug, Clone)]
pub struct BinaryVector {
    words: Vec<u64>,
    dim: usize,
}

impl BinaryVector {
    /// Quantize a float embedding to binary using sign-bit threshold.
    ///
    /// Values > 0.0 become 1, values <= 0.0 become 0.
    pub fn from_embedding(embedding: &[f32]) -> Self {
        let dim = embedding.len();
        let num_words = dim.div_ceil(64);
        let mut words = vec![0u64; num_words];

        for (i, &val) in embedding.iter().enumerate() {
            if val > 0.0 {
                let word_idx = i / 64;
                let bit_idx = i % 64;
                words[word_idx] |= 1u64 << bit_idx;
            }
        }

        Self { words, dim }
    }

    /// Compute Hamming distance (number of differing bits) between two binary vectors.
    ///
    /// Lower distance = more similar. For 384-dim, range is [0, 384].
    pub fn hamming_distance(&self, other: &BinaryVector) -> u32 {
        self.words
            .iter()
            .zip(other.words.iter())
            .map(|(a, b)| (a ^ b).count_ones())
            .sum()
    }

    /// Memory footprint in bytes.
    pub fn size_bytes(&self) -> usize {
        self.words.len() * std::mem::size_of::<u64>()
    }

    /// Number of dimensions this binary vector represents.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Serialize to bytes: [u32 dim][u64... words]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(4 + self.words.len() * 8);
        buf.extend_from_slice(&(self.dim as u32).to_le_bytes());
        for &w in &self.words {
            buf.extend_from_slice(&w.to_le_bytes());
        }
        buf
    }

    /// Deserialize from bytes. Returns None if buffer is too short.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }
        let dim = u32::from_le_bytes(data[0..4].try_into().ok()?) as usize;
        let num_words = dim.div_ceil(64);
        let expected_len = 4 + num_words * 8;
        if data.len() < expected_len {
            return None;
        }
        let mut words = Vec::with_capacity(num_words);
        for i in 0..num_words {
            let offset = 4 + i * 8;
            let w = u64::from_le_bytes(data[offset..offset + 8].try_into().ok()?);
            words.push(w);
        }
        Some(Self { words, dim })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_embedding_sign_extraction() {
        let embedding = vec![1.0, -1.0, 0.5, 0.0, -0.5, 2.0, 0.0, -3.0];
        let bv = BinaryVector::from_embedding(&embedding);
        // Expected bits: 1, 0, 1, 0, 0, 1, 0, 0 = 0b00100101 = 37
        assert_eq!(bv.words[0] & 0xFF, 0b00100101);
    }

    #[test]
    fn test_hamming_distance_identical() {
        let embedding = vec![1.0, -1.0, 0.5, -0.5];
        let bv1 = BinaryVector::from_embedding(&embedding);
        let bv2 = BinaryVector::from_embedding(&embedding);
        assert_eq!(bv1.hamming_distance(&bv2), 0);
    }

    #[test]
    fn test_hamming_distance_opposite() {
        let dim = 384;
        let pos: Vec<f32> = vec![1.0; dim];
        let neg: Vec<f32> = vec![-1.0; dim];
        let bv1 = BinaryVector::from_embedding(&pos);
        let bv2 = BinaryVector::from_embedding(&neg);
        assert_eq!(bv1.hamming_distance(&bv2), dim as u32);
    }

    #[test]
    fn test_size_bytes() {
        let embedding = vec![0.0; 384];
        let bv = BinaryVector::from_embedding(&embedding);
        // 384 bits = 6 u64 words = 48 bytes
        assert_eq!(bv.size_bytes(), 48);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let embedding: Vec<f32> = (0..384)
            .map(|i| if i % 3 == 0 { 1.0 } else { -1.0 })
            .collect();
        let bv = BinaryVector::from_embedding(&embedding);
        let bytes = bv.to_bytes();
        let bv2 = BinaryVector::from_bytes(&bytes).unwrap();
        assert_eq!(bv.hamming_distance(&bv2), 0);
        assert_eq!(bv.dim(), bv2.dim());
    }

    #[test]
    fn test_from_bytes_too_short() {
        assert!(BinaryVector::from_bytes(&[0, 0, 0]).is_none());
        assert!(BinaryVector::from_bytes(&[128, 1, 0, 0]).is_none()); // dim=384 but no data
    }
}
