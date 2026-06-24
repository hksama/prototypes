#![allow(warnings)]
use std::{arch::x86_64::*, mem};

#[target_feature(enable = "avx2")]
unsafe fn add_avx2(a: &[u32], b: &[u32], out: &mut [u32]) {
    let len = a.len();

    let mut i = 0;

    while i + 8 <= len {
        let va =
            _mm256_loadu_si256(a.as_ptr().add(i) as *const __m256i);

        let vb =
            _mm256_loadu_si256(b.as_ptr().add(i) as *const __m256i);

        let vc =
            _mm256_add_epi32(va, vb);

        _mm256_storeu_si256(
            out.as_mut_ptr().add(i) as *mut __m256i,
            vc,
        );

        i += 8;
    }

    // while i < len {
    //     out[i] = a[i] + b[i];
    //     i += 1;
    // }
}

unsafe fn memcmp(a:&[u64],b:&[u64],out:&mut [u64]) {
    let len = a.len();
    if a.len() != b.len() {
        // return false;
    }

    // load into __m256i registers
    let va = _mm256_loadu_si256(a.as_ptr() as *const __m256i);
    let vb = _mm256_loadu_si256(b.as_ptr() as *const __m256i);
    let cmp =  _mm256_cmpeq_epi8(va,vb);
    _mm256_storeu_si256(out.as_mut_ptr() as *mut __m256i, cmp);
    print!("va: {:?}\n", va);
    print!("vb: {:?}\n", vb);
    print!("cmp: {:?}\n", cmp);
    println!("{:02x?}", out);





    // scalar implementation
    // for i in 0..a.len() {
    //     if a[i] != b[i] {
    //         return false;
    //     }
    // }
    // true
     
}




fn main() {
    let a = vec![1u32; 16];
    let b = vec![2u32; 16];
    let mut out = vec![0u64; 8];

    unsafe {
        // add_avx2(&a, &b, &mut out);
        // memcmp(&[1u8; 32], &[4u8; 32]);
        memcmp([1u64,2u64,3u64,4u64].as_ref(), [4u64,3u64,3u64,2u64].as_ref(),&mut out);
    }

    // println!("{:?}", out);
}
