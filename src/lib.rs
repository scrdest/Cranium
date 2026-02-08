/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

#![doc = include_str!("../README.md")]
#![no_std]

pub use cranium_core::*;

pub mod prelude {
    pub use cranium_core::*;
    pub use cranium_core::types::*;
    pub use cranium_core::actions::AcceptsActionHandlerRegistrations;
    pub use cranium_core::actions::ActionPickCallback;
    pub use cranium_core::actions::ActionHandlerInputs;
    pub use cranium_core::action_runtime::TickBasedActionTrackerPlugin;
    pub use cranium_core::action_runtime::UserDefaultActionTrackerSpawnConfig;
    pub use cranium_core::ai::AIController;
    pub use cranium_core::considerations::AcceptsConsiderationRegistrations;
    pub use cranium_core::context_fetchers::AcceptsContextFetcherRegistrations;
    pub use cranium_core::curves::AcceptsCurveRegistrations;
    pub use cranium_core::events::AiDecisionRequested;
    pub use cranium_core::pawn::Pawn;

    #[cfg(any(feature = "bevy_plugin", feature = "testing"))]
    pub use cranium_bevy_plugin::CraniumPlugin;

    #[cfg(any(feature = "cranium-test-plugin", feature = "testing"))]
    pub use cranium_test_plugin::CraniumTestPlugin;

    #[cfg(feature = "actionset_loader")]
    pub use cranium_actionset_loader;

    #[cfg(any(feature = "ai_server", feature = "testing"))]
    pub use cranium_ffi;

    #[cfg(any(feature = "ai_server", feature = "testing"))]
    pub use cranium_api;
}
