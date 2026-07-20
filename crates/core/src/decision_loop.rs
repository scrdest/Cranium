/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! The meat of the AI - the logic that glues all the Utility concepts together to output Action choices.

use core::borrow::Borrow;

use bevy::prelude::*;

use crate::actions;
use crate::ai::{AIController};
use crate::context_fetchers::{ContextFetcherKeyToSystemMap, ShouldReinitCfQueries};
use crate::considerations::{ConsiderationKeyToSystemMap, ShouldReinitConsiderationQueries};
use crate::curves::{SupportedUtilityCurve, UtilityCurve, UtilityCurveRegistry, resolve_curve_from_name};
use crate::errors::NoCurveMatchStrategyConfig;
use crate::events::{AiActionPicked, AiDecisionInitiated, AiDecisionRequested, NoDecisionMessage, SomeAiDecisionProcessed};
use crate::lods::{AILevelOfDetail};
use crate::pawn::Pawn;
use crate::smart_object::ActionSetStore;
use crate::types::{self, ActionContextRef, ActionScore, ActionTemplateRef, ThreadSafeRef};

/// Correction formula as per the GDC 2015 "Building a Better Centaur AI" 
/// presentation by Dave Mark and Mike Lewis.
/// 
/// Ensures that we do not penalize Actions for having multiple Considerations 
/// by giving scores a small bonus for each Consideration scored.
/// 
/// The input should be the raw score, the product of Consideration scores 
/// with respective Curves applied but WITHOUT the Priority multiplier on top.
/// 
/// The output is guaranteed to remain clamped within the 0-1 range; 
/// the adjustment only applies to scores below the maximum.
/// 
/// Example w/ 5 Considerations:
/// - Input 0.900 => Output = 0.972
/// - Input 0.500 => Output = 0.700
/// - Input score 1.000 => Output score = 1.000
/// 
/// Example w/ 10 Considerations:
/// - Input 0.900 => Output = 0.981
/// - Input 0.500 => Output = 0.725
/// - Input score 1.000 => Output score = 1.000
/// 
fn consideration_adjustment(
    score: types::ActionScore,
    num_considerations: usize,
) -> types::ActionScore {

    // If a score is at zero (or below, somehow), there's no saving it anyway.
    if score <= types::MIN_CONSIDERATION_SCORE {
        // We could return raw score as well, but we'll safeguard the expected range here.
        return types::MIN_CONSIDERATION_SCORE
    }

    // The formula doesn't adjust scores above 1.0 anyway; 
    // saves us some unnecessary calculations.
    if score >= types::MAX_CONSIDERATION_SCORE {
        // We could return raw score as well, but we'll safeguard the expected range here.
        return types::MAX_CONSIDERATION_SCORE
    }
    
    // Division by zero workaround.
    if num_considerations <= 0 {
        return score
    }

    let floaty_num_considerations = num_considerations as f32;
    let modification_factor = 1. - (1. / floaty_num_considerations);
    let makeup_val = (1. - score) * modification_factor;
    let adjusted_score = score + (makeup_val * score);

    #[cfg(feature = "logging")]
    bevy::log::debug!(
        "Adjusted raw score {:?} w/ {:?} Considerations (float: {:?}) to {:?}",
        score,
        num_considerations,
        floaty_num_considerations,
        adjusted_score,
    );

    adjusted_score
}


/// A helper Observer that handles the setup for a Decision.
pub fn prepare_ai(
    event: On<AiDecisionRequested>,
    should_reinit_cf_queries: Option<ResMut<ShouldReinitCfQueries>>,
    should_reinit_cons_queries: Option<ResMut<ShouldReinitConsiderationQueries>>,
    mut commands: Commands,
) {
    #[cfg(feature = "logging")]
    bevy::log::debug!(
        "AiDecisionRequested Event fired, preparing AI for Entity {} (Request: {:?})", 
        event.entity,
        event.request_key,
    );

    should_reinit_cf_queries.map(|mut res| {
        res.set(true);
    });

    should_reinit_cons_queries.map(|mut res| {
        res.set(true);
    });
    
    commands.trigger(AiDecisionInitiated {
        entity: event.entity,
        request_key: event.request_key.clone(),
        smart_objects: event.smart_objects.clone(),
    });
}

pub fn disable_cf_reinit(
    _event: On<crate::events::SomeAiDecisionProcessed>,
    should_reinit_cf_queries: Option<ResMut<ShouldReinitCfQueries>>,
) {
    should_reinit_cf_queries.map(|mut res| {
        res.set(false);
    });
}

pub fn disable_consideration_reinit(
    _event: On<crate::events::SomeAiDecisionProcessed>,
    should_reinit_cons_queries: Option<ResMut<ShouldReinitConsiderationQueries>>,
) {
    should_reinit_cons_queries.map(|mut res| {
        res.set(false);
    });
}


/// Core AI decision loop. 
/// 
/// Finds the `Action` with the highest Utility Score and triggers an `ActionPickedEvent`.
/// 
/// Note that this is an Observer that runs for an *individual* `AiController`. 
/// Running all AIs at all times by default would be a waste of compute.
/// 
/// We iterate through all available ActionTemplates (from all available sources), 
/// then - after filtering out obvious non-starters - fetch potential `Contexts` for 
/// them to form a full-fledged candidate `Action`. 
/// 
/// We then apply Considerations specified in the ActionTemplate for each candidate 
/// in sequence, each returning a raw score, which is then adjusted using UtilityCurves 
/// to calculate the true Axis Score.
/// 
/// The final score for an Action is a product of all Axis Scores starting from 1.0 
/// (a classic simple fold/reduce type function), strictly non-increasing. There is 
/// also a nonlinear 'bonus' applied rewarding Actions with more Considerations to 
/// adjust for the downward drift adding extra Considerations incurs.
/// 
/// This is a gauntlet; any candidate whose value drops to zero is eliminated instantly, 
/// as is any candidate whose score dips below the frontrunner. Considerations may be 
/// expensive (multiple raycasts, complex formulas, etc.) so we avoid paying for those 
/// we are never going to actually use.
/// 
/// ContextFetchers, Considerations, and Curves used by this system can all be provided 
/// by the user or by third-party plugins! 
/// 
/// We refer to them by String keys in SmartObject definitions and resolve them at runtime 
/// using a couple of custom Resources provided by Cranium; see `app.register_consideration()`, 
/// `app.register_context_fetcher()` and `app.register_utility_curve()` for API details.
pub fn decision_engine(
    event: On<AiDecisionInitiated>,
    world_ref: &World, 
    actionset_store: Res<ActionSetStore>,
    context_fetcher_system_map: Res<ContextFetcherKeyToSystemMap>,
    consideration_system_map: Res<ConsiderationKeyToSystemMap>,
    entity_checker: Query<Entity, With<AIController>>, 
    lod_query: Query<Option<&AILevelOfDetail>>, 
    pawn_query: Query<Option<&Pawn>>,
    utility_curve_registry: Option<Res<UtilityCurveRegistry>>,
    no_match_strategy_config: Option<Res<NoCurveMatchStrategyConfig>>,
    mut commands: Commands,
) {
    #[cfg(feature = "logging")]
    bevy::log::debug!(
        "AiDecisionInitiated Event fired, running Decision Engine for Entity {} (Request: {:?})", 
        event.entity,
        event.request_key,
    );
    // Marks that SOMEONE has done some AI processing in this world-loop tick. 
    // This is currently mainly used to disable unnecessary duplicate reinits 
    // until the next time some AI decides to run and will actually use them.
    commands.trigger(SomeAiDecisionProcessed);

    let audience = event.event_target();

    let exist_check = entity_checker.get(audience);
    if exist_check.is_err() {
        // Early termination - the AI the decision was requested for either got despawned or the request 
        // was malformed and was pointed at something that was not an AI in the first place.
        #[cfg(feature = "logging")]
        bevy::log::debug!("decision_engine: Decision request target {:?} is not an AI - ignoring the request.", audience);
        commands.write_message(
            NoDecisionMessage {
                entity: Some(event.entity),
                request_key: event.request_key,
                comment: Some("Target is not an AI!\0"),
            }
        );
        return;
    }
    
    let lod_level = lod_query
        .get(audience)
        .ok()
        .flatten()
        .map(|lod| lod.get_current_lod())
    ;

    let is_disabled = lod_level.map(|lod| lod.is_inactive() ).unwrap_or(false);
    if is_disabled {
        // Early termination - this AI is disabled; generally we'd hope AiDecisionRequested would not even
        // fire in the first place, but weird things can sometimes happen in sufficiently big projects...
        #[cfg(feature = "logging")]
        bevy::log::debug!("decision_engine: AI {:?} disabled by LOD - ignoring decision request.", audience);
        commands.write_message(
            NoDecisionMessage {
                entity: Some(event.entity),
                request_key: event.request_key,
                comment: Some("Target LOD is below the processing threshold.\0"),
            }
        );
        return;
    }
    
    // Best score reached for this AI, globally
    // If any batch Score dips below this, we can discard the whole batch immediately 
    // as it cannot possibly beat the current best.
    let mut best_scoring_triple: Option<(ActionScore, ActionTemplateRef, ActionContextRef)> = None;

    // Best score reached for this ActionTemplate
    // This is a bit more 'local' than the per-AI score
    let mut best_scoring_template: Option<(ActionTemplateRef, ActionScore)> = None;
    
    let maybe_smartobjects = &event.smart_objects;
    let maybe_pawn = pawn_query.get(audience).ok().flatten().cloned();
    
    // 1. Gather ActionSets from Smart Objects
    let smartobjects = match maybe_smartobjects {
        None => {
            // Early return - we have no real options in this case.
            // Note that there is no notion of available Actions *NOT* tied to a SO; at
            // minimum, you'd have a SO with the key representing *the Controller itself*.
            #[cfg(feature = "logging")]
            bevy::log::debug!("decision_engine: AI {:?} - no SmartObjects available, idling", audience);
            commands.write_message(NoDecisionMessage {
                entity: Some(event.entity),
                request_key: event.request_key.clone(),
                comment: Some("Target has no SmartObjects available.\0"),
            });
            return;
        }
        Some(sos) => sos
    };

    let available_actions = smartobjects.actionset_refs.iter().filter_map(
        |actionset_key| {
            let maybe_act = actionset_store.map_by_name.get(actionset_key);
            maybe_act
        }
    )
    .flat_map(|acts| {
        acts.actions
        .iter()
        .cloned()
        .map(|act| ThreadSafeRef::new(act))
    });

    #[cfg(feature = "logging")]
    bevy::log::debug!(
        "decision_engine: AI {:?} - available Actions are: {:#?}", 
        audience, &smartobjects.actionset_refs
    );

    // Flags to abandon a Decision Engine run entirely due to a serious error 
    // (most likely, a poisoned lock).
    //
    // Instead of a boolean, it holds an optional 'reason' field, and if it is 
    // populated, it is considered to be set (i.e. trigger an abort).
    //
    // Aborting means that the AI will NOT be making a decision at all.
    // The system will emit a NoDecisionMessage as usual for such a scenario.
    let mut abort_processing = None;

    // 2. Emit a request for Context for each ActionTemplate.
    for action_template in available_actions {
        if abort_processing.is_some() {
            // abort_processing functions as a flag to abandon this run entirely
            break
        }

        if !action_template.is_within_lod_range(&lod_level) {
            #[cfg(feature = "logging")]
            bevy::log::debug!(
                "decision_engine: AI {:?} - skipping Template {:?} - current LOD {:?} does not allow for processing.", 
                &action_template.name, &audience, &lod_level,
            );
            continue;
        }

        #[cfg(feature = "logging")]
        bevy::log::debug!(
            "decision_engine: AI {:?} - requesting Contexts for Template {:?} from CF {:?}", 
            &audience, &action_template.name, &action_template.context_fetcher_name,
        );
        
        // Request Contexts using registered ContextFetcher Systems
        let cf_system = context_fetcher_system_map.mapping
            .get(&action_template.context_fetcher_name.0)
        ;

        let contexts = match cf_system {
            Some(system_guard) => {
                let entity_res = {
                    let res = system_guard.write().map(|mut cf_system| {
                        cf_system.run_readonly(
                            (
                                audience,
                                maybe_pawn.clone().map(|p| p.to_entity()).flatten(),
                            ),
                            world_ref,
                        )
                    });

                    if let Err(err) = res {
                        let err_msg = format!(
                            "decision_engine: AI {:?} - ContextFetcher '{:?}' errored - lock poisoned ({:?})!", 
                            &audience, 
                            &action_template.context_fetcher_name, 
                            &err,
                        );

                        #[cfg(feature = "logging")]
                        bevy::log::error!(err_msg);

                        abort_processing = Some(err_msg);
                        break;
                    }

                    // If we got here at all, we know this is safe to unwrap. 
                    // The expect() is pro-forma - if we hit it, something got badly messed up in the code.
                    res.expect("decision_engine: ContextFetcher lock guard errored out without aborting - this should not be possible!")
                };

                if let Err(err) = entity_res {
                    // If we got here, the error comes from a ContextFetcher.
                    // That means it's bad (user needs to fix their code), but not fatal 
                    // (other CFs can happily keep running; even this one may work for
                    //  other inputs depending on the nature of the issue)

                    #[cfg(feature = "logging")]
                    bevy::log::error!(
                        "decision_engine: AI {:?} - ContextFetcher '{:?}' errored: {:?}", 
                        &audience, 
                        &action_template.context_fetcher_name, 
                        &err,
                    );

                    continue;
                }

                entity_res.expect("decision_engine: ContextFetcher result is Err - this should not be possible!")
            },
            None => {
                #[cfg(feature = "logging")]
                bevy::log::error!(
                    "decision_engine: AI {:?} - ContextFetcher key '{:?}' could not be resolved to a System!", 
                    &audience, 
                    &action_template.context_fetcher_name, 
                );
                continue;
            }
        };

        for ctx in contexts {
            // A flag that indicates the whole processed Context is unusable; 
            // when true, this loop should continue out to the next value and
            // any nested loop should break ASAP to avoid wasting processing.
            let mut skip_this_context = false;

            let ctx_ref = ctx;
            
            #[cfg(feature = "logging")]
            bevy::log::debug!("decision_engine: AI {:?} - processing Ctx {:?} for Action {:?}", 
                &audience,
                &ctx_ref, 
                &action_template,
            );

            let curr_best_for_ai = best_scoring_triple
                .as_ref()
                .map(|tup| tup.0)
            ;

            // We do not unwrap curr_best_for_ai fully to be clearer when it's null vs zero.
            if let Some(some_curr_best) = &curr_best_for_ai {
                if some_curr_best >= &action_template.priority {
                    // Priority forms a ceiling for maximum final score.
                    // At Priority 1, the max score is 1.0; at 2 -> 2.0; at 5 -> 5.0 etc.
                    // If we have a Priority 1 Action and the high score is 2.2, we will never beat it.
                    // 
                    // Note that in general, a lower-Priority template can still win over a higher-Priority one; 
                    // this would happen if the high-Priority score gets cut down heavily by its Considerations.
                    // 
                    // For example, a P1 Idle can beat P5 Heal if IsHurt Consideration for the latter returns 0 
                    // (and so the final score for Heal is 5.0 * 0.0 => 0.0)
                    // 
                    // Here, we are tracking the top SCORE, not top PRIORITY processed, so skipping is valid.
                    continue;
                }
            }

            // The current total score for this AI + Action
            let mut curr_score = types::MAX_CONSIDERATION_SCORE;
            let mut consideration_count: usize = 0;

            for (cons_cnt, cons) in action_template.considerations.iter().enumerate() {
                // We'll use the Registry resource if we have one and fall back to the hardcoded pool if we do not.
                let mut maybe_resolved_curve: Option<SupportedUtilityCurve> = utility_curve_registry
                    .as_ref()
                    .map(|curve_mapping| 
                        curve_mapping.get_curve_by_name(&cons.curve_name)
                    )
                    .flatten()
                    .or_else(|| resolve_curve_from_name(&cons.curve_name))
                ;

                if maybe_resolved_curve.is_none() {
                    let curve_miss_strategy = no_match_strategy_config
                        .as_ref()
                        .map(|conf| conf.get_current_value())
                    ;

                    match curve_miss_strategy {
                        None => {
                            // This is a duplicate of the SkipActionWithLog strategy as indicated by the Default impl. 
                            // We COULD create a fallback value earlier, but that would cost us an extra 
                            // `.clone()` that we can kinda do without here just as well.
                            #[cfg(feature = "logging")]
                            bevy::log::warn!(
                                "decision_engine: AI {:?} - failed to resolve Curve key {:?} to a SupportedUtilityCurve, skipping ActionTemplate {:?}!", 
                                &audience,
                                &cons.curve_name,
                                &action_template.name,
                            );
                            break;
                        },

                        Some(crate::errors::NoCurveMatchStrategy::Panic) => {
                            #[cfg(feature = "logging")]
                            bevy::log::error!(
                                "decision_engine: AI {:?} - Failed to resolve Curve key {:?} to a SupportedUtilityCurve, panicking!", 
                                &audience,
                                &cons.curve_name
                            );
                            panic!("decision_engine: Failed to resolve Curve key to a SupportedUtilityCurve!");
                        },

                        Some(crate::errors::NoCurveMatchStrategy::SkipConsiderationWithLog) => {
                            #[cfg(feature = "logging")]
                            bevy::log::warn!(
                                "decision_engine: AI {:?} - failed to resolve Curve key {:?} to a SupportedUtilityCurve, skipping Consideration {:?}!", 
                                &audience,
                                &cons.curve_name,
                                &cons.consideration_name,
                            );
                            continue;
                        },

                        Some(crate::errors::NoCurveMatchStrategy::SkipActionWithLog) => {
                            #[cfg(feature = "logging")]
                            bevy::log::warn!(
                                "decision_engine: AI {:?} - failed to resolve Curve key {:?} to a SupportedUtilityCurve, skipping ActionTemplate {:?}!", 
                                &audience,
                                &cons.curve_name,
                                &action_template.name,
                            );
                            break;
                        },

                        Some(crate::errors::NoCurveMatchStrategy::DefaultCurveWithLog(curve_resolver)) => {
                            let resolved = curve_resolver(cons.curve_name.borrow());
                            
                            #[cfg(feature = "logging")]
                            bevy::log::warn!(
                                "AI {:?} - Curve key {:?} resolved using fallback Curve {:?}", 
                                &audience,
                                &cons.curve_name,
                                &resolved,
                            );

                            maybe_resolved_curve = Some(resolved)
                        },

                        Some(crate::errors::NoCurveMatchStrategy::DefaultCurveWithoutLog(curve_resolver)) => {
                            let resolved = curve_resolver(cons.curve_name.borrow());
                            maybe_resolved_curve = Some(resolved)
                        },
                    }
                }

                // We can safely unwrap this as any handling/panicking has been done earlier.
                let resolved_curve = maybe_resolved_curve.unwrap();

                let consideration_system = consideration_system_map.mapping
                    .get(&cons.consideration_name)
                ;

                match consideration_system {
                    None => {
                        #[cfg(feature = "logging")]
                        bevy::log::error!(
                            "decision_engine: AI {:?} - Failed to resolve Consideration '{:}' to a System!", 
                            &audience,
                            &cons.consideration_name
                        );
                        break;
                    },

                    Some(system_guard) => {
                        let system_state = system_guard.write();
                        
                        let run_res = {
                            let res = system_state
                                .map(|mut consideration_system| {
                                    consideration_system.run_readonly(
                                    (
                                            audience.entity(),
                                            maybe_pawn.clone().map(|p| p.to_entity()).flatten(),
                                            ctx_ref.clone(),
                                        ),
                                        world_ref,
                                    )
                                })
                            ;

                            if let Err(err) = res {
                                let err_msg = format!(
                                    "AI {:?} - Consideration '{:}' errored - lock poisoned ({:?})!", 
                                    &audience, 
                                    &cons.consideration_name, 
                                    err,
                                );

                                #[cfg(feature = "logging")]
                                bevy::log::error!(err_msg);
                                
                                abort_processing = Some(err_msg);
                                break;
                            };

                            // Only really needed if we don't break on poisoned locks.
                            res.expect("Consideration failed - lock poisoned!")
                        };

                        let raw_score = match run_res {
                            Err(_err) => {
                                #[cfg(feature = "logging")]
                                bevy::log::error!(
                                    "decision_engine: AI {:?} - Consideration '{:}' errored: {:?}", 
                                    &audience, 
                                    &cons.consideration_name, 
                                    &_err
                                );
                                curr_score = types::MIN_CONSIDERATION_SCORE;
                                break;
                            },
                            Ok(maybe_val) => match maybe_val {
                                Some(val) => val,
                                None => {
                                    // A None return value signifies something went wrong, but it's not worth crashing over. 
                                    // 
                                    // Usually, this is an issue with either the Context or the Pawn not satisfying the Consideration 
                                    // invariants (for example, a Consideration requires a Pawn, but it is null, or the Contexts should 
                                    // all have SomeRandomComponent but the ContextFetcher returned one without it somehow).
                                    //
                                    // This is distinct from returning zero, as zero 
                                    #[cfg(feature = "logging")]
                                    bevy::log::info!(
                                        "decision_engine: AI {:?} - Consideration '{:}' returned a None score, indicating a nonfatal error. Defaulting to zero score.", 
                                        &audience, 
                                        &cons.consideration_name, 
                                    );
                                    curr_score = types::MIN_CONSIDERATION_SCORE;
                                    skip_this_context = true; 
                                    break;
                                }
                            }
                        };

                        let (true_min, true_max) = match cons.min <= cons.max {
                            true => (cons.min, cons.max),
                            false => {
                                #[cfg(feature = "logging")]
                                bevy::log::error!(
                                    "Min/Max values for Consideration {:?} in Action {:?} 
                                    were flipped, min={:?} > max={:?}. 
                                    They have been flipped back so Min<=Max for you for now. 
                                    This fixup is not guaranteed to be in place in future versions of the library!",
                                    cons.consideration_name,
                                    &action_template.name,
                                    cons.min,
                                    cons.max,
                                );
                                (cons.max, cons.min)
                            }
                        };

                        let true_span = true_max - true_min;
                        if true_span == 0. {
                            #[cfg(feature = "logging")]
                            bevy::log::warn!(
                                "Min/Max values for Consideration {:?} in Action {:?} 
                                are both zero! min={:?}, max={:?}. 
                                This would result in a division-by-zero and will instead be  
                                interpreted as a disabled Consideration and ignored. If you did not  
                                expect both values to be zero, you should investigate the Action spec.",
                                cons.consideration_name,
                                &action_template.name,
                                cons.min,
                                cons.max,
                            );
                            continue;
                        }

                        // Remap the raw Consideration score (arbitrary value) to a unit interval. 
                        // Values outside of range get saturated to min/max (as appropriate), so 
                        // e.g. if min = -1 and raw_score = -5, we read the raw_score as just -1.
                        // Similarly if max = -4 and raw_score = -1, we read the raw_score as just -4.
                        let clamped = raw_score.clamp(true_min, true_max);
                        let rescaled_score = (clamped - true_min) / (true_max - true_min);

                        // Apply the Curve mapping to the raw score.
                        let score = resolved_curve.sample_safe(rescaled_score);

                        // The actual (raw) score is the product of all Consideration scores so far.
                        curr_score *= score;

                        #[cfg(feature = "logging")]
                        bevy::log::debug!(
                            "decision_engine: AI {:?} - Consideration '{:}' for Action {:?}:  
                            - Raw score => {:?}
                            - Rescaled w/ min/max => {:?}
                            - Adjusted w/ Curve {:?} => {:?}
                            - Current running total score for Action => {:?}",
                            audience,
                            cons.consideration_name,
                            &action_template.name,
                            raw_score,
                            rescaled_score,
                            cons.curve_name,
                            score,
                            curr_score,
                        );

                        // There is a superior Context for this ActionTemplate.
                        // We don't need to bother checking other Considerations for this Context, 
                        // as it will not get picked anyway.
                        let curr_beats_old_best = match &best_scoring_template {
                            None => true,
                            Some((_old_best_tmpl, old_best_score)) => &curr_score > old_best_score,
                        };

                        if !curr_beats_old_best {
                            #[cfg(feature = "logging")]
                            bevy::log::debug!(
                                "decision_engine: AI {:?} - Consideration '{:}' for Action {:?} - curr_score {:?} is below the template best of {:?}, discarding the Context.",
                                audience,
                                cons.consideration_name,
                                &action_template.name,
                                curr_score,
                                best_scoring_template,
                            );
                            skip_this_context = true; break;
                        }

                        // We need to know how many Considerations we have processed for later.
                        // Enumerate starts at zero, so we need to add one to adjust.
                        consideration_count = cons_cnt + 1;
                    }
                }
            }

            if skip_this_context {
                continue;
            }

            let curr_beats_old_best = match &best_scoring_template {
                None => true,
                Some((_old_best_tmpl, old_best_score)) => &curr_score > old_best_score,
            };

            if curr_beats_old_best {
                // Update best Context score for Template to skip sub-optimals
                let _ = best_scoring_template.insert((
                    action_template.to_owned(), 
                    curr_score.to_owned()
                ));
            }

            let adjusted_score = consideration_adjustment(
                curr_score, 
                consideration_count,
            );

            // todo: add a parametrizeable amount of randomness for break-evens
            let prioritized_score = adjusted_score * action_template.priority;

            match prioritized_score > curr_best_for_ai.unwrap_or(types::MIN_CONSIDERATION_SCORE) {
                false => {
                    #[cfg(feature = "logging")]
                    bevy::log::debug!(
                        "AI {:?} - Score for Action {:?} = {:?} is below the current best of {:?}. Ignoring.",
                        &audience,
                        &action_template.name,
                        prioritized_score,
                        curr_best_for_ai,
                    );
                },
                true => {
                    #[cfg(feature = "logging")]
                    bevy::log::debug!(
                        "AI {:?} - Score for Action {:?} = {:?} beats the current best of {:?}. Promoting to new best.",
                        &audience,
                        &action_template.name,
                        prioritized_score,
                        curr_best_for_ai,
                    );

                    // Update frontrunner.
                    best_scoring_triple = Some((prioritized_score, action_template.clone(), ctx_ref))
                }
            }
        }
    }

    if let Some(_abort_reason) = abort_processing {
        commands.write_message(NoDecisionMessage {
            entity: Some(event.entity),
            request_key: event.request_key.clone(),
            comment: Some("Decision Engine run aborted due to a serious error!\0"),
        });
        return;
    }
    
    match best_scoring_triple {
        None => {
            #[cfg(feature = "logging")]
            bevy::log::info!(
                "decision_engine: AI {:?} - no suitable Actions found.",
                &audience,
            );
            commands.write_message(NoDecisionMessage {
                entity: Some(event.entity),
                request_key: event.request_key.clone(),
                comment: Some("No suitable Actions found!\0"),
            });
        }
        Some(best_tuple) => {
            let (
                best_score, 
                best_template, 
                best_context
            ) = best_tuple;

            #[cfg(feature = "logging")]
            bevy::log::info!(
                "decision_engine: AI {:?} - Picking Action {:?} w/ Score {:?}.", 
                &audience,
                &best_template.name,
                &best_score,
            );

            let pick_evt = AiActionPicked {
                entity: audience.entity(),
                action_key: best_template.action_key.to_owned(),
                action_name: best_template.name.to_owned(),
                action_context: best_context.to_owned(),
                action_score: best_score,
                request_key: event.request_key
            };

            commands.trigger(pick_evt);
        }
    }
}

pub fn trigger_dispatch_to_user_actions(
    trigger: On<crate::events::AiActionPicked>,
    mut writer: MessageWriter<crate::events::AiActionDispatchToUserCode>,
) {
    let event = trigger.event();
    let action_key = &event.action_key;
    #[cfg(feature = "logging")]
    bevy::log::debug!(
        "dispatch_to_user_actions - Running for Action {:?} for Pick Event {:?}",
        action_key, event
    );

    let message = crate::events::AiActionDispatchToUserCode::new(
        event.entity, 
        action_key.to_owned(), 
        event.action_name.to_owned(), 
        event.action_context, 
        event.action_score
    );
    writer.write(message);
}

pub fn handle_dispatch_to_user_actions(
    pawn_query: Query<Option<&Pawn>>,
    mut commands: Commands,
    mut reader: MessageReader<crate::events::AiActionDispatchToUserCode>,
    mut callback_registry: ResMut<actions::ActionHandlerKeyToSystemMap>,
) {
    for msg in reader.read() {
        let action_key = &msg.action_key;
    
        #[cfg(feature = "logging")]
        bevy::log::debug!(
            "dispatch_to_user_actions - Running for Action {:?} for message {:?}",
            &action_key, &msg
        );

        let callback = match callback_registry.mapping.get_mut(action_key) {
            Some(cb) => cb,
            None => {
                #[cfg(feature = "logging")]
                bevy::log::error!(
                    "dispatch_to_user_actions - Could not resolve ActionKey {:?} to a registered ActionPickCallback, skipping!",
                    action_key
                );
                continue;
            }
        };

        let ai = msg.entity;
        let ctx = msg.action_context;
        let pawn = pawn_query
            .get(ai)
            .ok().flatten()
            .map(|p| p.clone().to_entity())
            .flatten()
        ;

        callback.call((ai, pawn, ctx), commands.reborrow());
    }
}
