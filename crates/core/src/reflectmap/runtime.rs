//! Contains the ReflectMap stuff that you actually care about during the program's lifetime 
//! (as opposed to persistence, which outlives the program).

use bevy::{platform::collections::HashMap, prelude::*, reflect::{GetTypeRegistration, Type}};

use crate::types::{CraniumCow, ReflectMapKey};


/// A Trait that turns the implementing type into a valid 'type marker' for [`ReflectMap`]s. 
/// 
/// [`ReflectMap`]s use a two-part key - a plain old string key, plus a type marker that identifies 
/// what type the stored Value is underneath it. The latter is exactly what [`ReflectMapTypeMarker`] 
/// represents.
/// 
/// It is not feasible to write a marker for every possible type for every possible library user; 
/// instead, this is a Trait that allows you to declare (fairly anemic) newtypes as use them to 
/// mark up any type that you might want to use in your application code (within reasonable limits).
/// 
/// This is pretty easy, as this is largely pure type-level wizardry. All you need to do is:
/// 
/// 1) Define the type that you want to put in the [`ReflectMap`], say `MyAwesomeType`
/// 2) Create a new fieldless struct with a clear name, like `MyAwesomeTypeMarker`
/// 3) Slap a `[derive(TypePath)]` on top of `MyAwesomeTypeMarker` (from [`bevy::reflect`])
/// 4) `impl ReflectMapTypeMarker for MyAwesomeTypeMarker` with `type Value = MyAwesomeType` 
///     and `const NAME: &'static str = "MyAwesomeTypeMarker"`. Done.
/// 
/// See also [`crate::reflectmap::markers::BasicReflectTypemarkersPlugin`] for a Bevy Plugin 
/// that adds a whole bunch of boilerplate impls of this trait for you for most common types.
pub trait ReflectMapTypeMarker: 'static + TypePath {
    type Value: Reflect + FromReflect + Clone + Send + Sync + GetTypeRegistration + TypePath + 'static;

    /// An identifier for this marker that survives code migrations (unlike the typepath)
    const NAME: &'static str;

    /// Returns True if the provided [`ReflectMapTypeMarker`] can mark instances 
    /// of the provided type `T` in [`ReflectMap`]s.
    fn marks_type<T: TypePath>() -> bool {
        Self::Value::type_path() == T::type_path()
    }

    /// Returns True if the provided [`ReflectMapTypeMarker`] can mark the type
    /// of the provided value in [`ReflectMap`]s.
    fn marks_type_of_value<T: TypePath>(_value: T) -> bool {
        Self::Value::type_path() == T::type_path()
    }
}


#[derive(Default)]
/// A SerDe-able HashMap storing values of heterogenous types, a'la `typedmap`.
/// 
/// 
/// You can think of it as something like an in-memory version of a JSON file, 
/// or a Python dict; it's got the usual O(1) key-value behavior of a HashMap, 
/// but the values stored inside it can be a complete hodgepodge of assorted 
/// Rust types - one key can hold a [`String`], another an [`i8`], a third a [`usize`].
/// 
/// However, unlike those two, [`ReflectMap`] stays entirely within your Rust code 
/// and is fully type-safe - the type that you put in is the type that you get 
/// back from the map.
/// This is achieved by storing the values type-erased and extending the keys 
/// with a type marker that tells the map how to unerase the value type.
/// 
/// 
/// [`ReflectMap`]s use a two-part key - a plain old string key, plus a type marker that identifies 
/// what type the stored Value is underneath it. For example, a key of (AgentId, "enemy") means that 
/// semantically the Value represents an Enemy and that that Enemy will be returned as an AgentId.
/// 
/// As a result, when authoring code like action handlers downstream, you get a hard contract as to 
/// the type provided to you from the map - you declare what you're expecting, and the map will give 
/// you that exact type. If, for whatever reason, the upstream code misbehaves, you will be able to 
/// spot this quickly as the map will refuse to hand out values of any type other than what you expect.
/// 
/// 
/// *Practically speaking*, the gruntwork is mostly done for you - the only thing 
/// you need to worry about that you don't have to deal with in classic HashMaps 
/// is to provide the marker type for the value you're storing/retrieving using 
/// the turbofish (`::<T>`) syntax, and *possibly* defining custom markers (easy!) 
/// in case you want to insert a type that doesn't already have a one pre-defined.
/// 
/// See [`ReflectMapTypeMarker`] for details on registering values.
/// 
/// [`ReflectMap`]s ARE serializable and deserializable (with the `reflectmap_persistence` feature), 
/// but doing so requires a bit of extra bookkeeping the regular SerDe traits cannot express natively.
/// 
/// See [`super::persistence::capture::capture_map`] for a function that can produce a serializable struct 
/// out of this type, and [`super::persistence::rehydrate::rehydrate_map`] for converting the raw deserialized 
/// type back into a [`ReflectMap`].
pub struct ReflectMap {
    pub(crate) entries: HashMap<
        (&'static str, ReflectMapKey),  // Typepath/NAME of the *Key* (depending on your features)
        Box<dyn Reflect>,
    >,
}

impl ReflectMap {
    /// Retrieves a type-erased value for the provided key and type marker. 
    /// 
    /// Returns None if the value is not in the map at all. 
    /// Returns Some(&T) if the value is in the map. 
    /// 
    /// See [`ReflectMap::try_get()`] if you want to get the value with the original type restored, or
    /// [`ReflectMap::get()`] (simpler alternative that gives you slightly less debugging options).
    /// 
    /// See [`ReflectMap::contains()`] if you just want to check if there is SOME value for this key.
    pub fn get_raw<M: ReflectMapTypeMarker>(&self, key: ReflectMapKey) -> Option<&dyn Reflect> {
        let key_id = cfg_select! {
            feature = "reflectmap_keys_by_name_const" => { M::NAME },
            _ => Type::of::<M>().path()
        };
        let key_tup = (key_id, key);
        let raw_result = self.entries.get(&key_tup);
        let derefd = raw_result.map(|b| b.as_ref());
        derefd
    }

    /// Returns true if SOME value is stored for that key/typemarker, without checking what that value is.
    /// 
    /// See [`ReflectMap::try_get()`], [`ReflectMap::get()`], or [`ReflectMap::get_raw()`] if you want to 
    /// actually get the value out of the map.
    pub fn contains<M: ReflectMapTypeMarker>(&self, key: ReflectMapKey) -> bool {
        self.get_raw::<M>(key).is_some()
    }

    /// Retrieves a typed value for the provided key and type marker. 
    /// 
    /// Returns None if the value is not in the map at all. (1)
    /// Returns Some(Err(e)) if the value is in the map, but of unexpected type. (2)
    /// Returns Some(&T) if the value is in the map. (3)
    /// 
    /// See [`ReflectMap::get()`] for a simpler API when you don't care about the distinction between (1) & (2).
    /// 
    /// See [`ReflectMap::get_raw()`] if you don't care about the actual type of the value.
    /// 
    /// See [`ReflectMap::contains()`] if you just want to check if there is SOME value for this key.
    pub fn try_get<M: ReflectMapTypeMarker>(&self, key: ReflectMapKey) -> Option<Result<&M::Value, &'static str>> {
        let raw_result = self.get_raw::<M>(key);
        let result = raw_result.map(|r| r.downcast_ref::<M::Value>().ok_or("invalid type!"));
        result
    }

    /// Retrieves a typed value for the provided key and type marker. 
    /// 
    /// Returns None if the value is not in the map at all.
    /// Returns Some(Err(e)) if the value is in the map, but of unexpected type.
    /// Returns Some(&T) if the value is in the map.
    /// 
    /// See [`ReflectMap::try_get()`] for an API that lets you debug the result in more detail.
    /// 
    /// See [`ReflectMap::contains()`] if you just want to check if there is SOME value for this key.
    /// 
    /// See [`ReflectMap::get_raw()`] if you don't care about the actual type of the value.
    pub fn get<M: ReflectMapTypeMarker>(&self, key: ReflectMapKey) -> Option<&M::Value> {
        let raw_result = self.try_get::<M>(key);
        let result = raw_result.map(|res| res.ok()).flatten();
        result
    }

    /// Inserts the provided value into the ReflectMap. 
    /// 
    /// If the map already held a value for the given type marker and key, 
    /// the old value will be returned as Some<&V>; otherwise the function returns None.
    /// 
    /// See [`ReflectMap::contains()`] if you want to check if there is a value for this key non-destructively.
    /// 
    /// See [`ReflectMap::insert_dyn()`] if you don't know the actual type at insert-time.
    pub fn insert<M: ReflectMapTypeMarker>(&mut self, key: ReflectMapKey, val: M::Value) -> Option<&M::Value> {
        let key_id = cfg_select! {
            feature = "reflectmap_keys_by_name_const" => { M::NAME },
            _ => Type::of::<M>().path()
        };
        let key_tup = (key_id, key);

        let existing = self.entries.insert(
            key_tup.clone(), 
            Box::new(val)
        );

        match existing {
            Some(preex) => {
                #[cfg(feature = "logging")]
                bevy::log::warn!("reflectmap: Overwrote preexisting value for key {:?}!", &key_tup);
                let cast_old = preex.downcast_ref().cloned();
                cast_old
            }
            None => None
        }
    }

    /// Untyped insert, for the persistence path where the marker is resolved
    /// from data rather than from a type parameter.
    /// 
    /// See [`ReflectMap::insert()`] for a standard insert (probably what you want 90% of the time).
    /// 
    /// See [`ReflectMap::contains()`] if you want to check if there is a value for this key non-destructively.
    pub fn insert_dyn(
        &mut self, 
        marker: &'static str, 
        key: CraniumCow<'static, str>, 
        value: Box<dyn Reflect>
    ) {
        self.entries.insert((marker, key), value);
    }

    /// Returns the length of the underlying map. 
    /// See [`HashMap::len()`] for details.
    pub fn len(&self) -> usize { self.entries.len() }

    /// Returns an iterator over items of the underlying map, with type-erased values.
    pub fn iter_raw(&self) -> impl Iterator<Item = (&(&'static str, CraniumCow<'static, str>), &dyn Reflect)> {
        self.entries.iter().map(|(k, v)| (k, v.as_reflect()))
    }
}