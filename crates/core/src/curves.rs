/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
//! Utility Curves - pure functions on a unit interval that modulate Consideration responses.
//! 
//! There is nothing particularly special or complex about these; 
//! they are, by and large, a subset of Bevy's generic Curves. 
//! 
//! For Utility AI purposes, all Curves have a unit interval domain (i.e. 0.0 to 1.0), 
//! and a range of values that is ALSO a unit interval (visually, forming a 1x1 square).
//! On top of that most of them are fairly simple and cheap (by necessity).
//! 
//! The main purpose of these Curves is to make Considerations more *expressive*.
//! 
//! A Consideration will give us the current Health, but should the Action score 
//! be higher (e.g. for RecklessAttack) or lower (e.g. for Heal) the higher the 
//! Health value is? Or perhaps the highest score should be around the middle? 
//! And even if higher is better, does 10% higher == 10% better, or is it nonlinear?
//! 
//! Curves provide us with the tool to handle this by mapping from the input to the output smoothly.
//! 
//! To use a Curve, simply include its key in the ActionSet JSON data for some object. 
//! The Consideration inputs will be automatically scaled to the right input range and 
//! fed through the Curve by the scoring system.
//! 
//! If you plan on using custom Curves, you will need to register them into the library 
//! by initializing the `UtilityCurveRegistry` Resource with `app.add_resource()` and 
//! using the `UtilityCurveRegistry.register_curve()` method with some key (as long 
//! as it does not conflict with a built-in key!)
//! 
//! The most important items in this module, in descending order, are:
//! 1) The `UtilityCurveRegistry` Resource, which allows you to register your own Curves
//! 2) The `UtilityCurve` trait which defines what a 'registrable' Curve is for the `UtilityCurveRegistry`
//! 3) The `SupportedUtilityCurve` enum which holds all built-in Utility Curves - those are a curated 
//!    selection of building blocks that should cover the majority of your needs. They are also slightly 
//!    more performant due to the lack of an Arc<T> overhead and a secondary registry lookup.
//! 4) The methods of UtilityCurveExt - if you are planning to create custom Utility Curves, this  
//!    trait provides constructors for various transforms that are still valid as Utility Curves.
use bevy::math::{self, curve::CurveExt, Curve, curve::Interval};
use bevy::platform::prelude::{String, ToOwned};
use bevy::platform::sync::Arc;
use crate::types::{ActionScore, CraniumKvMap, MIN_CONSIDERATION_SCORE, MAX_CONSIDERATION_SCORE};

// Reexporting some common basic Bevy Curves for easy access when building custom user Curves.
pub use bevy::math::curve::{LinearCurve, QuadraticInCurve, QuadraticInOutCurve, ExponentialInCurve, CubicInCurve};

/// Curve functions suitable for Utility scoring purposes.
/// 
/// A strict subset of Bevy's Curve trait.
/// 
/// All eligible functions must have a unity domain (i.e. <0.0; 1.0>) **AND* an output range 
/// of unity as well, or at least you must be willing to allow them to be clamped to this range 
/// by using the `UtilityCurve::sample_safe(&self, t)` method provided.
/// 
/// The datatype is also fixed to use whatever the ActionScore is implemented as.
pub trait UtilityCurve: Curve<ActionScore> + Send + Sync {
    /// The interval over which this curve is parametrized.
    /// 
    /// This is the range of values of t where we can sample the curve and receive valid output.
    /// 
    /// **DO NOT** override the default impl here! 
    /// 
    /// If you see a conflicting value in impls - good, that means the trait was implemented 
    /// for something that shouldn't be a UtilityCurve and you spotted it before it became a
    /// major headache.
    fn domain(&self) -> Interval {
        Interval::UNIT
    }

    /// **IMPORTANT!** Use this method for sampling for Utility purposes.
    /// 
    /// Sample a given point on the curve, clamping **both** the input and output values to a unit square.
    /// This is subtly different from `Curve::sample_clamped()` as that only clamps the *input* value.
    /// 
    /// For Utility purposes, the output must be on the unit interval as well, or you will Cause Trouble.
    fn sample_safe(&self, t: ActionScore) -> ActionScore {
        let clampin = Interval::UNIT.clamp(t);
        let raw = self.sample_unchecked(clampin);
        let clampout = raw.clamp(MIN_CONSIDERATION_SCORE, MAX_CONSIDERATION_SCORE);
        clampout
    }
}

pub trait UtilityCurveExt: UtilityCurve + Sized {
    /// Creates a new Curve whose output is the average of the output of this Curve and the input Curve.
    fn average_with<O: UtilityCurve>(self, other: O) -> AverageCurve<Self, O> {
        AverageCurve::from((self, other))
    }

    /// Creates a new, 'peaky' Curve by mirroring the shape around the halfway point. 
    /// The domain of the resulting curve is still a unit interval. 
    fn halfway_mirror(self) -> HalfwayMirrorCurve<Self> {
        HalfwayMirrorCurve::from(self)
    }

    /// Creates a new Curve that has the same shape as this Curve, but squished above a provided 
    /// 'floor' of Utility - e.g. `c.soft_leak(0.1)` will always output AT LEAST 0.1 Utility. 
    fn soft_leak(self, gain: ActionScore) -> SoftLeak<Self> {
        SoftLeak::new(self, gain)
    }

    /// Creates a new Curve that has the same shape as this Curve, but clipped above a provided 
    /// 'floor' of Utility - e.g. `c.hard_leak(0.1)` will always output AT LEAST 0.1 Utility. 
    fn hard_leak(self, gain: ActionScore) -> HardLeak<Self> {
        HardLeak::new(self, gain)
    }

    fn inverse_samples(self) -> UtilityCurveSampler<Self> {
        UtilityCurveSampler::new_inverse(self)
    }
}

impl<T: UtilityCurve + Sized> UtilityCurveExt for T {}

/// A curve with a constant, user-defined value. 
/// 
/// Will return the same score when sampled anywhere.
#[derive(Debug, Clone)]
pub struct UtilityConstantCurve {
    val: ActionScore
}

impl UtilityConstantCurve {
    /// Create a constant curve, which always produces the given value when sampled.
    /// This constructor is fallible - it will return an error if the constant value 
    /// is outside of the range of values valid for a Utility Curve.
    pub fn new(value: ActionScore) -> Result<Self, ()> {
        match Interval::UNIT.contains(value) {
            true => Ok(Self {val: value}),
            false => Err(())
        }
    }

    /// Create a constant curve, which always produces the given value when sampled.
    /// Does not check if the value provided is valid for Utility purposes - this is 
    /// faster, but may cause weirdness if someone feeds in a junk value.
    pub fn new_unchecked(value: ActionScore) -> Self {
        Self { val: value }
    }

    /// Create a constant curve, which always produces the given value when sampled.
    /// Ensures that the constant value is valid by clamping it.
    pub fn new_clamped(value: ActionScore) -> Self {
        Self { val: Interval::UNIT.clamp(value) }
    }

    /// Create a constant curve, which always produces the given value when sampled.
    /// 
    /// The value is fully const and always safe to construct (enforced at type-level).
    /// 
    /// The value is effectively calculated as (VAL/256), e.g.
    /// - UtilityConstantCurve<0> will return 0.00, 
    /// - UtilityConstantCurve<64> will return 0.25.
    /// - UtilityConstantCurve<128> will return 0.50.
    /// - UtilityConstantCurve<255> will return 1.00 (special case).
    /// 
    /// This means we can conveniently represent all common predefined values as u8 
    /// easily, at the low, low price of sacrificing 0.99609375 as unrepresentable.
    /// If you are hung up about it, use UtilityConstantCurve for it instead.
    pub const fn new_const(value: u8) -> Self {
        Self {
            val: match value {
                0 => 0.,
                255 => 1.,
                mid => (mid as ActionScore) / 256.
            }
        }
    }
}

impl Curve<ActionScore> for UtilityConstantCurve {
    fn domain(&self) -> Interval {
        Interval::UNIT
    }

    fn sample_unchecked(&self, _: f32) -> ActionScore {
        self.val
    }
}

// Trivial impl since it just marks that we've ensured the invariants hold.
impl UtilityCurve for UtilityConstantCurve {}


/// A Curve that returns max Utility for any `t >= 1.0` (where t is the *normalized* score).
/// 
/// In other words, if the **un**-normalized input is at or above the Consideration's 
/// Max value, the curve returns 1.0, otherwise returns 0.0.
/// 
/// You can think of this as a Curve equivalent of a simple if-statement, or 
/// more physically, as a diode with a cut-in threshold of Consideration Max.
/// 
/// This is easily the cheapest practically usable Utility Curve; as such it's 
/// great for quickly filtering out Contexts before doing any more expensive work, 
/// and for simple checks where only Yes/No answers make any semblance of sense.
#[derive(Clone)]
pub struct UtilityBinaryCurve;

impl UtilityBinaryCurve {
    fn new() -> Self { Self {} }
}

impl math::curve::Curve<ActionScore> for UtilityBinaryCurve {
    fn domain(&self) -> Interval {
        Interval::UNIT
    }

    fn sample_unchecked(&self, t: f32) -> ActionScore {
        match t >= 1. {
            true => 1.,
            false => 0.,
        }
    }
}

impl UtilityCurve for UtilityBinaryCurve {}

/// A Utility Curve that is effectively another Curve ping-ponged and squished 
/// back to a Unit domain range (i.e. 0.0-1.0 range; ping-ponging yields 0.0-2.0).
/// 
/// This generally produces a 'triangle' or 'bell'-shaped curve centered around 
/// the 0.5 value when wrapping basic Curves like Linear (forms a triangle), 
/// CircularOut (forms a half-circle arc) or SineOut (a single sine wave cycle).
/// 
/// If the base curve peaks at 1.0, its Midpoint equivalent will peak at 0.5 
/// before falling off at the same rate it grew when approaching the midpoint.
/// 
/// Likewise, if the base curve returns 0.0 at t=1.0, its Midpoint version will 
/// decay towards the halfway point before bouncing back at the same rate afterwards.
/// 
/// This can be used to model Considerations where the optimal values are somewhere 
/// in the middle (e.g. a slow AoE Attacker scoring areas by enemy count - too few   
/// targets is not worth the bother, too many and we'll get overrun) by using an  
/// *increasing* Curve function as a base. 
/// 
/// Conversely we can use a *decreasing* base to model scenarios where the optimal 
/// values are on the extremes and we **avoid** the middle. This is a bit more niche, 
/// but may form part of a larger decision pipeline - e.g. an Assassin-type AI that 
/// prefers either finishing off low-health targets, or softening up new ones.
/// 
/// **ADVANCED USAGE**: 
/// 
/// A HalfwayMirrorCurve wraps any UtilityCurve. 
/// A HalfwayMirrorCurve also **IS** a UtilityCurve.
/// 
/// This means you can create a HalfwayMirrorCurve<HalfwayMirrorCurve<T>>. 
/// This doubles the frequency of the 'wave', creating new extrema at 0.25 and 0.75 
/// (and doubling the value of the curve's derivatives inbetween them).
/// 
/// This most likely is an extremely niche use-case - but you do you!
#[derive(Clone)]
pub struct HalfwayMirrorCurve<C: UtilityCurve> {
    wrapped_curve: math::curve::LinearReparamCurve<
        ActionScore, 
        math::curve::PingPongCurve<ActionScore, C>
    >
}

impl<C: UtilityCurve> Curve<ActionScore> for HalfwayMirrorCurve<C> {
    fn domain(&self) -> Interval {
        self.wrapped_curve.domain()
    }

    fn sample_unchecked(&self, t: f32) -> ActionScore {
        self.wrapped_curve.sample_unchecked(t)
    }
}

impl<C: UtilityCurve> HalfwayMirrorCurve<C> {
    /// Explicitly tries to create a HalfwayMirror-ized curve from another curve 
    /// by ping-ponging it and then scaling down the domain by 50% (since 
    /// ping-ponging doubles the domain; we're squishing it back to 0-1 range).
    pub fn try_from_curve(curve: C) -> Result<Self, ()> {
        let wrapped = curve
            .ping_pong()
            .map(|pingponged| 
                pingponged.reparametrize_linear(Interval::UNIT)
            )
        ;

        match wrapped {
            Err(_pperr) => Err(()),
            Ok(reparam) => match reparam {
                Err(_reperr) => Err(()),
                Ok(wrapped_curve) => Ok(Self {
                    wrapped_curve: wrapped_curve
                })
            }
        }
    }
}

/// All Utility Curves should be valid inputs, so we provide From<T> rather than TryFrom<T>. 
/// Unfortunately TryFrom<T> is blanket-implemented for From<T>s, so if for some reason you 
/// REALLY need fallible conversions, use the custom `HalfwayMirrorCurve::try_from_curve(...)` method.
impl<C: UtilityCurve> From<C> for HalfwayMirrorCurve<C> {
    fn from(value: C) -> Self {
        Self::try_from_curve(value)
        .expect("The source Curve's domain is not a unit interval. This should never happen.")
    }
}

impl<C: UtilityCurve> UtilityCurve for HalfwayMirrorCurve<C> {}

/// A curve that adds the samples from two basic curves and renormalizes them by arithmetic mean. 
/// For example, an AverageCurve of Linear and Square Curves at t=0.9 would yield: 
/// 
/// `(0.9 + (0.9^2)) / 2` == `(0.9 + 0.81) / 2` == `0.855`.
/// 
/// This can be used to construct more complex Curves while still maintaining UtilityCurve invariants.
pub struct AverageCurve<A: UtilityCurve, B: UtilityCurve> {
    a: A,
    b: B,
}

impl<A: UtilityCurve, B: UtilityCurve> From<(A, B)> for AverageCurve<A, B> {
    fn from(value: (A, B)) -> Self {
        Self {
            a: value.0,
            b: value.1,
        }
    }
}

impl<A: UtilityCurve, B: UtilityCurve> math::curve::Curve<ActionScore> for AverageCurve<A, B> {
    fn domain(&self) -> Interval {
        Interval::UNIT
    }

    fn sample_unchecked(&self, t: f32) -> ActionScore {
        (self.a.sample_safe(t) + self.b.sample_safe(t)) / 2.
    }
}

impl<A: UtilityCurve, B: UtilityCurve> UtilityCurve for AverageCurve<A, B> {}

/// A transform that adds a constant amount of baseline Utility to the output of the wrapped Curve 
/// and rescales the rest to maintain the max=1.0; i.e.: 
/// 
/// `l(f(x), g) = g + (1.0 - g) * f(x)`.
/// 
/// This creates a floor of minimum Utility (hence 'leak', it always lets a bit of the Action through), 
/// while largely preserving the overall shape of the wrapped Curve (unlike HardLeak, which cuts it up).
/// However, the larger the added floor, the more squished the Curve becomes, causing some loss of detail.
/// 
/// At `g >= 1.`, the curve becomes oversaturated and collapses into always outputting 1.0 for all inputs.
/// 
/// As an analogy, this is equivalent to a downwards Compressor w/ makeup gain in audio processing.
/// 
/// This can be handy for Considerations that should never *eliminate* a candidate Action, but which 
/// still prefer the Context value to be in a certain range; e.g. SoftLeak(AtLeast(1.), 0.5) means 
/// that values above Max get a Utility of 1.0, while those below get a Utility of 0.5.
/// 
/// Note that the constant value in this formula can also be **negative**, which turns this into a 
/// Utility equivalent of an Expander instead. This sharpens continuous nonlinear wrapped Curves,
/// making the low scores lower and high scores higher the lower the constant value is.
/// 
/// **NOTE**: This wrapper should be applied OVER the UtilityCurveSampler, not wrapped by it - i.e. 
/// you SHOULD use SoftLeak<UtilityCurveSampler<C>>, and AVOID UtilityCurveSampler<SoftLeak<C>>. 
/// This is important, as it may cause surprising outputs when the Sampler runs in Inverse mode!
#[derive(Clone)]
pub struct SoftLeak<C: UtilityCurve> {
    curve: C,
    gain: ActionScore,
}

impl<C: UtilityCurve> SoftLeak<C> {
    pub fn new(curve: C, gain: ActionScore) -> Self {
        Self {
            curve: curve,
            gain: gain,
        }
    }

    pub const fn new_const_compressor<const GAIN: u8>(curve: C) -> Self {
        let gain = (GAIN as f32) / (u8::MAX as f32);
        Self {
            curve: curve,
            gain: gain,
        }
    }

    pub const fn new_const_expander<const GAIN: u8>(curve: C) -> Self {
        let gain = (GAIN as f32) / (u8::MAX as f32);
        Self {
            curve: curve,
            gain: -gain,
        }
    }
}

impl<C: UtilityCurve> Curve<ActionScore> for SoftLeak<C> {
    fn domain(&self) -> Interval {
        Interval::UNIT
    }

    fn sample_unchecked(&self, t: f32) -> ActionScore {
        self.gain + (1. - self.gain) * self.curve.sample_unchecked(t)
    }
}

impl<C: UtilityCurve> UtilityCurve for SoftLeak<C> {}

/// A transform that adds a constant amount of baseline Utility to the output of the wrapped Curve 
/// and rescales the rest to maintain the max=1.0; i.e.: 
/// 
/// `l(f(x), g) = (g + f(x)).clamp(0., 1.)`.
/// 
/// This creates a floor of minimum Utility (hence 'leak', it always lets a bit of the Action through), 
/// and shifts the Curve, leaving its derivatives unmolested (unlike SoftLeak). Instead, it flattens 
/// any parts of the shape above the maximum value, which may distort the overall shape at the maxima.
/// 
/// At `g >= 1.`, the curve becomes oversaturated and collapses into always outputting 1.0 for all inputs.
/// 
/// As an analogy, this is equivalent to Distortion (hard-clip overdrive) in audio processing.
/// 
/// This can be handy for Considerations that should never *eliminate* a candidate Action, but which 
/// still prefer the Context value to be in a certain range; e.g. HardLeak(AtLeast(1.), 0.5) means 
/// that values above Max get a Utility of 1.0, while those below get a Utility of 0.5.
/// 
/// Note that the constant value in this formula can also be **negative**, which instead acts as a 
/// simple *subtraction* (saturating at zero Utility) - e.g. HardLeak(AtLeast(1.), -0.5) would instead 
/// mean that values above Max get Utility of 0.5, while everything else gets zero.
/// 
/// **NOTE**: This wrapper should be applied OVER the UtilityCurveSampler, not wrapped by it - i.e. 
/// you SHOULD use HardLeak<UtilityCurveSampler<C>>, and AVOID UtilityCurveSampler<HardLeak<C>>. 
/// This is important, as it may cause surprising outputs when the Sampler runs in Inverse mode!
#[derive(Clone)]
pub struct HardLeak<C: UtilityCurve> {
    curve: C,
    gain: ActionScore,
}

impl<C: UtilityCurve> HardLeak<C> {
    pub fn new(curve: C, gain: ActionScore) -> Self {
        Self {
            curve: curve,
            gain: gain,
        }
    }

    pub const fn new_const_distortion<const GAIN: u8>(curve: C) -> Self {
        let gain = (GAIN as f32) / (u8::MAX as f32);
        Self {
            curve: curve,
            gain: gain,
        }
    }

    pub const fn new_const_subtraction<const GAIN: u8>(curve: C) -> Self {
        let gain = (GAIN as f32) / (u8::MAX as f32);
        Self {
            curve: curve,
            gain: -gain,
        }
    }
}

impl<C: UtilityCurve> Curve<ActionScore> for HardLeak<C> {
    fn domain(&self) -> Interval {
        Interval::UNIT
    }

    fn sample_unchecked(&self, t: f32) -> ActionScore {
        (self.gain + self.curve.sample_unchecked(t)).clamp(
            crate::types::MIN_CONSIDERATION_SCORE, 
            crate::types::MAX_CONSIDERATION_SCORE, 
        )
    }
}

impl<C: UtilityCurve> UtilityCurve for HardLeak<C> {}

// Handy common const-valued curves:
//
/// A curve that always returns zero; can be used to temporarily knock out an Action 
/// for testing/debugging without deleting it from game data outright.
pub const NEVER_CURVE: UtilityConstantCurve = UtilityConstantCurve::new_const(0);
//
/// A curve that always returns full score; mainly useful as a placeholder.
pub const ALWAYS_CURVE: UtilityConstantCurve = UtilityConstantCurve::new_const(255);
//
/// A curve that always returns 0.25; mainly useful as a placeholder.
pub const CONST_QUARTER_CURVE: UtilityConstantCurve = UtilityConstantCurve::new_const(64);
//
/// A curve that always returns 0.50; mainly useful as a placeholder.
pub const CONST_HALF_CURVE: UtilityConstantCurve = UtilityConstantCurve::new_const(128);
//
/// A curve that always returns 0.75; mainly useful as a placeholder.
pub const CONST_THREEQUARTER_CURVE: UtilityConstantCurve = UtilityConstantCurve::new_const(192);

// Borrowing usable Bevy curves for impls.
// This only includes the curves natively outputting in the unit interval.
// We clamp the outputs anyway, but it would be confusing.

// Supported easing curves
impl UtilityCurve for math::curve::CircularInCurve {}
impl UtilityCurve for math::curve::CircularInOutCurve {}
impl UtilityCurve for math::curve::CircularOutCurve {}
impl UtilityCurve for CubicInCurve {}
impl UtilityCurve for math::curve::CubicInOutCurve {}
impl UtilityCurve for math::curve::CubicOutCurve {}
impl UtilityCurve for ExponentialInCurve {}
impl UtilityCurve for math::curve::ExponentialInOutCurve {}
impl UtilityCurve for math::curve::ExponentialOutCurve {}
impl UtilityCurve for LinearCurve {}
impl UtilityCurve for QuadraticInCurve {}
impl UtilityCurve for QuadraticInOutCurve {}
impl UtilityCurve for math::curve::QuadraticOutCurve {}
impl UtilityCurve for math::curve::QuarticInCurve {}
impl UtilityCurve for math::curve::QuarticInOutCurve {}
impl UtilityCurve for math::curve::QuarticOutCurve {}
impl UtilityCurve for math::curve::QuinticInCurve {}
impl UtilityCurve for math::curve::QuinticInOutCurve {}
impl UtilityCurve for math::curve::QuinticOutCurve {}
impl UtilityCurve for math::curve::SineInCurve {}
impl UtilityCurve for math::curve::SineInOutCurve {}
impl UtilityCurve for math::curve::SineOutCurve {}
impl UtilityCurve for math::curve::SmoothStepCurve {}
impl UtilityCurve for math::curve::SmoothStepInCurve {}
impl UtilityCurve for math::curve::SmoothStepOutCurve {}
impl UtilityCurve for math::curve::SmootherStepCurve {}
impl UtilityCurve for math::curve::SmootherStepInCurve {}
impl UtilityCurve for math::curve::SmootherStepOutCurve {}

// A reverse of any valid curve is still a valid curve
impl<U: UtilityCurve> UtilityCurve for math::curve::ReverseCurve<ActionScore, U> {}

/// Specifies the transform used by the UtilityCurveSampler. 
/// - FORWARD => pass-through to `UtilityCurve::sample_safe()`.
/// - INVERSE => `(1.0 - UtilityCurve::sample_safe())` transform.
#[derive(Debug, Clone)]
pub enum CurveSamplerMode {
    FORWARD,
    INVERSE,
}

/// A very common transformation for Utility purposes is inverting a Curve's outputs, 
/// returning `(1. - raw_score)` rather than `raw_score`.
/// 
/// UtilityCurveSampler is a wrapper over a UtilityCurve that captures such transforms.
/// 
/// Note that these samplers themselves fulfil the trait interfaces of Curve and UtilityCurve. 
/// 
/// While it would theoretically be possible to stack them, it would be pretty pointless, 
/// as any pair of CurveSamplerModes either cancel each other out or are pass-through.
/// 
/// This is very similar to Bevy's ReverseCurve<T, C>, but specialized for Utility Curves 
/// in some ways that *might* make it a bit cheaper. Mainly though, const constructors go brr.
#[derive(Debug, Clone)]
pub struct UtilityCurveSampler<U: UtilityCurve> {
    curve: U,
    mode: CurveSamplerMode,
}

impl<U: UtilityCurve> UtilityCurveSampler<U> {
    pub const fn new(curve: U, mode: CurveSamplerMode) -> Self {
        Self {
            curve: curve,
            mode: mode,
        }
    }

    pub const fn new_forward(curve: U) -> Self {
        Self {
            curve: curve,
            mode: CurveSamplerMode::FORWARD,
        }
    }

    pub const fn new_inverse(curve: U) -> Self {
        Self {
            curve: curve,
            mode: CurveSamplerMode::INVERSE,
        }
    }
}

impl<U: UtilityCurve> Curve<ActionScore> for UtilityCurveSampler<U> {
    fn domain(&self) -> Interval {
        Interval::UNIT
    }

    fn sample_unchecked(&self, t: f32) -> ActionScore {
        match self.mode {
            CurveSamplerMode::FORWARD => self.curve.sample_safe(t),
            CurveSamplerMode::INVERSE => 1. - self.curve.sample_safe(t),
        }
    }
}

impl<U: UtilityCurve> UtilityCurve for UtilityCurveSampler<U> {}

// We're wrapping all of these in UtilityCurveSamplers even when not really necessary 
// for the sake of more predictable, uniform typing.
pub const CURVE_CONST_ZERO: UtilityCurveSampler<UtilityConstantCurve> = UtilityCurveSampler::new_forward(UtilityConstantCurve::new_const(0));
pub const CURVE_CONST_MAX: UtilityCurveSampler<UtilityConstantCurve> = UtilityCurveSampler::new_forward(UtilityConstantCurve::new_const(255));
pub const CURVE_CONST_HALF: UtilityCurveSampler<UtilityConstantCurve> = UtilityCurveSampler::new_forward(UtilityConstantCurve::new_const(128));
pub const CURVE_ATLEAST: UtilityCurveSampler<UtilityBinaryCurve> = UtilityCurveSampler::new_forward(UtilityBinaryCurve {});
pub const CURVE_LESSTHAN: UtilityCurveSampler<UtilityBinaryCurve> = UtilityCurveSampler::new_inverse(UtilityBinaryCurve {});
pub const CURVE_LINEAR: UtilityCurveSampler<LinearCurve> = UtilityCurveSampler::new_forward(LinearCurve {});
pub const CURVE_ANTILINEAR: UtilityCurveSampler<LinearCurve> = UtilityCurveSampler::new_inverse(LinearCurve {});
pub const CURVE_SQUARE: UtilityCurveSampler<QuadraticInCurve> = UtilityCurveSampler::new_forward(QuadraticInCurve {});
pub const CURVE_ANTISQUARE: UtilityCurveSampler<QuadraticInCurve> = UtilityCurveSampler::new_inverse(QuadraticInCurve {});
pub const CURVE_EXPONENTIAL: UtilityCurveSampler<ExponentialInCurve> = UtilityCurveSampler::new_forward(ExponentialInCurve {});
pub const CURVE_ANTIEXPONENTIAL: UtilityCurveSampler<ExponentialInCurve> = UtilityCurveSampler::new_inverse(ExponentialInCurve {});

#[derive(Clone)]
pub enum SupportedUtilityCurve {
    /// A Curve that always returns 0.0, no matter the input.
    /// 
    /// **COST:** Const-valued, so basically zero.
    /// 
    /// **USAGE:** Mainly useful for development; a code smell in production logs.
    ConstZero(UtilityCurveSampler<UtilityConstantCurve>),

    /// A Curve that always returns 1.0, no matter the input.
    /// 
    /// **COST:** Const-valued, so basically zero.
    /// 
    /// **USAGE:** Mainly useful for development; a code smell in production logs.
    ConstMax(UtilityCurveSampler<UtilityConstantCurve>),

    /// A Curve that always returns 0.5, no matter the input.
    /// 
    /// **COST:** Const-valued, so basically zero.
    /// 
    /// **USAGE:** Mainly useful for development; a code smell in production logs.
    ConstHalf(UtilityCurveSampler<UtilityConstantCurve>),

    /// A Curve that essentially acts as an if-statement: `t >= 1.0` where `t` is normalized input.
    /// 
    /// In un-normalized terms, checks that the input value is at or above the Max value, 
    /// returning max Utility if it is, zero Utility otherwise.
    /// 
    /// **COST:** Dirt-cheap.
    /// 
    /// **USAGE:** Absurdly cheap to calculate, although not very good at nuance. 
    /// It's either on or off, it cannot distinguish 'good' vs 'great' vs 'tolerable'.
    /// 
    /// Recommended for early filtering - for example, if we have a Heal Action, we can 
    /// filter out any targets that don't have at least 1% HP as too far gone before we 
    /// run any more Considerations on the remaining candidates to save on CPU time.
    AtLeast(UtilityCurveSampler<UtilityBinaryCurve>),

    /// A Curve that essentially acts as an if-statement: `t < 1.0` where `t` is normalized input.
    /// 
    /// In un-normalized terms, checks that the input value is at or above the Max value, 
    /// returning max Utility if it is, zero Utility otherwise.
    /// 
    /// **COST:** Dirt-cheap.
    /// 
    /// **USAGE:** Absurdly cheap to calculate, although not very good at nuance. 
    /// It's either on or off, it cannot distinguish 'good' vs 'great' vs 'tolerable'.
    /// 
    /// Recommended for early filtering - for example, if we have a SprintTo Action, we can 
    /// eliminate any targets that are a thousand miles away by simply capping the Max distance 
    /// between the candidate position and the Pawn before checking anything more expensive.
    LessThan(UtilityCurveSampler<UtilityBinaryCurve>),

    /// A Curve that essentially acts as an if-statement: `t == 1.0` where `t` is normalized input.
    /// 
    /// In un-normalized terms, checks that the input value is exactly at the Max value, 
    /// returning max Utility if it is, zero Utility otherwise.
    /// 
    /// **COST:** Dirt-cheap.
    /// 
    /// **USAGE:** Absurdly cheap to calculate, although not very good at nuance. 
    /// It's either on or off, it cannot distinguish 'good' vs 'great' vs 'tolerable'.
    /// 
    /// Use sparingly; it's included for completeness, but seeing it in production code 
    /// is most likely an AI design smell - your ContextFetchers might need improving instead.
    Equals(UtilityCurveSampler<HalfwayMirrorCurve<UtilityBinaryCurve>>),

    /// A Curve that essentially acts as an if-statement: `t != 1.0` where `t` is normalized input.
    /// 
    /// In un-normalized terms, checks that the input value is exactly at the Max value, 
    /// returning zero Utility if it is, max otherwise.
    /// 
    /// **COST:** Dirt-cheap.
    /// 
    /// **USAGE:** Absurdly cheap to calculate, although not very good at nuance. 
    /// It's either on or off, it cannot distinguish 'good' vs 'great' vs 'tolerable'.
    /// 
    /// Use sparingly; it's included for completeness, but seeing it in production code 
    /// is most likely an AI design smell - your ContextFetchers might need improving instead.
    NotEquals(UtilityCurveSampler<HalfwayMirrorCurve<UtilityBinaryCurve>>),

    /// A monotonically increasing 'high-pass' Curve where t<=min returns 0.0, 
    /// t>=max returns 1.0, and every value in between is LERPed. 
    /// 
    /// The most fundamental 'fuzzy logic' Utility Curve. 
    /// 
    /// **COST:** Very cheap to calculate.
    /// 
    /// **USAGE:** Recommended first option until it becomes clear that you need bigger guns for the job.
    Linear(UtilityCurveSampler<LinearCurve>),

    /// A monotonically *decreasing* 'low-pass' Curve where t<=min returns 1.0, 
    /// t>=max returns 0.0, and every value in between is LERPed. 
    /// 
    /// Linear's Opposite Day evil twin.
    /// 
    /// **COST:** Very cheap to calculate.
    /// 
    /// **USAGE:** Recommended first option until it becomes clear that you need bigger guns for the job.
    AntiLinear(UtilityCurveSampler<LinearCurve>),

    /// A monotonically increasing 'high-pass' Curve where t<=min returns 0.25, 
    /// t>=max returns 1.0, and every value in between is LERPed. 
    /// 
    /// Basically just Linear with a 25% SoftLeak.
    /// 
    /// **COST:** Cheap to calculate.
    /// 
    /// **USAGE:** When you'd use Linear, but the Min/Max values are more guidelines than hard requirements. 
    /// You're still kinda okay running with values outside of that range, but would prefer not to.
    Linear25pSoftLeak(SoftLeak<UtilityCurveSampler<LinearCurve>>),

    /// A monotonically *decreasing* 'low-pass' Curve where t<=min returns 1.0, 
    /// t>=max returns 0.25, and every value in between is LERPed. 
    /// 
    /// Basically just AntiLinear with a 25% SoftLeak.
    /// 
    /// **COST:** Cheap to calculate.
    /// 
    /// **USAGE:** When you'd use AntiLinear, but the Min/Max values are more guidelines than hard requirements.
    /// You're still kinda okay running with values outside of that range, but would prefer not to.
    AntiLinear25pSoftLeak(SoftLeak<UtilityCurveSampler<LinearCurve>>),

    /// A monotonically increasing 'high-pass' Curve similar to Linear, except uses input-squared. 
    /// 
    /// This makes it more 'picky', with Utility falling off faster the further we are from t=max.
    /// 
    /// **COST:** Bit more expensive than Linear, but nothing crazy. 
    /// 
    /// **USAGE:** Recommended whenever you want more focus on better-scoring values than Linear gives you.
    Square(UtilityCurveSampler<QuadraticInCurve>),

    /// A monotonically decreasing 'low-pass' Curve similar to AntiLinear, except uses input-squared. 
    /// 
    /// Prefers lower inputs more strongly the same way its Square twin prefers high ones.
    /// 
    /// **COST:** Bit more expensive than Linear, but nothing crazy. 
    /// 
    /// **USAGE:** Recommended whenever you want more focus on better-scoring values than AntiLinear gives you
    AntiSquare(UtilityCurveSampler<QuadraticInCurve>),

    /// A monotonically increasing 'high-pass' Curve using an exponential function. 
    /// 
    /// The log2 of the score grows linearly from -10 to 0, so the score 
    /// proper is at 1.0 at 1.0 and drops by half with every 10% drop
    /// (0.5 at t=0.9, 0.25 at t=0.8, 0.125 at t=0.7 and so on)
    /// 
    /// This yields a curve with a dramatic but smooth convex dropoff, 
    /// very strongly favoring scores close to the maximum, but with 
    /// some fuzziness left in for compromises.
    /// 
    /// **COST:** Does a bit of floating-point exponential magic; not dirt cheap, but should be fast.
    /// 
    /// **USAGE:** When you really only care about the top 10%-ish of the range but are still willing 
    /// to make compromises; a 'Binary with some tolerance' in a sense.
    ExponentialIn(UtilityCurveSampler<ExponentialInCurve>),
    
    /// A monotonically decreasing 'low-pass' Curve using an exponential function. 
    /// 
    /// The log2 of the score grows linearly from 0 to -10, so the score 
    /// proper is at 1.0 at 0.0 and drops by half with every 10% increase
    /// (0.5 at t=0.1, 0.25 at t=0.2, 0.125 at t=0.3 and so on)
    /// 
    /// This yields a curve with a dramatic but smooth convex dropoff, 
    /// very strongly favoring scores close to the minimum, but with 
    /// some fuzziness left in for compromises.
    /// 
    /// **COST:** Does a bit of floating-point exponential magic; not dirt cheap, but should be fast.
    /// 
    /// **USAGE:** When you really don't want something in the top 10%-ish of the range 
    /// but are still willing to make compromises; an 'AntiBinary with some tolerance' in a sense.
    AntiExponentialIn(UtilityCurveSampler<ExponentialInCurve>),

    /// A NON-monotonic, 'band-pass' Curve peaking at t=0.5 with minima at 0.0 and 1.0. 
    /// 
    /// This Curve is effectively a Linear + AntiLinear curve, glued back-to-back. 
    /// 
    /// This results in a sharp equilateral triangular shape where samples exhibit 
    /// linear growth from 0.0 to 1.0 for the first half of the range, then decay 
    /// back to 0.0 for the second half of the range at the same rate.
    /// 
    /// Another way to look at this is that this Curve's Utility score decays linearly 
    /// in proportion to the (doubled) L1 distance from the middle of the input range.
    /// 
    /// **COST:** Very cheap to calculate.
    /// 
    /// **USAGE:** Whenever you find yourself using a Linear + AntiLinear in sequence 
    /// to try to pick values in a specific range only (not too small, not too big). 
    /// This Curve is exactly equivalent to such a stack, but more efficient to process. 
    Triangle(UtilityCurveSampler<HalfwayMirrorCurve<LinearCurve>>),

    /// A NON-monotonic, 'band-stop' Curve with maxima at 0.0 and 1.0 and a 'through' at 0.5. 
    /// 
    /// This Curve is effectively a AntiLinear + Linear curve, glued back-to-back. 
    /// 
    /// This results in a sharp equilateral triangular shape where samples exhibit 
    /// linear decay from 0.0 to 1.0 for the first half of the range, then grow 
    /// back to 0.0 for the second half of the range at the same rate.
    /// 
    /// Another way to look at this is that this Curve's Utility score increases linearly 
    /// in proportion to the (doubled) L1 distance from the middle of the input range.
    /// 
    /// **COST:** Very cheap to calculate.
    /// 
    /// **USAGE:** When you want a specific value in the middle of the range, 
    /// but are willing to make compromises. 
    /// Use whenever you find yourself using a AntiLinear + Linear in sequence 
    /// to try to pick values outside of a specific range only (either big or small, but not mid). 
    /// This Curve is exactly equivalent to such a stack, but more efficient to process. 
    AntiTriangle(UtilityCurveSampler<HalfwayMirrorCurve<LinearCurve>>),

    /// A NON-monotonic, 'band-pass' Curve peaking at t=0.5 with minima at 0.0 and 1.0. 
    /// 
    /// This Curve is effectively a Square + AntiSquare curve, glued back-to-back. 
    /// 
    /// This results in a bell curve-like 'mound' shape with a softly concave top 
    /// in the middle and softly convex 'foot' near the edges of the range. 
    /// 
    /// Another way to look at this is that this Curve's Utility score decays linearly 
    /// in proportion to the (doubled) L2 distance from the middle of the input range.
    /// 
    /// This means that this curve tolerates small deviations from the midrange value, 
    /// but penalizes large ones more strongly.
    /// 
    /// **COST:** Low-moderate, as long as you don't use it for everything it should be fine.
    /// 
    /// **USAGE:** When you want values 'around' the middle, you don't care too strongly 
    /// about hitting the middle precisely - but the middle is still optimal in some way.
    QuadraticQuasiGauss(UtilityCurveSampler<HalfwayMirrorCurve<QuadraticInOutCurve>>),

    /// A NON-monotonic, 'band-stop' Curve with maxima at 0.0 and 1.0 and a 'through' at 0.5. 
    /// 
    /// This Curve is effectively a AntiSquare + Square curve, glued back-to-back. 
    /// 
    /// This results in a 'half-pipe' shape with softly concave edges
    /// and a smoothly rounded-off through in the middle.
    /// 
    /// Another way to look at this is that this Curve's Utility score grows linearly 
    /// in proportion to the (doubled) L2 distance from the middle of the input range.
    /// 
    /// This means that this curve strongly prefers extreme values, rejecting candidates 
    /// increasingly more forcefully as they get closer to the middle of the range.
    /// 
    /// **COST:** Low-moderate, as long as you don't use it for everything it should be fine.
    /// 
    /// **USAGE:** When you need to carve out a range of values as undesirable 
    /// without using a hard cutoff with binary filters. 
    /// For example, an AI with both a melee attack and a ranged attack with minimum range 
    /// could use this to filter targets - if the distances are slightly less than ideal, 
    /// the Pawn could move slightly to get a good shot, so we don't want to
    /// reject these potential targets outright.
    AntiQuadraticQuasiGauss(UtilityCurveSampler<HalfwayMirrorCurve<QuadraticInOutCurve>>),

    /// A user-defined Curve type registered in the UtilityCurveRegistry. 
    /// 
    /// Due to the Arc<dyn T> overhead, these will be less performant than 
    /// the corresponding built-in Curves above, even if they are implemented 
    /// in the exact same way.
    Custom(Arc<dyn UtilityCurve>)
}

impl core::fmt::Debug for SupportedUtilityCurve {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ConstZero(_) => f.debug_tuple("ConstZero").finish(),
            Self::ConstMax(_) => f.debug_tuple("ConstMax").finish(),
            Self::ConstHalf(_) => f.debug_tuple("ConstHalf").finish(),
            Self::AtLeast(_) => f.debug_tuple("AtLeast").finish(),
            Self::LessThan(_) => f.debug_tuple("LessThan").finish(),
            Self::Equals(_) => f.debug_tuple("Equals").finish(),
            Self::NotEquals(_) => f.debug_tuple("NotEquals").finish(),
            Self::Linear(_) => f.debug_tuple("Linear").finish(),
            Self::AntiLinear(_) => f.debug_tuple("AntiLinear").finish(),
            Self::Linear25pSoftLeak(_) => f.debug_tuple("Linear25%SoftLeak").finish(),
            Self::AntiLinear25pSoftLeak(_) => f.debug_tuple("AntiLinear25%SoftLeak").finish(),
            Self::Square(_) => f.debug_tuple("Square").finish(),
            Self::AntiSquare(_) => f.debug_tuple("AntiSquare").finish(),
            Self::ExponentialIn(_) => f.debug_tuple("ExponentialIn").finish(),
            Self::AntiExponentialIn(_) => f.debug_tuple("AntiExponentialIn").finish(),
            Self::Triangle(_) => f.debug_tuple("Triangle").finish(),
            Self::AntiTriangle(_) => f.debug_tuple("AntiTriangle").finish(),
            Self::QuadraticQuasiGauss(_) => f.debug_tuple("QuadraticQuasiGauss").finish(),
            Self::AntiQuadraticQuasiGauss(_) => f.debug_tuple("AntiQuadraticQuasiGauss").finish(),
            Self::Custom(_) => f.debug_tuple("Custom").finish(),
        }
    }
}

impl Curve<ActionScore> for SupportedUtilityCurve {
    fn domain(&self) -> Interval {
        Interval::UNIT
    }

    fn sample_unchecked(&self, t: f32) -> ActionScore {
        match self {
            Self::ConstZero(c) => c.sample_unchecked(t),
            Self::ConstMax(c) => c.sample_unchecked(t),
            Self::ConstHalf(c) => c.sample_unchecked(t),
            Self::AtLeast(c) => c.sample_unchecked(t),
            Self::LessThan(c) => c.sample_unchecked(t),
            Self::Equals(c) => c.sample_unchecked(t),
            Self::NotEquals(c) => c.sample_unchecked(t),
            Self::Linear(c) => c.sample_unchecked(t),
            Self::AntiLinear(c) => c.sample_unchecked(t),
            Self::Linear25pSoftLeak(c) => c.sample_unchecked(t),
            Self::AntiLinear25pSoftLeak(c) => c.sample_unchecked(t),
            Self::Square(c) => c.sample_unchecked(t),
            Self::AntiSquare(c) => c.sample_unchecked(t),
            Self::ExponentialIn(c) => c.sample_unchecked(t),
            Self::AntiExponentialIn(c) => c.sample_unchecked(t),
            Self::Triangle(c) => c.sample_unchecked(t),
            Self::AntiTriangle(c) => c.sample_unchecked(t),
            Self::QuadraticQuasiGauss(c) => c.sample_unchecked(t),
            Self::AntiQuadraticQuasiGauss(c) => c.sample_unchecked(t),
            Self::Custom(arc) => arc.sample_safe(t),
        }
    }
}

impl UtilityCurve for SupportedUtilityCurve {}


impl TryFrom<&str> for SupportedUtilityCurve {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        resolve_curve_from_name(value).ok_or(())
    }
}

impl TryFrom<&String> for SupportedUtilityCurve {
    type Error = ();

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        resolve_curve_from_name(value.as_str()).ok_or(())
    }
}

/// Retrieves a Utility curve based on a string(-ish) key.
/// 
/// This will only work for curves included with the library! 
/// 
/// If you want to use 
pub fn resolve_curve_from_name<S: core::borrow::Borrow<str>>(curve_name: S) -> Option<SupportedUtilityCurve> {
    match curve_name.borrow() {
        "ConstZero" => Some(SupportedUtilityCurve::ConstZero(CURVE_CONST_ZERO)),
        "ConstMax" => Some(SupportedUtilityCurve::ConstMax(CURVE_CONST_MAX)),
        "ConstHalf" => Some(SupportedUtilityCurve::ConstHalf(CURVE_CONST_HALF)),
        "AtLeast" => Some(SupportedUtilityCurve::AtLeast(CURVE_ATLEAST)),
        "LessThan" => Some(SupportedUtilityCurve::AtLeast(CURVE_LESSTHAN)),
        "Equals" => Some(SupportedUtilityCurve::Equals(
            UtilityCurveSampler::new_forward(UtilityBinaryCurve::new().halfway_mirror())
        )),
        "NotEquals" => Some(SupportedUtilityCurve::Equals(
            UtilityCurveSampler::new_inverse(UtilityBinaryCurve::new().halfway_mirror())
        )),
        "Linear" => Some(SupportedUtilityCurve::Linear(CURVE_LINEAR)),
        "AntiLinear" => Some(SupportedUtilityCurve::AntiLinear(CURVE_ANTILINEAR)),
        "Linear25%SoftLeak" => Some(SupportedUtilityCurve::Linear25pSoftLeak(
            UtilityCurveSampler::new_forward(LinearCurve {}).soft_leak(0.25)
        )),
        "AntiLinear25%SoftLeak" => Some(SupportedUtilityCurve::AntiLinear25pSoftLeak(
            UtilityCurveSampler::new_inverse(LinearCurve {}).soft_leak(0.25)
        )),
        "Square" => Some(SupportedUtilityCurve::Square(CURVE_SQUARE)),
        "AntiSquare" => Some(SupportedUtilityCurve::AntiSquare(CURVE_ANTISQUARE)),
        "ExponentialIn" => Some(SupportedUtilityCurve::ExponentialIn(CURVE_EXPONENTIAL)),
        "AntiExponentialIn" => Some(SupportedUtilityCurve::AntiExponentialIn(CURVE_ANTIEXPONENTIAL)),
        "Triangle" => Some(SupportedUtilityCurve::Triangle(
            UtilityCurveSampler::new_forward((LinearCurve {}).halfway_mirror())
        )),
        "AntiTriangle" => Some(SupportedUtilityCurve::AntiTriangle(
            UtilityCurveSampler::new_inverse((LinearCurve {}).halfway_mirror())
        )),
        "QuadGauss" => Some(SupportedUtilityCurve::QuadraticQuasiGauss(
            UtilityCurveSampler::new_forward((QuadraticInOutCurve {}).halfway_mirror())
        )),
        "AntiQuadGauss" => Some(SupportedUtilityCurve::QuadraticQuasiGauss(
            UtilityCurveSampler::new_inverse((QuadraticInOutCurve {}).halfway_mirror())
        )),
        _ => None,
    }
}

/// A map that lets us request Utility Curves by a string key and register new entries for custom Curves. 
#[derive(bevy::prelude::Resource, Clone, Default)]
pub struct UtilityCurveRegistry {
    mapping: CraniumKvMap<String, SupportedUtilityCurve>
}

impl UtilityCurveRegistry {
    pub fn get_curve_by_name<S: core::borrow::Borrow<str>>(&self, name: S) -> Option<SupportedUtilityCurve> {
        let static_resolve = resolve_curve_from_name(name.borrow());

        match static_resolve {
            Some(static_curve) => Some(static_curve),
            None => self.mapping.get(name.borrow()).cloned()
        }
    }

    pub fn register_curve<C: UtilityCurve + 'static>(
        &mut self, 
        curve: C, 
        name: String
    ) -> Result<SupportedUtilityCurve, ()> {
        let is_static = resolve_curve_from_name(name.as_str());
        match is_static {
            Some(_) => Err(()),
            None => {
                let wrapper = SupportedUtilityCurve::Custom(Arc::new(curve));
                self.mapping.insert(name, wrapper.clone());
                Ok(wrapper)
            }
        }
    }
}


/// Something that allows us to register a UtilityCurve to the World. 
/// 
/// Note that for convenience, the first registration attempt 
/// will initialize *an empty registry* if one does not exist yet, so
/// you don't need to use `app.initialize_resource::<UtilityCurveRegistry>()` 
/// unless you want to be explicit about it.
pub trait AcceptsCurveRegistrations {
    fn register_utility_curve<
        U: UtilityCurve + 'static,
        IS: Into<String>
    >(
        &mut self, 
        curve: U, 
        key: IS,
    ) -> &mut Self;
}

impl AcceptsCurveRegistrations for bevy::prelude::World {
    fn register_utility_curve<
        U: UtilityCurve + 'static,
        IS: Into<String>
    >(
        &mut self, 
        curve: U, 
        key: IS,
    ) -> &mut Self {
        let mut registry = self.get_resource_or_init::<UtilityCurveRegistry>();
        let curve_key = crate::types::UtilityCurveKey::from(key.into());
        
        let old = registry.mapping.insert(
            curve_key.to_owned(), 
            SupportedUtilityCurve::Custom(
                Arc::new(curve)
            )
        );

        match old {
            None => {},
            Some(_) => {
                #[cfg(feature = "logging")]
                bevy::log::warn!(
                    "Detected a key collision for key {:?}. Ejecting previous registration...",
                    curve_key
                );
            } 
        };

        self
    }
}

impl AcceptsCurveRegistrations for bevy::prelude::App {
    fn register_utility_curve<
        U: UtilityCurve + 'static,
        IS: Into<String>
    >(
        &mut self, 
        curve: U, 
        key: IS,
    ) -> &mut Self {
        self.world_mut().register_utility_curve(curve, key);
        self
    }
}

