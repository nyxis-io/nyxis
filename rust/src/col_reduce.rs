//! Dense columnar `f64` reductions with runtime SIMD dispatch (open-core).

/// True when every record index `0..n` is set in the column null bitmap.
pub fn null_bitmap_dense(bm: &[u8], n: usize) -> bool {
    if n == 0 {
        return true;
    }
    let need = (n + 7) / 8;
    if bm.len() < need {
        return false;
    }
    let full = n / 8;
    for &b in &bm[..full] {
        if b != 0xFF {
            return false;
        }
    }
    let rem = n % 8;
    if rem == 0 {
        return true;
    }
    let mask = (1u8 << rem) - 1;
    bm[full] & mask == mask
}

/// Sum `f64` column values; uses SIMD on a dense null bitmap.
pub fn sum_f64_column(vals: &[u8], bm: &[u8], n: usize) -> f64 {
    if n == 0 {
        return 0.0;
    }
    if null_bitmap_dense(bm, n) && vals.len() >= n.saturating_mul(8) {
        sum_f64_dense_le(vals, n)
    } else {
        sum_f64_sparse(vals, bm, n)
    }
}

#[inline]
fn col_bit(bm: &[u8], rec: usize) -> bool {
    (bm[rec / 8] >> (rec % 8)) & 1 == 1
}

fn sum_f64_sparse(vals: &[u8], bm: &[u8], n: usize) -> f64 {
    let mut sum = 0.0;
    for i in 0..n {
        if col_bit(bm, i) {
            let off = i * 8;
            if let Some(chunk) = vals.get(off..off + 8) {
                sum += f64::from_le_bytes(chunk.try_into().unwrap_or([0; 8]));
            }
        }
    }
    sum
}

fn sum_f64_dense_le(vals: &[u8], n: usize) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("avx2") {
            return unsafe { sum_f64_avx2(vals, n) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return unsafe { sum_f64_neon(vals, n) };
        }
    }
    sum_f64_scalar(vals, n)
}

fn sum_f64_scalar(vals: &[u8], n: usize) -> f64 {
    let mut sum = 0.0;
    for i in 0..n {
        let off = i * 8;
        if let Some(chunk) = vals.get(off..off + 8) {
            sum += f64::from_le_bytes(chunk.try_into().unwrap_or([0; 8]));
        }
    }
    sum
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn sum_f64_avx2(vals: &[u8], n: usize) -> f64 {
    use std::arch::x86_64::*;

    let ptr = vals.as_ptr() as *const f64;
    let mut i = 0usize;
    let mut a0 = _mm256_setzero_pd();
    let mut a1 = _mm256_setzero_pd();
    let mut a2 = _mm256_setzero_pd();
    let mut a3 = _mm256_setzero_pd();

    while i + 16 <= n {
        let p = ptr.add(i);
        a0 = _mm256_add_pd(a0, _mm256_loadu_pd(p));
        a1 = _mm256_add_pd(a1, _mm256_loadu_pd(p.add(4)));
        a2 = _mm256_add_pd(a2, _mm256_loadu_pd(p.add(8)));
        a3 = _mm256_add_pd(a3, _mm256_loadu_pd(p.add(12)));
        i += 16;
    }

    let mut tail = hsum_pd256(_mm256_add_pd(_mm256_add_pd(a0, a1), _mm256_add_pd(a2, a3)));
    while i < n {
        tail += ptr.add(i).read_unaligned();
        i += 1;
    }
    tail
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn hsum_pd256(v: std::arch::x86_64::__m256d) -> f64 {
    use std::arch::x86_64::*;
    let mut tmp = [0.0f64; 4];
    _mm256_storeu_pd(tmp.as_mut_ptr(), v);
    tmp.iter().sum()
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn sum_f64_neon(vals: &[u8], n: usize) -> f64 {
    use std::arch::aarch64::*;

    let ptr = vals.as_ptr() as *const f64;
    let mut i = 0usize;
    let mut a0 = vdupq_n_f64(0.0);
    let mut a1 = vdupq_n_f64(0.0);
    let mut a2 = vdupq_n_f64(0.0);
    let mut a3 = vdupq_n_f64(0.0);

    while i + 8 <= n {
        let p = ptr.add(i);
        a0 = vaddq_f64(a0, vld1q_f64(p));
        a1 = vaddq_f64(a1, vld1q_f64(p.add(2)));
        a2 = vaddq_f64(a2, vld1q_f64(p.add(4)));
        a3 = vaddq_f64(a3, vld1q_f64(p.add(6)));
        i += 8;
    }

    let mut tail = vaddvq_f64(vaddq_f64(vaddq_f64(a0, a1), vaddq_f64(a2, a3)));
    while i < n {
        tail += ptr.add(i).read_unaligned();
        i += 1;
    }
    tail
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le_bytes(values: &[f64]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn dense_bitmap_detect() {
        let bm = vec![0xFFu8; 125_000];
        assert!(null_bitmap_dense(&bm, 1_000_000));
        let mut sparse = bm.clone();
        sparse[100] = 0xFE;
        assert!(!null_bitmap_dense(&sparse, 1_000_000));
    }

    #[test]
    fn sum_matches_scalar() {
        let vals: Vec<f64> = (0..256).map(|i| i as f64 * 0.5).collect();
        let raw = le_bytes(&vals);
        let bm = vec![0xFFu8; 32];
        let want: f64 = vals.iter().sum();
        let got = sum_f64_column(&raw, &bm, vals.len());
        assert!((got - want).abs() < 1e-9, "got {got} want {want}");
    }
}
