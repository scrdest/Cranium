/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

use core::marker::PhantomData;
use core::time::Duration;

use cranium_core::bevy::asset::{AssetLoader, LoadContext, io::Reader};
use cranium_core::bevy::prelude::*;

use cranium_core::actionset::{ActionSet};
use cranium_core::types::CraniumKvMap;


pub trait ActionSetLoaderBackend: Send + Sync + TypePath + 'static {
    /// What type does the loader return as a loader on error. 
    type Error: core::error::Error + Send + Sync + 'static;

    /// Must be able to load from a byte array.
    fn from_slice<'a>(v: &'a [u8]) -> core::result::Result<ActionSet, Self::Error>;

    /// What extensions should be read for this (by default)?
    fn extensions() -> &'static [&'static str] {
        &[]
    }
}

#[cfg(any(feature = "json_support", test))]
pub mod json_support {
    use super::{ActionSetLoaderBackend, ActionSet};

    #[derive(Default, bevy_reflect::Reflect)]
    pub struct JsonActionSetLoader;

    impl ActionSetLoaderBackend for JsonActionSetLoader {
        type Error = serde_json::Error;

        fn from_slice<'a>(v: &'a [u8]) -> core::result::Result<ActionSet, Self::Error> {
            serde_json::from_slice(&v)
        }

        fn extensions() -> &'static [&'static str] {
            &["json"]
        }
    }
}


#[cfg(any(feature = "toml_support"))]
pub mod toml_support {
    use super::{ActionSetLoaderBackend, ActionSet};

    #[derive(Default, bevy_reflect::Reflect)]
    pub struct TomlActionSetLoader;

    impl ActionSetLoaderBackend for TomlActionSetLoader {
        type Error = toml::de::Error;

        fn from_slice<'a>(v: &'a [u8]) -> core::result::Result<ActionSet, Self::Error> {
            toml::from_slice(&v)
        }

        fn extensions() -> &'static [&'static str] {
            &["toml"]
        }
    }
}


#[cfg(any(feature = "msgpack_support"))]
pub mod msgpack_support {
    use super::{ActionSetLoaderBackend, ActionSet};

    #[derive(Default, bevy_reflect::Reflect)]
    pub struct MsgpackActionSetLoader;

    impl ActionSetLoaderBackend for MsgpackActionSetLoader {
        type Error = rmp_serde::decode::Error;

        fn from_slice<'a>(v: &'a [u8]) -> core::result::Result<ActionSet, Self::Error> {
            rmp_serde::decode::from_slice(v)
        }

        fn extensions() -> &'static [&'static str] {
            &["msgpack"]
        }
    }
}


#[cfg(any(all(feature = "cbor_support", feature = "std", not(feature = "nostd_support"))))]
pub mod cbor_support {
    use super::{ActionSetLoaderBackend, ActionSet};

    #[derive(Default, bevy_reflect::Reflect)]
    pub struct CborActionSetLoader;

    impl ActionSetLoaderBackend for CborActionSetLoader {
        type Error = ciborium::de::Error<std::io::Error>;

        fn from_slice<'a>(v: &'a [u8]) -> core::result::Result<ActionSet, Self::Error> {
            ciborium::de::from_reader(v)
        }

        fn extensions() -> &'static [&'static str] {
            &[".cbor"]
        }
    }
}


#[cfg(any(feature = "ron_support", test))]
pub mod ron_support {
    use super::{ActionSetLoaderBackend, ActionSet};

    #[derive(Default, bevy_reflect::Reflect)]
    pub struct RonActionSetLoader;

    impl ActionSetLoaderBackend for RonActionSetLoader {
        type Error = ron::de::SpannedError;

        fn from_slice<'a>(v: &'a [u8]) -> core::result::Result<ActionSet, Self::Error> {
            ron::de::from_bytes(v)
        }

        fn extensions() -> &'static [&'static str] {
            &["ron"]
        }
    }
}


#[cfg(any(feature = "yaml_support", test))]
pub mod yaml_support {
    use super::{ActionSetLoaderBackend, ActionSet};

    #[derive(Default, bevy_reflect::Reflect)]
    pub struct YamlActionSetLoader;

    impl ActionSetLoaderBackend for YamlActionSetLoader {
        type Error = serde_saphyr::Error;

        fn from_slice<'a>(v: &'a [u8]) -> core::result::Result<ActionSet, Self::Error> {
            serde_saphyr::from_slice(v)
        }

        fn extensions() -> &'static [&'static str] {
            &["yaml", "yml"]
        }
    }
}


#[cfg(any(feature = "postcard_support"))]
pub mod postcard_support {
    use super::{ActionSetLoaderBackend, ActionSet};

    #[derive(Default, bevy_reflect::Reflect)]
    pub struct PostcardActionSetLoader;

    impl ActionSetLoaderBackend for PostcardActionSetLoader {
        type Error = postcard::Error;

        fn from_slice<'a>(v: &'a [u8]) -> core::result::Result<ActionSet, Self::Error> {
            postcard::from_bytes(v)
        }

        fn extensions() -> &'static [&'static str] {
            &["postcard"]
        }
    }
}


// Asset loader
#[derive(Default, bevy_reflect::Reflect)]
pub struct ActionSetLoader<B: ActionSetLoaderBackend>(PhantomData<B>);

impl<B: ActionSetLoaderBackend> ActionSetLoader<B> {
    fn from_slice<'a>(v: &'a [u8]) -> core::result::Result<ActionSet, B::Error> {
        B::from_slice(v)
    }
}

impl<B: ActionSetLoaderBackend> AssetLoader for ActionSetLoader<B> {
    type Asset = ActionSet;
    type Settings = ();
    type Error = Box<dyn core::error::Error + Send + Sync + 'static>;

    async fn load(
        &self, 
        reader: &mut dyn Reader, 
        _settings: &Self::Settings, 
        _ctx: &mut LoadContext<'_>
    ) -> Result<Self::Asset, Self::Error> {
        #[cfg(feature = "logging")]
        cranium_core::bevy::log::debug!("ActionSetLoader running...");
        let mut bytes = cranium_core::types::CraniumList::new();
        let _ = reader.read_to_end(&mut bytes).await;
        let read = Self::from_slice(&bytes);
        let res: Result<ActionSet, Box<dyn core::error::Error + Send + Sync + 'static>> = read.map_err(|err| { 
            #[cfg(feature = "logging")]
            cranium_core::bevy::log::error!("ActionSetLoader error: {:?}", err); 
            err.into() 
        } );
        #[cfg(feature = "logging")]
        cranium_core::bevy::log::debug!("ActionSetLoader finished...");
        res
    }

    fn extensions(&self) -> &[&str] {
        B::extensions()
    }
}

#[derive(Resource, Default)]
struct ActionSetHandles(pub CraniumKvMap<String, Handle<ActionSet>>);


#[derive(Resource, Default)]
struct AssetLoadTimeouts(pub CraniumKvMap<String, Timer>);


#[derive(Event, Debug)]
pub struct LoadActionSetRequest {
    filename: String
}

impl LoadActionSetRequest {
    pub fn new<IS: Into<String>>(filename: IS) -> Self {
        Self {
            filename: filename.into()
        }
    }
}

#[derive(Event, Debug)]
pub struct ActionSetLoaded {
    pub filename: String,
    pub asset_handle: Handle<ActionSet>,
}

#[derive(Event, Debug)]
pub struct ActionSetLoadingTimeout {
    pub filename: String,
    pub timeout_time: f32,
}

fn load_asset(
    event: On<LoadActionSetRequest>,
    asset_server: Res<AssetServer>,
    mut handles: ResMut<ActionSetHandles>,
    mut timer: ResMut<AssetLoadTimeouts>,
) {
    let asset_path = event.event().filename.to_owned();
    #[cfg(feature = "logging")]
    cranium_core::bevy::log::info!("Reading ActionSet from {}...", &asset_path);
    let handle: Handle<ActionSet> = asset_server.load(asset_path.to_owned());
    handles.0.entry(asset_path.to_owned()).or_insert(handle);
    timer.0.insert(asset_path.to_owned(), Timer::new(Duration::from_secs(2), TimerMode::Once));
}

fn countdown(
    time: Res<Time>,
    handles: Res<ActionSetHandles>,
    assets: Res<Assets<ActionSet>>,
    mut timers: ResMut<AssetLoadTimeouts>,
    mut commands: Commands,
) {
    timers.0.iter_mut().for_each(|(key, timer)| {
        if timer.is_finished() {
            let handle = handles.0.get(key);
            let asset = handle
                .map(|handle| assets.get(handle))
                .flatten()
            ;
            
            match asset {
                Some(_loaded_data) => {
                    #[cfg(feature = "logging")]
                    cranium_core::bevy::log::info!("Successfully loaded ActionSet from file {:?}...", key);
                    let notification = ActionSetLoaded {
                        filename: key.to_owned(),
                        asset_handle: handle.unwrap().to_owned(),
                    };
                    commands.trigger(notification);
                },
                None => {
                    let elapsed_time = timer.elapsed_secs();
                    #[cfg(feature = "logging")]
                    cranium_core::bevy::log::warn!(
                        "Loading ActionSet data from file {:?} timed out after {:?}s!", 
                        key, elapsed_time
                    );
                    let notification = ActionSetLoadingTimeout {
                        filename: key.to_owned(),
                        timeout_time: elapsed_time,
                    };
                    commands.trigger(notification);
                },
            };
        }
        else {
            timer.tick(time.delta());
        }
    });
}


fn cleanup_timers_for_loaded_actionsets(
    event: On<ActionSetLoaded>,
    mut timers: ResMut<AssetLoadTimeouts>,
) {
    let evt = event.event();
    timers.0.remove(&evt.filename);
}


#[derive(Default, bevy_reflect::Reflect)]
pub struct ActionSetAssetPlugin<B: ActionSetLoaderBackend>(PhantomData<B>);


impl<B: ActionSetLoaderBackend + Default> cranium_core::bevy::app::Plugin for ActionSetAssetPlugin<B> {
    fn build(&self, app: &mut cranium_core::bevy::app::App) {
        app
        .add_plugins(AssetPlugin::default())
        .init_resource::<ActionSetHandles>()
        .init_asset::<ActionSet>()
        .init_asset_loader::<ActionSetLoader<B>>()
        .init_resource::<AssetLoadTimeouts>()
        .add_observer(load_asset)
        .add_observer(cleanup_timers_for_loaded_actionsets)
        .add_systems(First, countdown)
        ;
    }
}

#[cfg(test)]
mod tests {
    use cranium_core::bevy::{app::ScheduleRunnerPlugin, prelude::*};
    use cranium_core::bevy::asset::{io::AssetSourceBuilder};
    use crate::ron_support::RonActionSetLoader;
    use crate::yaml_support::YamlActionSetLoader;

    use super::*;
    use super::json_support::JsonActionSetLoader;

    #[derive(Resource, Debug)]
    struct TestAssetFilepath(String);

    fn load_test_asset(
        src_path_res: Res<TestAssetFilepath>,
        mut commands: Commands,
    ) {
        let request = LoadActionSetRequest {
            filename: src_path_res.0.to_owned()
        };
        commands.trigger(request);
    }

    fn succeed_on_loaded(
        trigger: On<ActionSetLoaded>,
        mut exit: MessageWriter<AppExit>,
    ) {
        let _evt = trigger.event();
        #[cfg(feature = "logging")]
        cranium_core::bevy::log::info!("ActionSet loaded successfully from {:?} as {:?}", _evt.filename, _evt.asset_handle);
        exit.write(AppExit::Success);
    }

    fn fail_on_timeout(
        trigger: On<ActionSetLoadingTimeout>,
        mut exit: MessageWriter<AppExit>,
    ) {
        let _evt = trigger.event();
        #[cfg(feature = "logging")]
        cranium_core::bevy::log::error!("ActionSet loading from {:?} timed out after {:?}s", _evt.filename, _evt.timeout_time);
        assert!(false);
        exit.write(AppExit::Success);
    }

    /// An abstraction over the common bits of each format's test code. 
    fn run_loader_test<B: ActionSetLoaderBackend + Default>(src_path: &str) {
        let asloader: ActionSetAssetPlugin<B> = Default::default();
        let mut app = App::new();
        app
        .register_asset_source(
            "test_assets", 
            AssetSourceBuilder::platform_default(
                "test_assets", 
                None,
            )
        )
        .insert_resource(TestAssetFilepath(src_path.to_string()))
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(Duration::from_millis(200))),
            #[cfg(feature = "logging")]
            cranium_core::bevy::log::LogPlugin { 
                level: cranium_core::bevy::log::Level::DEBUG, 
                custom_layer: |_| None, 
                filter: "wgpu=error,bevy_render=info,bevy_ecs=info".to_string(),
                fmt_layer: |_| None,
            },
            asloader,
            // DefaultPlugins
        ))
        .add_systems(Startup, load_test_asset)
        .add_observer(succeed_on_loaded)
        .add_observer(fail_on_timeout)
        .run();
    }

    #[test]
    fn test_load_json() {
        run_loader_test::<JsonActionSetLoader>("test_assets://simpleagent.json");
    }

    #[test]
    fn test_load_ron() {
        run_loader_test::<RonActionSetLoader>("test_assets://simpleagent.ron");
    }

    #[test]
    fn test_load_yaml() {
        run_loader_test::<YamlActionSetLoader>("test_assets://simpleagent.yaml");
    }
}
