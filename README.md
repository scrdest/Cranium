# Cranium

[![License](https://img.shields.io/badge/license-MPL%202.0-blue.svg)](https://github.com/scrdest/Cranium/blob/master/LICENSE.md)
[![Crates.io](https://img.shields.io/crates/v/cranium.svg)](https://crates.io/crates/cranium)
[![Downloads](https://img.shields.io/crates/d/cranium.svg)](https://crates.io/crates/cranium)
[![Docs](https://docs.rs/cranium/badge.svg)](https://docs.rs/cranium/latest/cranium/)

**Mission Statement:** 

> See more vibrant, interesting, living and breathing virtual worlds in games  
> by making it as easy as possible for developers to create and run them efficiently.

`Cranium` is an opinionated, modular, no_std-friendly (if you have access to alloc), 
data-driven (and data-oriented) Rust library for 'classical' AI (primarily but not 
exclusively Utility AI) for interactive applications such as games and simulations.

`Cranium` is built using the [Bevy Game Engine](https://bevyengine.org/). 

However, a key design goal of the library is to be usable **outside** of Bevy - and even for
non-Rust applications (*"AI Server style"*), while integrating cleanly *with* Bevy
applications at the same time (*"Native AI style"*).

In fact, a major motivator for its creation was the need for a performant AI engine 
for NPCs in a project written in a deeply locked-down, decades-old game engine. 


## Features

* **Data-driven** - Let coders code core capabilities and let designers (and modders!) play with them.
* **High-performance** - Beyond 'blazingly fast': parallelism, data-oriented processing, an efficient core AI paradigm, and a bag of game AI optimization tricks to keep framerates smooth and crowds large.
* **Batteries Included** - Covering the hard parts the tutorials skip. LODs, state machines, per-agent knowledge base and knowledge sharing... 
* **Modular** - Disable things you don't use! Add extra stuff you need from your own app code!
* **Generic** - For turn-based or real-time, strategy or shooters, or even a non-game app... except possibly chess engines. 
* **Proven** - Informed by over a decade of experience building complex and performant game AIs and a small forest's worth of papers about game AI design. 
* **Reactive** - Uses event-driven mechanisms to avoid paying for systems you don't use and allowing you to hook into the ones that you might be interested in in your own code.
* **Portable** - Use in your Bevy app as a Plugin or drive it yourself from another engine. 
* **Safe** - Typechecker-approved, uses thread-safe solutions where multithreading is relevant.
* **MPL-2.0** - Both FOSS and commercial-friendly licensing, just don't gatekeep source forks.


## Version compatibility

| Bevy |              Cranium                | 
|------|-------------------------------------|
| 0.19 | 0.3                                 |
| 0.18 | 0.2                                 |
| 0.17 | 0.1, 0.2                            |


In case of multiple comma-separated values, multiple versions support this Bevy version.

### Example 

For an explanation of the concepts used in the code below, see the [Glossary](GLOSSARY.md)

You may also want to have a look at the [Examples folder](examples) for additional executable examples.

```rust
// Include the building blocks from Bevy and Cranium.
use cranium_core::bevy::prelude::*;
use cranium::prelude::*;
use cranium_bevy_plugin::CraniumPlugin;

/// A simple 2d Position component for demo purposes only.
#[derive(Component, Clone, Copy, Debug, Default)]
struct Position2d(Vec2);

impl Position2d {
    fn euclid_distance(&self, other: &Self) -> f32 {
        self.0.distance(other.0)
    }
}

// We'll filter on this simple marker Component being present.
#[derive(Component)]
pub struct DumbMarker;

/// A simple ContextFetcher that returns Entities with a DumbMarker
fn example_context_fetcher(
    // Any CF must handle ContextFetcherInputs, even if they aren't used.
    _inp: ContextFetcherInputs, 
    // You can add any number of Queries, Resources, etc. here as long as they're read-only-compatible.
    context_data_qry: Query<Entity, With<DumbMarker>>,
) 
// Any CF must output this type (it's some flavor of an array of Entity like Vec<Entity>, 
// you generally should be able to let the library worry about it by using a `.collect()`).
-> ContextFetcherOutputs {
    context_data_qry.iter().collect()
}

fn example_consideration(
    // Any Consideration must handle ConsiderationInputs, even if they aren't used.
    inputs: types::ConsiderationInputs,
    // You can add any number of Queries, Resources, etc. here as long as they're read-only-compatible.
    // In this case, we'll only query for Positions.
    qry: Query<&Position2d>
) 
// Any CF must output this type - this is just an Option<f32>, 
// where None indicates the data provided is invalid in some way and Some(value) is later 
// normalized between the Min and Max values of the Consideration to make it 'comparable'.
-> ConsiderationOutputs {
    // These three values (AI Entity, optionally its Pawn Entity, and the Context Entity) are
    // the standard Consideration inputs provided to every registered System by the runtime. 
    let (ai, maybe_pawn, targ) = inputs.0;

    // Validate the inputs. Instead of panicking, we'll return None.
    // This means the runtime will recognize this Context was bad and ignore it, 
    // but it can and will continue trying with other candidate Contexts.
    let pawn = match maybe_pawn {
        None => return None,
        Some(p) => p,
    };

    let pawn_pos = match qry.get(pawn) {
        Err(_) => return None,
        Ok(pos) => pos,
    };

    let targ_pos = match qry.get(targ) {
        Err(_) => return None,
        Ok(pos) => pos,
    };

    // Calculate the actual raw score:
    let val = pawn_pos.euclid_distance(targ_pos);

    // We'll use .into() to ensure that we can ignore simple output interface changes.
    val.into()
}

/// This is an example event that lives in *your game code* (not in Cranium) and handles movement triggers.
#[derive(Event)]
struct MoveTo(Entity, Entity);

/// This is a (very crude) movement Action implementation that lives in *your game code* (no offense!), 
/// not in Cranium - Cranium likely does not even know it exists at all.
/// For each MoveTo event, moves some Entity (MoveTo.0) one step towards the target Entity (MoveTo.1).
/// To keep things simple, we are assuming both Entities actually exist and have `Position2d`s.
fn user_movement_observer(event: On<MoveTo>, mut pos_qry: Query<&Position2d>) {
    let pawn = event.0;
    let target = event.1;

    let mut pawn_pos = pos_qry.get_mut(pawn).unwrap().0;
    let target_pos = pos_qry.get(target).unwrap().0;

    pawn_pos.move_towards(target_pos, 1.);
}

/// ActionHandlers bridge the gap between Cranium and your own game logic by using Commands 
/// to raise Events, write Messages, spawn Entities, or whatever else your game responds to. 
fn example_action_handler(inputs: ActionHandlerInputs, mut commands: Commands) {
    // ActionHandlers always receive the same, standard parameters - they are functions, not Systems!
    let (ai, pawn, ctx) = inputs;
    // We'll build a MoveTo event and trigger it, which will itself trigger a `user_movement_observer()`.
    commands.trigger(MoveTo(pawn.unwrap(), ctx));
}

// Putting it all together. You can easily port this example to a Plugin impl instead to wrap
// your AI integration into a nice, portable bundle you can drop into your real `main()`.
fn main() {
    let mut app = App::new();

    app
        .add_plugins(CraniumPlugin)
        // Registering a ContextFetcher is as simple as passing it in with a key. 
        // 
        // 'Namespacing' keys (here with `mycode::`) is not necessary, but it is 
        // recommended to reduce the risk of key collisions.
        .register_context_fetcher(example_context_fetcher, "mycode::example_cf")
        // Registering Considerations follows the same general pattern as for CFs. 
        // 
        // Note that the key we are supplying can have absolutely nothing to do 
        // with the actual name of the function, and that we could register 
        // many different keys mapped to the same implementation!
        .register_consideration(example_consideration, "mycode::distance2d")
        .register_action_handler(example_action_handler, "mycode::move_to")
    ;

    app.run();
}
```


## Architecture

The core `Cranium` engine largely follows the excellent `Infinite Axis Utility System` architecture as 
outlined in the classic GDC 2015 talk ["Building a Better Centaur: AI at Massive Scale"](https://gdcvault.com/play/1021848/Building-a-Better-Centaur-AI) by Mike Lewis and Dave Mark.

The basic idea is very simple: 
1) The AI (`AIController`) gathers all actions it can possibly take.
2) The AI calculates a normalized score for every candidate `Action`, fuzzy-logic style.
3) The AI simply picks the highest-scoring candidate.
4) The `Action` gets dispatched to the AI-controlled entity (a `Pawn`) for execution.

The `Glossary` provides a quick run-through of the process, with terms
introduced roughly in the sequence in which they come up during the decision loop.

Of course, the devil is in the details. There are a couple of design decisions worth mentioning.

#### Data-driven

Recompiling is famously (especially in Rust) a great excuse to go get coffee, 
but it can quickly get tedious whenever you're taming an unruly AI. 

While reading the README of a Rust AI library suggests you're probably happy messing with 
code, a good number of AI designers, modders, and tooling developers would also be happier if
they didn't have to make code changes, and could instead tweak something in JSON or whatever.

`Cranium` is built around treating AI definitions as Assets to be loaded from files 
and mapped to an implementation at runtime using nice, human-readable string keys.

You CAN hardcode things instead of using data assets if you are concerned about exposing things 
to savvier users for whatever reason; a hybrid approach is also feasible.

#### SmartObjects-oriented

A key idea used to make agent capabilities manageable is **Smart Objects**, 
famously used across "The Sims" series as far back as 2000.

This is a decision borne out of the experiences of working with AIs retrofitted to operate 
in extremely semantically rich game-worlds - what do you do when even the *floor* can be 
interacted with given the right tools and you don't have infinite time to write all behaviors? 

By putting the definitions in world objects rather than in the AI and allowing AIs to 'gather' 
those from the game-world in appropriate contexts, we get all the features of traditional 
'dump it all in the NPC' approach where needed, plus a whole lot of flexibility and moddability 
for scenarios where the simple approach wouldn't cut it. 

#### AI-as-an-Engine

`Cranium` could not possibly provide support for every genre and use-case out of the box.

Even with open-source contributions, waiting for a release to support the use-cases 
that you need to build your applications would not be a good experience for library users.

However, the core concepts of the library are open for third-party code to extend by
registering implementations of Actions, Curves, Considerations, ContextFetchers, and more!

This covers both the game-specific logic you bring to the table when building with `Cranium`, 
but also leaves the door open for sub-libraries that provide useful AI tools and templates 
for a specific genre.


## Getting started

The two main patterns of using `Cranium` are: 
- **Native** - as a native part of your Bevy Engine applications.
- **AI Server** - as a separate "AI World" for non-Bevy and/or non-Rust applications.

The two approaches are more similar than they might appear. 

In both cases, we're running the same core runtime in an ECS World. 

The only difference is whether `Cranium` has direct access to your application data, and 
whether the AI loop is part of your application loop or 'manually driven' by your apps.

For the **Native** integration, `Cranium` provides a configurable Bevy Plugin which handles 
the majority of the basic setup for you - all you need to do is import it, add it to your 
app, and register any custom types for the library to use on your behalf in AI code.

For the **AI Server** integration, `Cranium` provides an API that lets you set up and drive 
a Bevy ECS World for the AI to operate in, and methods to update this world with data from 
your own application.

In either case, you will nearly always need to tailor four things to your needs:

1) ContextFetchers (registered with `app.register_context_fetcher(func, key)`)
2) Considerations (registered with `app.register_consideration(func, key)`)
3) ActionHandlers (registered with `app.register_action_handler(func, key)`)
4) Actions proper (not registered; wire them up to whatever you do in ActionHandlers).


#### ContextFetchers 

`ContextFetchers` are Bevy Systems - for non-Bevy users, this means they are mostly straightforward 
Rust functions, except for being able to make Queries (fast lookups of Entities and their Components 
in the ECS World `Cranium` is running in).

`ContextFetchers` receive some pre-defined inputs (normal function parameters whose types are 
wrapped in Bevy's `In<T>` wrappers) to provide the necessary metadata to power your Queries.

`ContextFetchers` should be *strictly read-only* and should return 
lists of Entities that represent possible targets for Actions. 

For example, to eat an apple, a `ContextFetcher` for the `Eat` Action should return every 
Entity that has an IsApple Component (possibly within some radius, or un-owned, or any other 
extra logic you want the AI to account for - up to you).

You can think of a `ContextFetcher` as a simple **filter** - they trim down all the Entities 
in the World to just those that make sense for the Actions they work for.

Once you're done building your `ContextFetchers`, you can register them to the World easily 
using either the classic App Builder-style `app.register_context_fetcher(func, key)` or on 
the World directly using `world.register_context_fetcher(func, key)`. 

Rust's type system will stop you from registering a `ContextFetcher` if there is anything 
wrong with the function you have built. 


#### Considerations 

`Considerations` are, like `ContextFetchers`, also Bevy Systems.
They also take predefined inputs to help you build Queries in them 
(including a Context returned from the `ContextFetcher`).

`Considerations` should look up some quantifiable data about the world and return it 
as a floating-point number. 

For example, if the `Context` is an enemy unit, we may return its Health, or its 
Distance to our Pawn, and do it as either raw numbers, or as percentage, or 
whatever - as long as it's a floating-point number, it's valid. 

Once you're done building your Consideration(s), you can register them easily using 
either the classic App Builder-style `app.register_consideration(func, key)` or on 
the World directly using `world.register_consideration(func, key)`. 

Rust's type system will stop you from registering a Consideration if there is anything 
wrong with the function you have built. 


#### Actions & ActionHandlers

Deciding what's the best `Action` is well and good, but pretty pointless on its own. 
Cranium needs to have a way of telling the application it's working for what to *do*.

Cranium cannot - and should not - mandate an implementation style for *your* code. 
Instead, we use a translation layer of `ActionHandlers`.

An `ActionHandler` is a function - ANY function - that takes a set of standard inputs 
as defined by Cranium (basic helpful details - AI, Pawn & Context), plus `mut Commands`.

`ActionHandlers` will be triggered by Cranium at appropriate times and are expected to 
use the Commands to raise Events, write Messages, spawn Entities, or whatever else it 
is that you've wired your own game logic to respond to to trigger Stuff Happening.

`ActionHandlers` are registered into Cranium (much like Considerations/CFs/etc.) using an associated Key. 
This should correspond to the `ActionKey` field of *at least one* `ActionTemplate`. 

If that `ActionTemplate` gets picked, Cranium will trigger the corresponding ActionHandler, which 
should then trigger whichever game-logic implementation you have for that ActionTemplate.

For example, if an `ActionTemplate` with an `ActionKey` of `"mycode::Move"` gets picked, 
Cranium will look for an ActionHandler registered with `"mycode::Move"` as the Key. 

Suppose it finds a `move_action_handler(...)` - which builds and triggers a custom Event defined in 
your own game code, `MovementRequest(Entity, Entity)` that you have wired up to your own Observers, 
who will set up some NPC to start moving towards a target location. Job done! 

Cranium does not really care about all that - once the `ActionHandler` has been triggered, 
the library's job is done until the time comes to select another Action for execution. 
Your game code is entirely within your power, as long as you use normal Bevy machinery to run it.

Once you're done building your ActionHandler(s), you can register them easily using 
either the classic App Builder-style `app.register_action_handler(func, key)` or on 
the World directly using `world.register_action_handler(func, key)`. 

Rust's type system will stop you from registering an ActionHandler if there is anything 
wrong with the function you have built. 
