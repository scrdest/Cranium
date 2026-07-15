/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

use bevy::{platform, prelude::*};

// These two imports should keep you covered for day-to-day usage.
// The CraniumPlugin is in the Prelude with the right features enabled, but it doesn't work here fsr. 
use cranium::prelude::*;
use cranium_bevy_plugin::CraniumPlugin;

// These imports are needed for setting up stuff behind the scenes. 
// In a normal setup, you usually shouldn't need to import these.
use cranium::action_runtime::{ActionTrackerState};
use cranium::actionset::ActionSet;
use cranium::considerations::{ConsiderationData};
use cranium::curves::{LinearCurve, UtilityCurveExt};
use cranium::smart_object::{ActionSetStore};
use cranium_test_plugin::CraniumTestPlugin;

const EXAMPLE_CONTEXT_FETCHER_NAME: &str = "e2e::ExampleCF";

#[derive(Debug, EntityEvent)]
struct ExampleActionEvent{
    entity: AiEntity, 
    _pawn: PawnEntityRef, 
    ctx: ActionContextRef,
}

impl ExampleActionEvent {
    fn from_context_ref(
        context: ActionContextRef, 
        ai: AiEntity, 
        pawn: PawnEntityRef,
    ) -> Self {
        Self {
            entity: ai,
            _pawn: pawn,
            ctx: context,
        }
    }
}

fn example_action(
    trigger: On<ExampleActionEvent>, 
    associated_ai_qry: Query<NameOrEntity, With<AIController>>,
    context_data_qry: Query<&ExampleStateMapContextComponent>,
    mut tracker_state_qry: Query<&mut ActionTrackerState>,
    mut state_writer: MessageWriter<crate::action_state::AiActionStateChangeRequest>,
) {
    let event = trigger.event();
    bevy::log::debug!("example_action: Running, trigger: {:?}", event);

    let ai_entity = event.entity;

    let maybe_tracker_state = tracker_state_qry.get_mut(ai_entity);
    
    let maybe_ai_owner = associated_ai_qry
        .get(ai_entity)
        .ok()
        .map(|bundle| bundle)
    ;

    let ai_owner = maybe_ai_owner
        .map(|own| format!("{:}", own))
        .unwrap_or("<none>".to_string())
    ;

    let context_data = context_data_qry
        .get(event.ctx)
        .map_err(|err| {
            bevy::log::error!(
                "example_action for AI {:?} - Received an invalid Context {:?} ({:?})!", 
                ai_owner, event.ctx, err,
            );
            err
        })
        .expect("Invalid context")
    ;

    let state_mapping = &context_data.statemap;

    let curr_state = match maybe_tracker_state.as_ref() {
        Ok(state) => state.0.clone(),
        Err(err) => {
            bevy::log::info!("Could not find a State for AI {:?} - {:?}", ai_entity, err);
            ActionState::Ready
        },
    };

    bevy::log::info!("example_action for AI {:?} - Current state is {:?}", ai_owner, curr_state);

    let new = match state_mapping.get(&curr_state) {
        None => {
            bevy::log::error!(
                "example_action for AI {:?} - could not find a mapped target state for current state {:?}, aborting!", 
                ai_owner, curr_state
            );
            return;
        },
        Some(new_state) => new_state
    };

    bevy::log::info!("example_action for AI {:?}: Requesting state change to {:?}", ai_owner, new);

    state_writer.write(crate::action_state::AiActionStateChangeRequest {
        entity: event.entity,
        action: "example_action".to_string(),
        to_state: *new,
    });

}


fn action_state_change_observer(
    trigger: On<crate::action_state::AiActionStateChange>,
) {
    let event = trigger.event();
    bevy::log::info!("{:?} for AI {:?}: New state is {:?}", event.action, event.entity, event.to_state);
}


/// A simple 2d Position component for demo purposes only.
#[derive(Component, Clone, Copy, Debug, Default)]
struct Position2d(Vec2);

impl Position2d {
    fn euclid_distance(&self, other: &Self) -> f32 {
        self.0.distance(other.0)
    }
}

/// A Component storing a simple FSM-like state transition mapping. 
/// 
/// Only used for demonstration purposes; we'll add an example ContextFetcher 
/// that returns Entities with this Component and use the Component in a sample 
/// Action which will use the mapping to figure out its next State by lookup.
#[derive(Component, Default, Debug, Clone)]
struct ExampleStateMapContextComponent {
    statemap: CraniumKvMap<ActionState, ActionState>
}

#[derive(Component, Default, Debug, Clone)]
pub struct DumbMarker;

/// A ContextFetcher that returns Entities with an ExampleStateMapContextComponent
fn example_context_fetcher(
    inp: ContextFetcherInputs,
    context_data_qry: Query<Entity, With<ExampleStateMapContextComponent>>,
) -> ContextFetcherOutputs {
    bevy::log::debug!("example_context_fetcher triggered for AI {:?}", inp.0.0);
    context_data_qry.iter().collect()
}

fn setup_example_context(
    mut commands: Commands,
) {
    let mut statemap = CraniumKvMap::with_capacity(3);
    statemap.insert(ActionState::Ready, ActionState::Running);
    statemap.insert(ActionState::Running, ActionState::Failed);
    statemap.insert(ActionState::Failed, ActionState::Failed);

    let component = ExampleStateMapContextComponent {statemap};
    let backup = component.clone();

    let newent = commands.spawn((
        component,
        DumbMarker::default(),
    ));
    let ctx_id = newent.id();

    commands.spawn((
        backup.clone(),
        DumbMarker::default(),
    ));

    bevy::log::debug!("setup_example_context - Spawned a Context entity with ID {:?} w/ {:?}", ctx_id, backup)
}

fn setup_example_entity(
    mut commands: Commands,
    mut actionset_store: ResMut<ActionSetStore>,
) {
    let example_actions = [
        ActionTemplate::new(
            "ExampleAction",
            EXAMPLE_CONTEXT_FETCHER_NAME.to_string(), 
            crate::types::CraniumList::from([
                ConsiderationData::new(
                    "e2e::One",
                    "Linear",
                    0.,
                    1.5,
                ),
                ConsiderationData::new(
                    "e2e::Two",
                    "Square",
                    0.,
                    1.,
                )
            ]),
            1.,
            "e2e::ExampleAction",
            None, 
            None,
        )
    ];

    let example_actionset = ActionSet {
        name: "ExampleActionSet".to_string(),
        actions: cranium::types::CraniumList::from(example_actions)
    };

    actionset_store.map_by_name.insert(example_actionset.name.to_owned(), example_actionset);

    let new_controller = AIController::default();
    let new_sos = SmartObjects {
        actionset_refs: ThreadSafeRef::new(cranium::types::CraniumList::from(["ExampleActionSet".to_string()]))
    };

    let spawned = commands.spawn((
        new_controller,
    ));

    let ai_id = spawned.id();

    commands.trigger(AiDecisionRequested { 
        entity: ai_id,  
        request_key: None, 
        smart_objects: Some(new_sos),
    });
}

fn setup_default_action_tracker_config(
    mut config_res: ResMut<UserDefaultActionTrackerSpawnConfig>
) {
    config_res.with_config_builder(|builder|
        builder
        .set_use_ticker(true)
        .set_use_timers(false)
    );
}


fn example_consideration_one(
    _inputs: ConsiderationInputs,
    qry: Query<&ActionTrackerState>
) -> ConsiderationOutputs {
    let mut good_cnt = 0;
    let mut bad_cnt = 0;

    for tracker in qry {
        match tracker.get_state() {
            ActionState::Failed => { bad_cnt += 1 },
            _ => { good_cnt += 1 },
        }
    }

    let total_cnt = good_cnt + bad_cnt;

    let val = if total_cnt > 0 {
        (good_cnt as ActionScore) / (total_cnt as ActionScore)
    } else {
        1.
    };

    val.into()
}

/// Trivial Consideration, returns a flat value.
fn example_consideration_two(
    _inputs: ConsiderationInputs,
) -> ConsiderationOutputs {
    0.9.into()
}

/// A Consideration that measures the distance (Euclidian, so straight-line) between 
/// the AI's Pawn and the Context 'target'. 
/// 
/// This is a common and insanely useful building block for a variety of Actions, e.g. 
/// - To favor targets close by for pickup/interaction/combat 
/// - To select spawn locations a minimum distance away from the player
/// - To cheaply pre-filter locations for pathfinding or raycasting Considerations downstream
fn example_consideration_three(
    inputs: ConsiderationInputs,
    qry: Query<&Position2d>
) -> ConsiderationOutputs {
    
    let (ai, maybe_pawn, targ) = inputs.0;

    let pawn = match maybe_pawn {
        None => {
            bevy::log::error!(
                "example_consideration_three requires a Pawn, but AI {:?} does not have one!",
                ai, 
            );
            return None
        },
        Some(p) => p,
    };

    let pawn_pos = match qry.get(pawn) {
        Err(_) => {
            bevy::log::error!(
                "example_consideration_three requires the Pawn to have a Position2d, but Pawn {:?} of AI {:?} does not!",
                pawn, ai,
            );
            return None
        }
        Ok(pos) => pos,
    };

    let targ_pos = match qry.get(targ) {
        Err(_) => {
            bevy::log::error!(
                "example_consideration_three requires the Context to have a Position2d, but Context {:?} for AI {:?} does not!",
                targ, ai,
            );
            return None
        }
        Ok(pos) => pos,
    };

    // Calculate the actual raw score:
    let val = pawn_pos.euclid_distance(targ_pos);

    // We'll use .into() to ensure that we can ignore simple output interface changes.
    val.into()
}


fn example_action_handler(
    inputs: ActionHandlerInputs,
    mut commands: Commands, 
) {
    bevy::log::info!("Triggering a ExampleActionEvent w/ inputs {:?}...", &inputs);
    let (ai, pawn, ctx) = inputs;
    commands.trigger(
        ExampleActionEvent::from_context_ref(
            ctx,
            ai,
            pawn,
        )
    );
}


fn main() {
    let mut app = App::new();

    // A custom user-defined Curve
    let leaky_curve = LinearCurve.hard_leak(0.5);

    app
    // Enables the main Cranium integration:
    .add_plugins(CraniumPlugin)
    
    // Configures the app to shut down once all Actions are finished at the end of a tick, plus logs and such:
    .add_plugins(CraniumTestPlugin)
    
    // We'll use 'ticking' Actions (a'la Update() method on non-ECS GameObjects) for this demo.
    .add_plugins(TickBasedActionTrackerPlugin)

    // Registering Considerations/ContextFetchers/Curves.
    //
    // We'll use an 'e2e::' prefix as a convention to quasi-namespace user-registered Systems. 
    // This is not required, just a good practice to avoid key collisions from multiple Plugins. 
    // In case of a collision, the most recently registered value takes precedence. 
    .register_consideration(example_consideration_one, "e2e::One")
    .register_consideration(example_consideration_two, "e2e::Two")
    .register_consideration(example_consideration_three, "e2e::DistanceToPawn2d")
    .register_context_fetcher(example_context_fetcher, EXAMPLE_CONTEXT_FETCHER_NAME)
    .register_utility_curve(leaky_curve, "e2e::Linear50pHardLeak")
    .register_action_handler(example_action_handler, "e2e::ExampleAction")
    
    // Setting up various demo Entities
    .add_systems(Startup, (
        setup_example_context, 
        setup_example_entity, 
        setup_default_action_tracker_config,
    ))
    
    // Setting up an Observer for our Action.
    .add_observer(example_action)

    // Adds a logging callback to let us know how and when the Action State changes.
    .add_observer(action_state_change_observer)
    ;

    app.run();

    bevy::log::info!("All actions finished - Bevy app exited successfully. Exiting Cranium shortly...");
    // Delay the exit to let people using one-off terminal windows see what's going on.
    let mut counter = 0u32;
    while counter < 100 {
        counter += 1;
        platform::thread::sleep(core::time::Duration::from_millis(100));
    }
}
