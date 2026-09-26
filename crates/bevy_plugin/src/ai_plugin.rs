/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

use cranium_core::bevy;
use cranium_core::bevy::prelude::*;
use cranium_core::actions;
use cranium_core::action_runtime;
use cranium_core::action_state;
use cranium_core::considerations;
use cranium_core::context_fetchers;
use cranium_core::decision_loop;
use cranium_core::events::AiActionPickedMessage;
use cranium_core::schedule;
use cranium_core::smart_object;

#[cfg(feature = "include_actionset_loader")]
use cranium_actionset_loader::ActionSetAssetPlugin;

#[cfg(feature = "ai_server")]
use cranium_core::bevy::remote::RemotePlugin;

pub enum CraniumPluginError {
    // NoDispatchBackends(Option<&'static str>),
    MutuallyExclusiveConfigOptions(Option<&'static str>)
}

#[derive(Default, Debug)]
pub struct CraniumPluginBuilder {
    include_actionset_loader: bool,
    include_ai_server: bool,
    include_simple_observer_dispatch: bool,
    include_simple_message_dispatch: bool,
    include_actiontracker_dispatch: bool,
}

impl CraniumPluginBuilder {
    pub fn build(&self) -> Result<CraniumPlugin, CraniumPluginError> {
        // tally up enabled dispatch backends so we can alert if > 1
        let dispatch_methods_used = [
            self.include_simple_observer_dispatch,
            self.include_simple_message_dispatch,
            self.include_actiontracker_dispatch,
        ]
        .into_iter()
        .filter(|p| *p)
        .count()
        ;

        if dispatch_methods_used < 1 {
            bevy::log::warn!(
                "No dispatch backends enabled for CraniumPlugin! 
                This is fine if you are using your own CUSTOM dispatch, but if not, 
                this means Cranium will NEVER actually invoke your callbacks, so it's 
                probably just wasting your CPU cycles for nothing."
            )
        }

        if dispatch_methods_used > 1 {
            return Err(CraniumPluginError::MutuallyExclusiveConfigOptions(Some(
                "Multiple dispatch backends enabled simultaneously! 
                Using multiple at the same time is currently unsupported as it would result 
                in double-triggering each Action. Please pick one and disable the others."
            )))
        }

        Ok(CraniumPlugin { 
            include_actionset_loader: self.include_actionset_loader, 
            include_ai_server: self.include_ai_server,
            include_simple_observer_dispatch: self.include_simple_observer_dispatch,
            include_simple_message_dispatch: self.include_simple_message_dispatch,
            include_actiontracker_dispatch: self.include_actiontracker_dispatch,
        })
    }

    pub fn set_include_actionset_loader(&mut self, val: bool) -> &mut Self {
        self.include_actionset_loader = val;
        self
    }

    pub fn set_include_ai_server(&mut self, val: bool) -> &mut Self {
        self.include_ai_server = val;
        self
    }

    pub fn set_include_simple_observer_dispatch(&mut self, val: bool) -> &mut Self {
        self.include_simple_observer_dispatch = val;
        self
    }

    pub fn set_include_simple_message_dispatch(&mut self, val: bool) -> &mut Self {
        self.include_simple_message_dispatch = val;
        self
    }

    pub fn set_include_actiontracker_dispatch(&mut self, val: bool) -> &mut Self {
        self.include_actiontracker_dispatch = val;
        self
    }
}

pub struct CraniumPlugin {
    /// Whether to add an assetloader for ActionSets. 
    /// The include_actionset_loader feature is required for this to work; defaults to True if it is enabled
    /// (otherwise, why did you bother adding the extra feature?)
    include_actionset_loader: bool,
    
    /// Whether to add an AI Server for running Cranium as a DLL. 
    /// The ai_server feature is required for this to work; defaults to True if it is enabled
    /// (otherwise, why did you bother adding the extra feature?)
    include_ai_server: bool,

    /// Whether to use the simplistic user-code dispatch based on Observers. 
    /// 
    /// This is a pretty limited method - it does not track the Action execution at all - but 
    /// you may find it handy if your game already has its own machinery for tracking Action 
    /// execution, and all you want out of Cranium is to shut up and run a simple callback.
    /// 
    /// Note that this and other `include_*_dispatch` options are currently mutually exclusive; 
    /// the paths ultimately emit the SAME dispatch Messages, so if you enabled both, you'd 
    /// be double-booking your callbacks on the first tick.
    include_simple_observer_dispatch: bool,

    /// Whether to use the simplistic user-code dispatch based on Messages. 
    /// 
    /// This is a pretty limited method - it does not track the Action execution at all - but 
    /// you may find it handy if your game already has its own machinery for tracking Action 
    /// execution, and all you want out of Cranium is to shut up and run a simple callback.
    /// 
    /// Note that this and other `include_*_dispatch` options are currently mutually exclusive; 
    /// the paths ultimately emit the SAME dispatch Messages, so if you enabled both, you'd 
    /// be double-booking your callbacks on the first tick.
    include_simple_message_dispatch: bool,

    /// Whether to use the full ActionTracker-based state machine for executing Actions.
    /// 
    /// This is a more complex and more powerful backend for running your AI Actions, 
    /// most notably capable of expressing stuff that can happen over multiple ticks,
    /// but it comes at the cost of more processing overhead, and it may clash or 
    /// duplicate your preexisting solution for tracking Action execution.
    /// 
    /// *This is currently the default backend.*
    /// 
    /// Note that this and `include_simple_observer_dispatch` are currently mutually exclusive; 
    /// the two paths ultimately emit the SAME dispatch Messages, so if you enabled both, you'd 
    /// be double-booking your callbacks on the first tick.
    include_actiontracker_dispatch: bool,
}

impl CraniumPlugin {
    pub fn builder() -> CraniumPluginBuilder {
        CraniumPluginBuilder::default()
    }

    /// Creates a new CraniumPluginBuilder with config copied from the current plugin, 
    /// so that you can tweak the values and produce a new instance of this plugin (or just clone it).
    pub fn copy_to_builder(&self) -> CraniumPluginBuilder {
        CraniumPluginBuilder { 
            include_actionset_loader: self.include_actionset_loader, 
            include_ai_server: self.include_ai_server,
            include_simple_observer_dispatch: self.include_simple_observer_dispatch,
            include_simple_message_dispatch: self.include_simple_message_dispatch,
            include_actiontracker_dispatch: self.include_actiontracker_dispatch,
        }
    }
}

impl Default for CraniumPlugin {
    fn default() -> Self {
        let include_actionset_loader = cfg!(feature = "include_actionset_loader");
        let include_ai_server= cfg!(feature = "ai_server");
        let include_simple_observer_dispatch = false;
        let include_simple_message_dispatch = false;
        let include_actiontracker_dispatch = true;

        Self {
            include_actionset_loader,
            include_ai_server,
            include_simple_observer_dispatch, 
            include_simple_message_dispatch, 
            include_actiontracker_dispatch,
        }
    }
}

impl TryFrom<CraniumPluginBuilder> for CraniumPlugin {
    type Error = CraniumPluginError;

    fn try_from(value: CraniumPluginBuilder) -> Result<Self, Self::Error> {
        value.build()
    }
}

impl Plugin for CraniumPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "include_actionset_loader")]
        if self.include_actionset_loader {
            app
            .add_plugins((
                ActionSetAssetPlugin, 
            ));
        }

        #[cfg(feature = "ai_server")]
        if self.include_ai_server {
            app
            .add_plugins((
                RemotePlugin, 
            ));
        }

        app
        .add_plugins((
            actions::ActionHandlerPlugin,
            action_state::ActionStateUpdatesPlugin,
            context_fetchers::ContextFetcherPlugin, 
            considerations::ConsiderationPlugin,
            schedule::CraniumScheduleStagesPlugin,
        ))

        .init_resource::<action_runtime::UserDefaultActionTrackerSpawnConfig>()
        .init_resource::<smart_object::ActionSetStore>()
        .init_resource::<decision_loop::DecisionEngineConfig>()

        .add_message::<AiActionPickedMessage>()
        .add_message::<cranium_core::events::AiActionDispatchToUserCode>()

        .add_observer(decision_loop::prepare_ai)
        .add_observer(decision_loop::decision_engine)
        ;

        if self.include_actiontracker_dispatch {
            app
            .add_systems(
                FixedUpdate, 
                (
                    decision_loop::handle_dispatch_to_user_actions,
                )
                .in_set(schedule::CraniumSet::Dispatch)
            )
            .add_systems(
                FixedUpdate, 
                (
                    action_runtime::actiontracker_done_cleanup_system,
                )
                .in_set(schedule::CraniumSet::Maintain)
            )
            .add_observer(action_runtime::create_tracker_for_picked_action)
            .add_observer(action_runtime::actiontracker_triggered_spawner)
            .add_observer(action_runtime::actiontracker_triggered_despawner)
            ;
        }

        if self.include_simple_observer_dispatch {
            app
            .add_systems(
                FixedUpdate, 
                (
                    decision_loop::handle_dispatch_to_user_actions,
                )
                .chain()
                .in_set(schedule::CraniumSet::Dispatch)
            )
            .add_observer(decision_loop::trigger_dispatch_to_user_actions_eventbased)
            ;
        }

        if self.include_simple_message_dispatch {
            app
            .add_systems(
                FixedUpdate, 
                (
                    decision_loop::trigger_dispatch_to_user_actions_messagebased,
                    decision_loop::handle_dispatch_to_user_actions,
                )
                .chain()
                .in_set(schedule::CraniumSet::Dispatch)
            )
            ;
        }
    }
}
