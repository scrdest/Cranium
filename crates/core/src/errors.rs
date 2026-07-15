/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! Errors and error-handling.

use bevy::ecs::resource::Resource;
use bevy::platform::prelude::String;
use bevy::platform::sync::Arc;

#[derive(Debug)]
pub enum DynResolutionError {
    UnexpectedType(String),
    NotInRegistry(String)
}

pub trait CurveResolverFn: Send + Sync + Fn(&String) -> crate::curves::SupportedUtilityCurve {}
impl<F: Send + Sync + Fn(&String) -> crate::curves::SupportedUtilityCurve> CurveResolverFn for F {}

/// A config value indicating how the library code should handle Curve keys that 
/// do not correspond to any known value (dynamically registered or hardcoded). 
/// 
/// By default the AI code will panic to avoid either running Actions in unexpected 
/// and potentially harmful ways or silently skipping bad inputs, but users may 
/// opt-in into alternative behaviors (skip/default) at their own responsibility.
#[derive(Default, Clone)]
pub enum NoCurveMatchStrategy {
    #[default]
    SkipActionWithLog,
    SkipConsiderationWithLog,
    DefaultCurveWithLog(Arc<dyn CurveResolverFn>),
    DefaultCurveWithoutLog(Arc<dyn CurveResolverFn>),
    Panic,
}

impl NoCurveMatchStrategy {
    pub const fn panic() -> Self {
        Self::Panic
    }

    pub const fn skip_consideration() -> Self {
        Self::SkipConsiderationWithLog
    }

    pub const fn skip_action() -> Self {
        Self::SkipActionWithLog
    }

    pub fn log_and_default_to<F: Send + Sync + Fn(&String) -> crate::curves::SupportedUtilityCurve + 'static>(
        curve_fn: F
    ) -> Self {
        Self::DefaultCurveWithLog(Arc::new(curve_fn))
    }

    pub fn quietly_default_to<F: Send + Sync + Fn(&String) -> crate::curves::SupportedUtilityCurve + 'static>(
        curve_fn: F
    ) -> Self {
        Self::DefaultCurveWithoutLog(Arc::new(curve_fn))
    }
}

impl core::fmt::Debug for NoCurveMatchStrategy {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Panic => write!(f, "Panic"),
            Self::SkipConsiderationWithLog => write!(f, "SkipConsiderationWithLog"),
            Self::SkipActionWithLog => write!(f, "SkipActionWithLog"),
            Self::DefaultCurveWithLog(_) => write!(f, "DefaultCurveWithLog"),
            Self::DefaultCurveWithoutLog(_) => write!(f, "DefaultCurveWithoutLog"),
        }
    }
}

/// A Resource that represents app-wide configuration for how to handle bad Curve keys. 
/// 
#[derive(Resource, Default)]
pub struct NoCurveMatchStrategyConfig(pub NoCurveMatchStrategy);

impl NoCurveMatchStrategyConfig {
    pub fn get_current_value(&self) -> &NoCurveMatchStrategy {
        return &self.0
    }

    /// Sets the handler to one of the supported strategies (panic, skip, default, etc.).
    pub fn set(&mut self, strategy: NoCurveMatchStrategy) -> &mut Self {
        self.0 = strategy;
        self
    }

    /// Configures the app to panic if a Curve key cannot be resolved to a Curve.
    /// 
    /// This is the default behavior, so this method is only useful if something 
    /// else has already modified the default settings.
    pub fn set_panic(&mut self) -> &mut Self {
        self.set(NoCurveMatchStrategy::panic())
    }

    /// Configures the app to ignore the Consideration and log a warning   
    /// if its associated Curve key cannot be resolved to a Curve.
    /// 
    /// **WARNING**: This is generally **NOT** recommended as it is roughly 
    /// equivalent to returning max score on error, so you may run actions 
    /// that should not be valid and processing Considerations that should 
    /// have been optimized away, so it may be a footgun - but you do you.
    pub fn set_skip_consideration(&mut self) -> &mut Self {
        self.set(NoCurveMatchStrategy::skip_consideration())
    }

    /// Configures the app to discard the whole Action and log a warning   
    /// if any of its Considerations' associated Curve keys cannot be  
    /// resolved to a Curve.
    /// 
    /// This means any buggy Actions effectively get disabled; this means 
    /// the application can keep on truckin' in case of designer errors, 
    /// but the AIs may be missing some capabilities. 
    /// 
    /// However, this may be desirable if you have multiple versions of 
    /// an ActionTemplate, each compatible with a different version of your 
    /// app/modding API/whatever other integration things you are exposing.
    pub fn set_skip_action(&mut self) -> &mut Self {
        self.set(NoCurveMatchStrategy::skip_action())
    }

    /// Configures the app to log a warning if a Curve key cannot be 
    /// resolves to a Curve and select a fallback Curve using the 
    /// provided (`'static`!) mapping function instead.
    /// 
    /// This allows for graceful recovery in case of AI designer error, 
    /// but puts the responsibility on the user to specify good fallbacks. 
    /// 
    /// Broadly speaking, fallbacks should try to match the expected Curve first on 
    /// order relation class (increasing/decreasing/peaking, if peaking - how many peaks), 
    /// then secondarily on 'falloff sharpness similarity' (we could try to define it with derivatives, 
    /// but you probably get the idea informally - e.g. `MoreThan` > `Exponential` > `Square` > `Linear`). 
    /// 
    /// It does not need to be perfect, but the closer the match, the better your chances 
    /// are that things will still work reasonably well using the fallback.
    /// 
    /// If you are happy handling the resolution, this is probably the most robust strategy, 
    /// giving you both a crash-free experience and a log warning about the fallback used.
    pub fn set_log_and_use_default<F: CurveResolverFn + 'static>(
        &mut self, 
        curve_resolver: F
    ) -> &mut Self {
        self.set(NoCurveMatchStrategy::log_and_default_to(curve_resolver))
    }

    /// Configures the app to select a fallback Curve using the 
    /// provided (`'static`!) mapping function if a Curve key cannot  
    /// be resolves to a Curve without logging a warning.
    /// 
    /// This allows for graceful recovery in case of AI designer error, 
    /// but puts the responsibility on the user to specify good fallbacks.
    /// 
    /// Broadly speaking, fallbacks should try to match the expected Curve first on 
    /// order relation class (increasing/decreasing/peaking, if peaking - how many peaks), 
    /// then secondarily on 'falloff sharpness' (we could try to define it with derivatives, 
    /// but you probably get the idea - e.g. Exponential is sharper than Square than Linear). 
    /// 
    /// It does not need to be perfect, but the closer the match, the better your chances 
    /// are that things will still work reasonably well using the fallback.
    /// 
    /// As you might guess, this is effectively a quiet variant of DefaultCurveWithLog. 
    /// 
    /// The primary use-case for this would be if you are using a lot of custom Curves 
    /// and are very confident in your fallback resolution doing a good job and you want 
    /// to reduce warning-spam without necessarily filtering out the warnings from the library altogether.
    pub fn set_silently_use_default<F: CurveResolverFn + 'static>(
        &mut self, 
        curve_resolver: F
    ) -> &mut Self {
        self.set(NoCurveMatchStrategy::quietly_default_to(curve_resolver))
    }
}
