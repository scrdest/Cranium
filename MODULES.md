# Cranium AI: Sub-Crate Summary

### `cranium` (Root/Umbrella Crate)

The top-level workspace crate that serves as the primary public entry point for users. It re-exports the full contents of `cranium-core` and, depending on enabled feature flags, conditionally re-exports the Bevy plugin, test plugin, ActionSet loader, FFI types, and AI server API. It provides a `prelude` module consolidating all commonly-needed imports, and defines the feature-flag matrix (e.g. `bevy_plugin`, `ai_server`, `actionset_loader`, `testing`, `json_support`, `yaml_support`, etc.) that controls which sub-crates are compiled in. It also houses the `examples/` directory containing the end-to-end (`e2e`) and `ai_server` examples.

### `cranium-core`

The heart of the library. Contains all fundamental AI engine code:

- **Actions & ActionTemplates** — Defining candidate behaviors (abstract templates paired with runtime contexts), and `ActionHandler` registration/dispatch for executing chosen actions.
- **Considerations** — Registerable Bevy Systems that score ActionTemplate+Context pairs by evaluating world state and returning a normalized float, modulated by Utility Curves.
- **ContextFetchers** — Registerable Bevy Systems that find candidate contexts (targets/inputs) for ActionTemplates.
- **Curves** — A curated library of Utility Curves (constant, binary, linear, logistic, etc.) plus combinators (`SoftLeak`, `HardLeak`, `UtilityCurveSampler`) and a registry for custom curves.
- **Decision Loop** — The core engine logic that orchestrates the full Utility AI pipeline: gathering available ActionTemplates from SmartObjects, requesting contexts via ContextFetchers, scoring via Considerations, applying curve adjustments and consideration compensation, selecting the highest-scoring action, and dispatching it to the appropriate `ActionHandler`.
- **SmartObjects** — The Sims-inspired pattern where world objects expose `ActionSets` to AIs, stored in a central `ActionSetStore` resource keyed by name.
- **LODs (Levels of Detail)** — A performance optimization system allowing AI processing frequency/depth to be reduced for distant or low-priority entities.
- **Events** — The event types (`AiDecisionRequested`, `AiDecisionInitiated`, `AiActionPicked`, `AiActionDispatchToUserCode`, etc.) that drive the event-reactive decision pipeline.
- **Types & Identifiers** — Shared type aliases (`CraniumRwLock`, `CraniumList`, `CraniumKvMap`, `ThreadSafeRef`, etc.) and newtype identifiers for context fetchers, considerations, and curves.
- **Pawn** — A component linking an `AIController` entity to the actual game entity it drives.
- **Entity Identifier** — A unified identifier type for entities.
- **Thread-Safe Wrapper** — `ThreadSafeRef<T>`, an `Arc<T>`-based abstraction for cheap cloning and dynamic dispatch in possibly-parallel scenarios.

All core modules are `#![no_std]` with `alloc`, making the engine usable in constrained environments.

### `cranium-bevy-plugin`

A Bevy `Plugin` (`CraniumPlugin`) that streamlines "Native AI" integration — where Cranium runs directly inside a Bevy application with access to the game's ECS World. It handles the boilerplate of registering the core Resources, Observers, and Systems that form the AI framework, so users only need to add the plugin and register their custom ContextFetchers, Considerations, ActionHandlers, and Curves.

### `cranium-actionset-loader`

A crate extending Cranium with the ability to load `ActionSets` from serialized data (JSON, YAML, TOML, RON, CBOR, MessagePack, Postcard — depending on enabled features) via Bevy's `AssetSource` system. This supports in-memory, local filesystem, or web URL sources (platform-dependent). The `ConsiderationData` and `ActionTemplate` structs derive `Serialize`/`Deserialize` when the `actionset_loader` feature is enabled, enabling data-driven AI definition without code changes.

### `cranium-test-plugin`

A first-party Bevy `Plugin` (`CraniumTestPlugin`) designed to standardize testing of the Cranium library itself. It sets up a `MinimalPlugins` environment with a `ScheduleRunnerPlugin` running a 200ms loop, optional logging, observation of action tracker despawns, and an exit-on-finish system for deterministic test termination. Intended for the library's own test suite rather than end-user consumption.

### `cranium-ffi`

A crate providing FFI (Foreign Function Interface) types for Cranium, primarily as a companion to `cranium-api`. It compiles as both a `lib` and `cdylib`, enabling non-Rust applications (or Rust applications that want to keep the AI in a separate dynamic library) to interact with Cranium's type system. It depends on `cranium-core` and `cranium-bevy-plugin`, and includes `serde`/`serde_json` for serialization support.

### `cranium-api`

A crate providing tooling to run Cranium as a standalone "AI Server" for non-Bevy applications, or for Bevy applications that want to keep their game World separate from the AI World. It includes:

- **API** — Functions to create and manage an ECS World for the AI, drive it tick-by-tick externally, and query/update AI decisions.
- **Channels** — Crossbeam-based communication channels for sending data to and receiving decisions from the AI server.
- **Heartbeat** — A mechanism for monitoring AI server liveness.
- **Spawn** — Utilities for spawning AI entities in the server World.

It compiles as both a `lib` and `cdylib` and depends on `cranium-core`, `cranium-bevy-plugin`, `cranium-ffi`, and `crossbeam-channel`. This is the primary integration point for non-Rust hosts (e.g. a C++ game engine) or non-Bevy Rust applications.

---

## Integration Modes Summary

| Mode | Primary Crate | Use Case |
|------|--------------|----------|
| **Native AI** | `cranium-bevy-plugin` | Bevy apps where AI runs in the same World |
| **AI Server** | `cranium-api` + `cranium-ffi` | Non-Bevy or non-Rust apps, or Bevy apps wanting World separation |

Both modes run the same `cranium-core` engine; the difference is data access and loop control.
