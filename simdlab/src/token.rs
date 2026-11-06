//! Backend tokens: zero-sized proofs that the CPU supports a set of target features.

/// A zero-sized value that exists only if this CPU supports [`Token::FEATURES`]. Declare one with
/// [`target_token!`](crate::target_token).
///
/// SIMD operations that take a token (`self`) are safe to call: holding the token is the proof.
/// For code generation, run the SIMD code inside [`Token::vectorize`]: it enters a function
/// compiled with the token's features, and the `#[inline(always)]` operations inlined into it get
/// compiled for them (instead of becoming calls to out-of-line intrinsics).
pub trait Token: Copy + Send + Sync + std::fmt::Debug + 'static {
    /// The target features, as `#[target_feature(enable = ...)]` / [`crate::cpu`] names.
    const FEATURES: &'static [&'static str];

    /// A token, if this CPU supports every feature.
    fn detect() -> Option<Self>;

    /// Runs `f` inside a function compiled with [`Token::FEATURES`] enabled.
    fn vectorize<R>(self, f: impl FnOnce() -> R) -> R;
}

/// Declares a [`Token`] type.
///
/// ```ignore
/// simdlab::target_token!(
///     /// Scalar code: no features, always available.
///     pub struct Scalar;
/// );
/// #[cfg(target_arch = "x86_64")]
/// simdlab::target_token!(
///     /// AVX2 + BMI2.
///     pub struct Avx2: "avx2", "bmi2";
/// );
/// ```
///
/// A token without features is a unit struct anyone can construct; one with features can only be
/// obtained from [`Token::detect`] or the `unsafe fn new_unchecked`.
#[macro_export]
macro_rules! target_token {
    ($(#[$m:meta])* $vis:vis struct $name:ident;) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        $vis struct $name;

        impl $crate::Token for $name {
            const FEATURES: &'static [&'static str] = &[];
            #[inline]
            fn detect() -> Option<Self> {
                Some($name)
            }
            #[inline(always)]
            fn vectorize<R>(self, f: impl FnOnce() -> R) -> R {
                f()
            }
        }
    };
    ($(#[$m:meta])* $vis:vis struct $name:ident: $($feat:tt),+ $(,)?;) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $vis struct $name(());

        impl $name {
            /// # Safety
            /// The CPU must support every feature in [`Token::FEATURES`](`$crate::Token::FEATURES`).
            #[inline(always)]
            pub const unsafe fn new_unchecked() -> Self {
                $name(())
            }
        }

        impl $crate::Token for $name {
            const FEATURES: &'static [&'static str] = &[$($feat),+];
            #[inline]
            fn detect() -> Option<Self> {
                // std's detection macros rather than `cpu::has`: a cached bit test per feature
                // (constant `true` for features the build enables, e.g. with -C target-cpu=native),
                // so checking a token on every call costs nothing; unknown names fail to compile.
                $crate::__detect_all!($($feat),+).then_some($name(()))
            }
            #[inline(always)]
            fn vectorize<R>(self, f: impl FnOnce() -> R) -> R {
                $(#[target_feature(enable = $feat)])+
                #[inline]
                unsafe fn imp<R>(f: impl FnOnce() -> R) -> R {
                    f()
                }
                // SAFETY: a token exists only if the CPU supports FEATURES (detect / new_unchecked).
                unsafe { imp(f) }
            }
        }
    };
}

#[doc(hidden)]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[macro_export]
macro_rules! __detect_all {
    ($($feat:tt),+) => { true $(&& ::std::arch::is_x86_feature_detected!($feat))+ };
}
#[doc(hidden)]
#[cfg(target_arch = "aarch64")]
#[macro_export]
macro_rules! __detect_all {
    ($($feat:tt),+) => { true $(&& ::std::arch::is_aarch64_feature_detected!($feat))+ };
}
#[doc(hidden)]
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
#[macro_export]
macro_rules! __detect_all {
    ($($feat:tt),+) => { false };
}

/// The token, or a panic naming the missing features. For dispatch code that has already
/// checked support (e.g. a backend from [`Backends::available`](crate::Backends::available)).
#[track_caller]
pub fn require<T: Token>() -> T {
    T::detect().unwrap_or_else(|| {
        panic!("this CPU does not support {} (missing: {})", std::any::type_name::<T>(), crate::cpu::missing(T::FEATURES).join(" "))
    })
}
