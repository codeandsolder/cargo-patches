//! SIMD-accelerated engines for the standard and URL-safe alphabets.
//!
//! These are gated behind the `simd-unsafe` feature because they use `unsafe`. Three engines are
//! provided:
//!
//! - `Simd` detects the best available instruction set at runtime and falls back to the scalar
//!   [`GeneralPurpose`] engine when none is available. It requires `std` for the detection.
//! - `Avx2` and `Neon` target a specific instruction set with no runtime detection, so they can
//!   be used in `no_std` builds when the target is known to support the instructions.
//!
//! Only the STANDARD and URL_SAFE alphabets are accelerated (they share indices `0..=61` and differ
//! only at `62`/`63`). Each engine therefore has dedicated `standard` / `url_safe` constructors
//! rather than taking an arbitrary [`Alphabet`](crate::alphabet::Alphabet); use [`GeneralPurpose`]
//! for any other alphabet.
//!
//! The kernels use nightly Rust's `core::simd` with architecture-neutral byte shuffles,
//! arithmetic and comparisons. Runtime target-feature dispatch is the only CPU-specific code.
//! WebAssembly targets with `simd128` enabled use the same kernels directly.
#![allow(unsafe_code)]

use crate::alphabet::Symbol;
#[cfg(any(feature = "alloc", test))]
use crate::{encode::encode_with_padding, encoded_len, engine::Config};
#[cfg(any(feature = "alloc", test))]
use alloc::string::String;

/// Append the complete encoding without a temporary String or a UTF-8 rescan.
///
/// # Safety
/// Only engines proven to emit ASCII may use this helper.
/// The engine must write only ASCII, including on partial writes and panic.
/// All three built-in SIMD engines use the standard or URL-safe ASCII alphabet
/// and ASCII padding, including their scalar fallback.
#[cfg(any(feature = "alloc", test))]
unsafe fn append_ascii<E: Engine>(engine: &E, input: &[u8], output: &mut String) {
    let encoded = encoded_len(input.len(), engine.config().encode_padding())
        .expect("integer overflow when calculating buffer size");
    let start = output.len();
    let end = start
        .checked_add(encoded)
        .expect("integer overflow when appending base64");
    // SAFETY: This helper is private and only called with the built-in SIMD
    // engines. Resize initializes every new byte to ASCII NUL. The encoding
    // writes only ASCII symbols, so the String remains valid UTF-8 even if
    // encoding panics. The existing prefix is never modified.
    let bytes = unsafe { output.as_mut_vec() };
    bytes.resize(end, 0);
    encode_with_padding(input, &mut bytes[start..], engine, encoded);
}

// Only used inside the three private built-in ASCII engine implementations.
macro_rules! impl_ascii_encode {
    () => {
        #[cfg(any(feature = "alloc", test))]
        fn encode<T: AsRef<[u8]>>(&self, input: T) -> String {
            let mut output = String::new();
            // SAFETY: this built-in engine writes only ASCII.
            unsafe { append_ascii(self, input.as_ref(), &mut output) };
            output
        }

        #[cfg(any(feature = "alloc", test))]
        fn encode_string<T: AsRef<[u8]>>(&self, input: T, output: &mut String) {
            // SAFETY: this built-in engine writes only ASCII.
            unsafe { append_ascii(self, input.as_ref(), output) };
        }
    };
}

use crate::{
    engine::{
        general_purpose::{
            decode::decode_helper, encode_helper, GeneralPurpose, GeneralPurposeConfig,
            GeneralPurposeEstimate,
        },
        DecodeMetadata, Engine,
    },
    DecodeSliceError,
};

/// The accelerated ASCII alphabet family, dispatched once per call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SimdKind {
    Standard,
    UrlSafe,
}

// These kernels contain only portable SIMD. The target-feature entry points
// below select a suitable instruction set, not a separate implementation.
use core::simd::{prelude::*, simd_swizzle, Simd as Vector, ToBytes};

type B32 = Vector<u8, 32>;
type W16 = Vector<u16, 16>;
type D8 = Vector<u32, 8>;

#[inline(always)]
fn encode_ascii<const URL: bool>(v: B32) -> B32 {
    let s = v.cast::<i8>();
    let base = s.simd_gt(Vector::splat(51)).select(
        B32::splat(252),
        s.simd_gt(Vector::splat(25))
            .select(B32::splat(71), B32::splat(65)),
    );
    let special = if URL {
        (v & B32::splat(1))
            .simd_eq(B32::splat(0))
            .select(B32::splat(239), B32::splat(32))
    } else {
        (v & B32::splat(1)) * B32::splat(3) + B32::splat(237)
    };
    v + s.simd_gt(Vector::splat(61)).select(special, base)
}

#[inline(always)]
fn encode_block<const URL: bool, const OVERLAP: bool>(a: B32) -> B32 {
    let s = if OVERLAP {
        simd_swizzle!(
            a,
            [
                5, 4, 6, 5, 8, 7, 9, 8, 11, 10, 12, 11, 14, 13, 15, 14, 17, 16, 18, 17, 20, 19, 21,
                20, 23, 22, 24, 23, 26, 25, 27, 26
            ]
        )
    } else {
        simd_swizzle!(
            a,
            [
                1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10, 13, 12, 14, 13, 16, 15, 17, 16,
                19, 18, 20, 19, 22, 21, 23, 22
            ]
        )
    };
    // Each pair is a little-endian word containing adjacent input bytes.
    // The masks select the two interleaved fields of each output sextet pair.
    let w = W16::from_le_bytes(s);
    let hi = w & W16::from_array([
        0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0, 0xfc00,
        0x0fc0, 0xfc00, 0x0fc0, 0xfc00, 0x0fc0,
    ]);
    let lo = w & W16::from_array([
        0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f, 0x03f0,
        0x003f, 0x03f0, 0x003f, 0x03f0, 0x003f,
    ]);
    // The masked high products are exact multiples of 2^16 when multiplied
    // by 64 or 1024. Adding 1 to each multiplier leaves the high half unchanged
    // but avoids LLVM replacing the multiply with widened variable shifts.
    let hm = W16::from_array([
        65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025, 65, 1025,
    ]);
    // Adding 0x1000 to the first low multiplier preserves the low half
    // because that masked input is divisible by 16.
    let lm = W16::from_array([
        4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256, 4112, 256,
    ]);
    let hi = ((hi.cast::<u32>() * hm.cast::<u32>()) >> 16).cast::<u16>();
    encode_ascii::<URL>((hi | (lo * lm)).to_le_bytes())
}

// The first load has no prefix. Later loads begin four bytes before the
// next triplet, allowing the shuffle to stay within each 128-bit lane.
#[inline(always)]
fn encode_prefix<const URL: bool>(input: &[u8], output: &mut [u8]) -> (usize, usize) {
    if input.len() < 32 || output.len() < 32 {
        return (0, 0);
    }
    encode_block::<URL, false>(B32::from_slice(input)).copy_to_slice(output);
    let mut i = 24;
    let mut o = 32;
    while input.len() - i >= 28 && output.len() - o >= 32 {
        encode_block::<URL, true>(B32::from_slice(&input[i - 4..])).copy_to_slice(&mut output[o..]);
        i += 24;
        o += 32;
    }
    (i, o)
}

#[inline(always)]
fn decode_block<const URL: bool>(v: B32) -> Option<B32> {
    let alpha = (v | B32::splat(32)) - B32::splat(b'a');
    let digit = v - B32::splat(b'0');
    let letters = alpha.simd_lt(B32::splat(26));
    let digits = digit.simd_lt(B32::splat(10));
    let plus = v.simd_eq(B32::splat(if URL { b'-' } else { b'+' }));
    let slash = v.simd_eq(B32::splat(if URL { b'_' } else { b'/' }));
    if !(letters | digits | plus | slash).all() {
        return None;
    }
    let low = v.cast::<i8>().simd_gt(Vector::splat(b'Z' as i8));
    let letter_offset = low.select(B32::splat(185), B32::splat(191));
    let other_offset = plus.select(
        B32::splat(if URL { 17 } else { 19 }),
        B32::splat(if URL { 224 } else { 16 }),
    );
    let offset = letters.select(letter_offset, digits.select(B32::splat(4), other_offset));
    let v = v + offset;
    let w = W16::from_le_bytes(v);
    let p = ((w & W16::splat(255)) * W16::splat(64)) | (w >> 8);
    let d = D8::from_le_bytes(p.to_le_bytes());
    let d = ((d & D8::splat(0xffff)) << 12) | (d >> 16);
    let s = d.to_be_bytes();
    Some(simd_swizzle!(
        s,
        [
            1, 2, 3, 5, 6, 7, 9, 10, 11, 13, 14, 15, 17, 18, 19, 21, 22, 23, 25, 26, 27, 29, 30,
            31, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    ))
}

// On invalid input leave the entire current vector to the scalar decoder,
// which reports the precise original character offset. Stores are exactly
// 24 bytes, including when the caller provides a larger output buffer.
#[inline(always)]
fn decode_prefix<const URL: bool>(
    input: &[u8],
    quads_end: usize,
    output: &mut [u8],
) -> (usize, usize) {
    let mut i = 0;
    let mut o = 0;
    while quads_end - i >= 32 && output.len() - o >= 24 {
        let Some(packed) = decode_block::<URL>(B32::from_slice(&input[i..])) else {
            break;
        };
        let head = simd_swizzle!(
            packed,
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        );
        let tail = simd_swizzle!(packed, [16, 17, 18, 19, 20, 21, 22, 23]);
        head.copy_to_slice(&mut output[o..]);
        tail.copy_to_slice(&mut output[o + 16..]);
        i += 32;
        o += 24;
    }
    (i, o)
}

#[inline(always)]
fn encode_dispatch(kind: SimdKind, input: &[u8], output: &mut [u8]) -> (usize, usize) {
    match kind {
        SimdKind::Standard => encode_prefix::<false>(input, output),
        SimdKind::UrlSafe => encode_prefix::<true>(input, output),
    }
}
#[inline(always)]
fn decode_dispatch(kind: SimdKind, input: &[u8], end: usize, output: &mut [u8]) -> (usize, usize) {
    match kind {
        SimdKind::Standard => decode_prefix::<false>(input, end, output),
        SimdKind::UrlSafe => decode_prefix::<true>(input, end, output),
    }
}

/// # Safety
/// The CPU must support AVX2.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn avx2_encode(kind: SimdKind, input: &[u8], output: &mut [u8]) -> (usize, usize) {
    encode_dispatch(kind, input, output)
}
/// # Safety
/// The CPU must support AVX2.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn avx2_decode(
    kind: SimdKind,
    input: &[u8],
    end: usize,
    output: &mut [u8],
) -> (usize, usize) {
    decode_dispatch(kind, input, end, output)
}
/// # Safety
/// The CPU must support NEON.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn neon_encode(kind: SimdKind, input: &[u8], output: &mut [u8]) -> (usize, usize) {
    encode_dispatch(kind, input, output)
}
/// # Safety
/// The CPU must support NEON.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn neon_decode(
    kind: SimdKind,
    input: &[u8],
    end: usize,
    output: &mut [u8],
) -> (usize, usize) {
    decode_dispatch(kind, input, end, output)
}

/// Which instruction set an engine dispatches to.
#[cfg(all(
    feature = "std",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        all(target_arch = "wasm32", target_feature = "simd128")
    )
))]
#[derive(Clone, Copy, Debug)]
enum Backend {
    #[cfg(not(target_arch = "wasm32"))]
    Scalar,
    #[cfg(target_arch = "wasm32")]
    Portable,
    #[cfg(target_arch = "x86_64")]
    Avx2,
    #[cfg(target_arch = "aarch64")]
    Neon,
}

/// A base64 engine that uses the best SIMD instruction set detected at runtime, falling back to the
/// scalar [`GeneralPurpose`] engine.
///
/// Requires `std` and an `x86_64`, NEON-enabled `aarch64`, or SIMD-enabled `wasm32` target.
/// On WebAssembly the SIMD instruction set is selected at compile time. On other targets,
/// use [`GeneralPurpose`] directly. Only the STANDARD and URL_SAFE
/// alphabets are accelerated, so it is constructed with [`Simd::standard`] / [`Simd::url_safe`].
#[cfg(all(
    feature = "std",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        all(target_arch = "wasm32", target_feature = "simd128")
    )
))]
#[derive(Debug, Clone)]
pub struct Simd {
    inner: GeneralPurpose,
    kind: SimdKind,
    backend: Backend,
}

#[cfg(all(
    feature = "std",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        all(target_arch = "wasm32", target_feature = "simd128")
    )
))]
impl Simd {
    /// Create a `Simd` engine for the STANDARD alphabet, detecting the instruction set once.
    #[must_use]
    pub fn standard(config: GeneralPurposeConfig) -> Self {
        Self::new(SimdKind::Standard, &crate::alphabet::STANDARD, config)
    }

    /// Create a `Simd` engine for the URL_SAFE alphabet, detecting the instruction set once.
    #[must_use]
    pub fn url_safe(config: GeneralPurposeConfig) -> Self {
        Self::new(SimdKind::UrlSafe, &crate::alphabet::URL_SAFE, config)
    }

    fn new(
        kind: SimdKind,
        alphabet: &crate::alphabet::Alphabet,
        config: GeneralPurposeConfig,
    ) -> Self {
        #[cfg(target_arch = "x86_64")]
        let backend = if std::is_x86_feature_detected!("avx2") {
            Backend::Avx2
        } else {
            Backend::Scalar
        };
        #[cfg(target_arch = "aarch64")]
        let backend = if std::arch::is_aarch64_feature_detected!("neon") {
            Backend::Neon
        } else {
            Backend::Scalar
        };

        #[cfg(target_arch = "wasm32")]
        let backend = Backend::Portable;
        Self {
            inner: GeneralPurpose::new(alphabet, config),
            kind,
            backend,
        }
    }
}

#[cfg(all(
    feature = "std",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        all(target_arch = "wasm32", target_feature = "simd128")
    )
))]
impl Engine for Simd {
    type Config = GeneralPurposeConfig;
    type DecodeEstimate = GeneralPurposeEstimate;

    impl_ascii_encode!();

    fn internal_encode(&self, input: &[u8], output: &mut [u8]) -> usize {
        let kind = self.kind;
        match self.backend {
            #[cfg(not(target_arch = "wasm32"))]
            Backend::Scalar => self.inner.internal_encode(input, output),
            #[cfg(target_arch = "wasm32")]
            Backend::Portable => encode_helper(self.inner.encode_table(), input, output, |i, o| {
                encode_dispatch(kind, i, o)
            }),
            #[cfg(target_arch = "x86_64")]
            Backend::Avx2 => encode_helper(self.inner.encode_table(), input, output, |i, o| {
                // SAFETY: the Avx2 backend is only selected when AVX2 was detected.
                unsafe { avx2_encode(kind, i, o) }
            }),
            #[cfg(target_arch = "aarch64")]
            Backend::Neon => encode_helper(self.inner.encode_table(), input, output, |i, o| {
                // SAFETY: the Neon backend is only selected when NEON was detected.
                unsafe { neon_encode(kind, i, o) }
            }),
        }
    }

    fn internal_decoded_len_estimate(&self, input_len: usize) -> Self::DecodeEstimate {
        self.inner.internal_decoded_len_estimate(input_len)
    }

    fn internal_decode(
        &self,
        input: &[u8],
        output: &mut [u8],
        estimate: Self::DecodeEstimate,
    ) -> Result<DecodeMetadata, DecodeSliceError> {
        let kind = self.kind;
        match self.backend {
            #[cfg(not(target_arch = "wasm32"))]
            Backend::Scalar => self.inner.internal_decode(input, output, estimate),
            #[cfg(target_arch = "wasm32")]
            Backend::Portable => decode_helper(
                input,
                &estimate,
                output,
                self.inner.decode_table(),
                self.inner.config().decode_allow_trailing_bits(),
                self.inner.padding(),
                self.inner.config().decode_padding_mode(),
                |i, end, o| decode_dispatch(kind, i, end, o),
            ),
            #[cfg(target_arch = "x86_64")]
            Backend::Avx2 => decode_helper(
                input,
                &estimate,
                output,
                self.inner.decode_table(),
                self.inner.config().decode_allow_trailing_bits(),
                self.inner.padding(),
                self.inner.config().decode_padding_mode(),
                // SAFETY: the Avx2 backend is only selected when AVX2 was detected.
                |i, end, o| unsafe { avx2_decode(kind, i, end, o) },
            ),
            #[cfg(target_arch = "aarch64")]
            Backend::Neon => decode_helper(
                input,
                &estimate,
                output,
                self.inner.decode_table(),
                self.inner.config().decode_allow_trailing_bits(),
                self.inner.padding(),
                self.inner.config().decode_padding_mode(),
                // SAFETY: the Neon backend is only selected when NEON was detected.
                |i, end, o| unsafe { neon_decode(kind, i, end, o) },
            ),
        }
    }

    fn config(&self) -> &Self::Config {
        self.inner.config()
    }

    fn padding(&self) -> Symbol {
        self.inner.padding()
    }
}

/// A base64 engine that unconditionally uses AVX2, without runtime detection.
///
/// This works in `no_std` builds. Because it does not check for AVX2 support, it must only be used
/// on a CPU that has it. Only the STANDARD and URL_SAFE alphabets are accelerated, so it is
/// constructed with the [`Avx2::standard`] / [`Avx2::url_safe`] (checked) or
/// [`Avx2::standard_unchecked`] / [`Avx2::url_safe_unchecked`] constructors.
#[cfg(target_arch = "x86_64")]
#[derive(Debug, Clone)]
pub struct Avx2 {
    inner: GeneralPurpose,
    kind: SimdKind,
}

#[cfg(target_arch = "x86_64")]
impl Avx2 {
    /// Create an `Avx2` engine for the STANDARD alphabet if the running CPU supports AVX2, else
    /// `None`.
    ///
    /// Requires the `std` feature for the detection; in `no_std` use [`Avx2::standard_unchecked`].
    #[cfg(feature = "std")]
    #[must_use]
    pub fn standard(config: GeneralPurposeConfig) -> Option<Self> {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support was just verified.
            Some(unsafe { Self::standard_unchecked(config) })
        } else {
            None
        }
    }

    /// Create an `Avx2` engine for the URL_SAFE alphabet if the running CPU supports AVX2, else
    /// `None`.
    ///
    /// Requires the `std` feature for the detection; in `no_std` use [`Avx2::url_safe_unchecked`].
    #[cfg(feature = "std")]
    #[must_use]
    pub fn url_safe(config: GeneralPurposeConfig) -> Option<Self> {
        if std::is_x86_feature_detected!("avx2") {
            // SAFETY: AVX2 support was just verified.
            Some(unsafe { Self::url_safe_unchecked(config) })
        } else {
            None
        }
    }

    /// Create an `Avx2` engine for the STANDARD alphabet without checking for AVX2 support.
    ///
    /// # Safety
    ///
    /// The CPU that will run encode/decode must support AVX2. Using the engine on a CPU without
    /// AVX2 is undefined behavior.
    #[must_use]
    pub const unsafe fn standard_unchecked(config: GeneralPurposeConfig) -> Self {
        Self {
            inner: GeneralPurpose::new(&crate::alphabet::STANDARD, config),
            kind: SimdKind::Standard,
        }
    }

    /// Create an `Avx2` engine for the URL_SAFE alphabet without checking for AVX2 support.
    ///
    /// # Safety
    ///
    /// The CPU that will run encode/decode must support AVX2. Using the engine on a CPU without
    /// AVX2 is undefined behavior.
    #[must_use]
    pub const unsafe fn url_safe_unchecked(config: GeneralPurposeConfig) -> Self {
        Self {
            inner: GeneralPurpose::new(&crate::alphabet::URL_SAFE, config),
            kind: SimdKind::UrlSafe,
        }
    }
}

#[cfg(target_arch = "x86_64")]
impl Engine for Avx2 {
    type Config = GeneralPurposeConfig;
    type DecodeEstimate = GeneralPurposeEstimate;

    impl_ascii_encode!();

    fn internal_encode(&self, input: &[u8], output: &mut [u8]) -> usize {
        let kind = self.kind;
        encode_helper(self.inner.encode_table(), input, output, |i, o| {
            // SAFETY: constructing this engine asserts AVX2 support.
            unsafe { avx2_encode(kind, i, o) }
        })
    }

    fn internal_decoded_len_estimate(&self, input_len: usize) -> Self::DecodeEstimate {
        self.inner.internal_decoded_len_estimate(input_len)
    }

    fn internal_decode(
        &self,
        input: &[u8],
        output: &mut [u8],
        estimate: Self::DecodeEstimate,
    ) -> Result<DecodeMetadata, DecodeSliceError> {
        let kind = self.kind;
        decode_helper(
            input,
            &estimate,
            output,
            self.inner.decode_table(),
            self.inner.config().decode_allow_trailing_bits(),
            self.inner.padding(),
            self.inner.config().decode_padding_mode(),
            // SAFETY: constructing this engine asserts AVX2 support.
            |i, end, o| unsafe { avx2_decode(kind, i, end, o) },
        )
    }

    fn config(&self) -> &Self::Config {
        self.inner.config()
    }

    fn padding(&self) -> Symbol {
        self.inner.padding()
    }
}

/// A base64 engine that unconditionally uses NEON, without runtime detection.
///
/// This engine is available on aarch64 targets compiled with NEON support and works in `no_std`.
#[cfg(target_arch = "aarch64")]
#[derive(Debug, Clone)]
pub struct Neon {
    inner: GeneralPurpose,
    kind: SimdKind,
}

#[cfg(target_arch = "aarch64")]
impl Neon {
    /// Create a `Neon` engine for the STANDARD alphabet on a target compiled with NEON support.
    #[must_use]
    pub const fn standard(config: GeneralPurposeConfig) -> Self {
        Self {
            inner: GeneralPurpose::new(&crate::alphabet::STANDARD, config),
            kind: SimdKind::Standard,
        }
    }

    /// Create a `Neon` engine for the URL_SAFE alphabet on a target compiled with NEON support.
    #[must_use]
    pub const fn url_safe(config: GeneralPurposeConfig) -> Self {
        Self {
            inner: GeneralPurpose::new(&crate::alphabet::URL_SAFE, config),
            kind: SimdKind::UrlSafe,
        }
    }
}

#[cfg(target_arch = "aarch64")]
impl Engine for Neon {
    type Config = GeneralPurposeConfig;
    type DecodeEstimate = GeneralPurposeEstimate;

    impl_ascii_encode!();

    fn internal_encode(&self, input: &[u8], output: &mut [u8]) -> usize {
        let kind = self.kind;
        encode_helper(self.inner.encode_table(), input, output, |i, o| {
            // SAFETY: this module is only compiled for targets with NEON enabled.
            unsafe { neon_encode(kind, i, o) }
        })
    }

    fn internal_decoded_len_estimate(&self, input_len: usize) -> Self::DecodeEstimate {
        self.inner.internal_decoded_len_estimate(input_len)
    }

    fn internal_decode(
        &self,
        input: &[u8],
        output: &mut [u8],
        estimate: Self::DecodeEstimate,
    ) -> Result<DecodeMetadata, DecodeSliceError> {
        let kind = self.kind;
        decode_helper(
            input,
            &estimate,
            output,
            self.inner.decode_table(),
            self.inner.config().decode_allow_trailing_bits(),
            self.inner.padding(),
            self.inner.config().decode_padding_mode(),
            // SAFETY: this module is only compiled for targets with NEON enabled.
            |i, end, o| unsafe { neon_decode(kind, i, end, o) },
        )
    }

    fn config(&self) -> &Self::Config {
        self.inner.config()
    }

    fn padding(&self) -> Symbol {
        self.inner.padding()
    }
}

#[cfg(all(test, feature = "std"))]
mod portable_regressions {
    use crate::{
        engine::{
            general_purpose::{GeneralPurpose, GeneralPurposeConfig, NO_PAD, PAD},
            Simd,
        },
        Engine,
    };
    fn compare(engine: &impl Engine, scalar: &GeneralPurpose) {
        let mut data = [0u8; 2080];
        let mut state = 0x12345678u32;
        for x in &mut data {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *x = state as u8;
        }
        let mut output = [0u8; 2800];
        let mut recovered = [0u8; 2081];
        for offset in 0..32 {
            for len in 0..=2048 {
                let input = &data[offset..offset + len];
                let expected = scalar.encode(input);
                let n = expected.len();
                output.fill(0xa5);
                recovered.fill(0xa5);
                assert_eq!(
                    engine.encode_slice(input, &mut output[offset..]).unwrap(),
                    n
                );
                assert_eq!(&output[offset..offset + n], expected.as_bytes());
                assert_eq!(output[offset + n], 0xa5);
                assert_eq!(engine.encode(input), expected);
                let mut prefix = String::from("α🌍:");
                engine.encode_string(input, &mut prefix);
                assert_eq!(prefix, format!("α🌍:{expected}"));
                assert_eq!(
                    engine
                        .decode_slice(&output[offset..offset + n], &mut recovered[offset..])
                        .unwrap(),
                    len
                );
                assert_eq!(&recovered[offset..offset + len], input);
                assert_eq!(recovered[offset + len], 0xa5);
            }
        }
    }
    #[test]
    fn boundaries() {
        for config in [PAD, NO_PAD] {
            let standard = GeneralPurpose::new(&crate::alphabet::STANDARD, config);
            let url = GeneralPurpose::new(&crate::alphabet::URL_SAFE, config);
            compare(&Simd::standard(config), &standard);
            compare(&Simd::url_safe(config), &url);
            #[cfg(target_arch = "x86_64")]
            if let (Some(a), Some(b)) = (
                crate::engine::Avx2::standard(config),
                crate::engine::Avx2::url_safe(config),
            ) {
                compare(&a, &standard);
                compare(&b, &url);
            }
        }
    }
    #[test]
    fn exhaustive_triplets() {
        let mut all = Vec::with_capacity(3 * (1 << 24));
        for x in 0u32..(1 << 24) {
            all.extend_from_slice(&x.to_le_bytes()[..3]);
        }
        for url in [false, true] {
            let engine = if url {
                Simd::url_safe(PAD)
            } else {
                Simd::standard(PAD)
            };
            let scalar = GeneralPurpose::new(
                if url {
                    &crate::alphabet::URL_SAFE
                } else {
                    &crate::alphabet::STANDARD
                },
                PAD,
            );
            let expected = scalar.encode(&all);
            let mut encoded = vec![0; expected.len()];
            let mut decoded = vec![0; all.len()];
            assert_eq!(
                engine.encode_slice(&all, &mut encoded).unwrap(),
                encoded.len()
            );
            assert!(encoded == expected.as_bytes());
            assert_eq!(
                engine.decode_slice(&encoded, &mut decoded).unwrap(),
                all.len()
            );
            assert!(decoded == all);
        }
    }
    #[test]
    fn malformed_exact_errors() {
        for padding in [
            crate::engine::DecodePaddingMode::RequireCanonical,
            crate::engine::DecodePaddingMode::RequireNone,
            crate::engine::DecodePaddingMode::Indifferent,
        ] {
            for trailing in [false, true] {
                let config = GeneralPurposeConfig::new()
                    .with_decode_padding_mode(padding)
                    .with_decode_allow_trailing_bits(trailing);
                for url in [false, true] {
                    let engine = if url {
                        Simd::url_safe(config)
                    } else {
                        Simd::standard(config)
                    };
                    let scalar = GeneralPurpose::new(
                        if url {
                            &crate::alphabet::URL_SAFE
                        } else {
                            &crate::alphabet::STANDARD
                        },
                        config,
                    );
                    let mut s = [b'A'; 132];
                    for i in 0..s.len() {
                        for b in 0..=255u8 {
                            s[i] = b;
                            let a = scalar.decode(s);
                            let b = engine.decode(s);
                            assert_eq!(a, b, "offset={i} byte={} url={url}", s[i]);
                        }
                        s[i] = b'A';
                    }
                    for s in [
                        "", "A", "AA", "AAA", "AAAA", "A=", "AA=", "AAA=", "AA==", "A===", "====",
                        "AA=A", "AAAA=", "AAAA==", "AB==", "AAB=", "AB", "AAB",
                    ] {
                        assert_eq!(scalar.decode(s), engine.decode(s));
                    }
                }
            }
        }
    }
}
