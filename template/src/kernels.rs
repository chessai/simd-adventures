//! The algorithm, written once over [`SumOps`], and the scalar reference.

use crate::ops::SumOps;

/// The reference backend's token: the plain loop needs no vector operations.
#[derive(Clone, Copy, Debug, Default)]
pub struct Scalar;

/// Wrapping sum of `xs`.
pub trait Sum: Copy {
    fn sum_u32(self, xs: &[u32]) -> u32;
}

impl Sum for Scalar {
    fn sum_u32(self, xs: &[u32]) -> u32 {
        xs.iter().fold(0u32, |s, &x| s.wrapping_add(x))
    }
}

impl<O: SumOps> Sum for O {
    fn sum_u32(self, xs: &[u32]) -> u32 {
        // Everything inside `vectorize` is compiled with the token's target features.
        self.vectorize(|| {
            let (chunks, tail) = xs.as_chunks::<8>();
            let mut acc = self.zero();
            for c in chunks {
                acc = self.add(acc, self.load(c));
            }
            tail.iter().fold(self.reduce(acc), |s, &x| s.wrapping_add(x))
        })
    }
}

/// Entry point for [`crate::with_backend!`]: generic over the token type.
pub fn sum_u32<K: Sum>(k: K, xs: &[u32]) -> u32 {
    k.sum_u32(xs)
}
