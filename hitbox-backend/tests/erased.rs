use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use hitbox_backend::{Backend, BackendResult, CacheBackend, CacheKeyFormat, CompositionBackend};
use hitbox_core::{
    BoxContext, CacheContext, CacheKey, CacheValue, CacheableResponse, EntityPolicyConfig, Raw,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[cfg(feature = "rkyv_format")]
use rkyv::{Archive, Serialize as RkyvSerialize};
#[cfg(feature = "rkyv_format")]
use rkyv_typename::TypeName;

#[derive(Debug)]
struct MemBackend {
    storage: RwLock<HashMap<String, Raw>>,
}

impl MemBackend {
    fn new() -> Self {
        let mut storage = HashMap::new();
        // URL-encoded format: key1=
        storage.insert(
            "key1=".to_owned(),
            bytes::Bytes::from(&b"{\"name\": \"test\", \"index\": 42}"[..]),
        );
        MemBackend {
            storage: RwLock::new(storage),
        }
    }
}

#[async_trait]
impl Backend for MemBackend {
    async fn read(&self, key: &CacheKey) -> BackendResult<Option<CacheValue<Raw>>> {
        let lock = self.storage.read().await;
        let key_str = String::from_utf8(CacheKeyFormat::UrlEncoded.serialize(key)?).unwrap();
        let value = lock.get(&key_str).cloned();
        Ok(value.map(|value| CacheValue::new(value, Some(Utc::now()), Some(Utc::now()))))
    }

    async fn write(
        &self,
        key: &CacheKey,
        value: CacheValue<Raw>,
        _ttl: Option<std::time::Duration>,
    ) -> BackendResult<()> {
        let mut lock = self.storage.write().await;
        let key_str = String::from_utf8(CacheKeyFormat::UrlEncoded.serialize(key)?).unwrap();
        lock.insert(key_str, value.data);
        Ok(())
    }

    async fn remove(&self, _key: &CacheKey) -> BackendResult<hitbox_backend::DeleteStatus> {
        todo!()
    }
}

impl CacheBackend for MemBackend {}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(
    feature = "rkyv_format",
    derive(Archive, RkyvSerialize, rkyv::Deserialize, TypeName)
)]
#[cfg_attr(feature = "rkyv_format", archive(check_bytes))]
#[cfg_attr(feature = "rkyv_format", archive_attr(derive(TypeName)))]
struct Value {
    name: String,
    index: u8,
}

#[async_trait]
impl CacheableResponse for Value {
    type Cached = Self;
    type Subject = Self;

    async fn cache_policy<P>(
        self,
        _predicates: P,
        _: &EntityPolicyConfig,
    ) -> hitbox_core::ResponseCachePolicy<Self>
    where
        P: hitbox_core::Predicate<Subject = Self::Subject> + Send + Sync,
    {
        todo!()
    }

    async fn into_cached(self) -> hitbox_core::CachePolicy<Self::Cached, Self> {
        todo!()
    }

    async fn from_cached(_cached: Self::Cached) -> Self {
        todo!()
    }
}

struct Cache<B> {
    backend: B,
}

impl<B> Cache<B>
where
    B: CacheBackend + Sync,
{
    fn new(backend: B) -> Self {
        Cache { backend }
    }

    async fn test(&self) {
        let value = CacheValue::new(
            Value {
                name: "value3".to_owned(),
                index: 128,
            },
            Some(Utc::now()),
            Some(Utc::now()),
        );
        let mut ctx: BoxContext = CacheContext::default().boxed();
        self.backend
            .set::<Value>(&CacheKey::from_str("key3", ""), &value, None, &mut ctx)
            .await
            .unwrap();
        dbg!(
            self.backend
                .get::<Value>(&CacheKey::from_str("key3", ""), &mut ctx)
                .await
                .unwrap()
        );
    }
}

#[tokio::test]
async fn dyn_backend() {
    let key1 = CacheKey::from_str("key1", "");
    let key2 = CacheKey::from_str("key2", "");
    let storage = MemBackend::new();
    let mut ctx: BoxContext = CacheContext::default().boxed();
    let value = storage.get::<Value>(&key1, &mut ctx).await.unwrap();
    dbg!(value);

    let backend: Box<dyn Backend> = Box::new(storage);
    let value = backend.get::<Value>(&key1, &mut ctx).await.unwrap();
    dbg!(value);

    let value = CacheValue::new(
        Value {
            name: "value2".to_owned(),
            index: 255,
        },
        Some(Utc::now()),
        Some(Utc::now()),
    );
    backend
        .set::<Value>(&key2, &value, None, &mut ctx)
        .await
        .unwrap();
    let value = backend.get::<Value>(&key2, &mut ctx).await.unwrap();
    dbg!(value);

    let cache = Cache::new(backend);
    cache.test().await;

    let cache = Cache::new(MemBackend::new());
    cache.test().await;
}

#[tokio::test]
async fn test_composition_with_boxed_backends() {
    // Create two separate backends
    let l1: Box<dyn Backend> = Box::new(MemBackend::new());
    let l2: Box<dyn Backend> = Box::new(MemBackend::new());

    // Compose them
    let composition = CompositionBackend::new(l1, l2);

    // Test 1: Write through composition - should populate both layers
    let key_both = CacheKey::from_str("key_both", "");
    let value_both = CacheValue::new(
        Value {
            name: "both_layers".to_owned(),
            index: 1,
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );
    let mut ctx: BoxContext = CacheContext::default().boxed();
    composition
        .set::<Value>(
            &key_both,
            &value_both,
            Some(std::time::Duration::from_secs(60)),
            &mut ctx,
        )
        .await
        .unwrap();

    // Read should return the value
    let result = composition.get::<Value>(&key_both, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.name, "both_layers");

    // Test 2: Key that doesn't exist - should return None
    let key_missing = CacheKey::from_str("missing", "");
    let result = composition
        .get::<Value>(&key_missing, &mut ctx)
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_composition_with_arc_backends() {
    use std::sync::Arc;

    // Create two separate backends with Arc
    let l1: Arc<dyn Backend + Send + 'static> = Arc::new(MemBackend::new());
    let l2: Arc<dyn Backend + Send + 'static> = Arc::new(MemBackend::new());

    // Compose them
    let composition = CompositionBackend::new(l1, l2);

    // Write a value
    let key = CacheKey::from_str("arc_key", "");
    let value = CacheValue::new(
        Value {
            name: "arc_value".to_owned(),
            index: 42,
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );
    let mut ctx: BoxContext = CacheContext::default().boxed();
    composition
        .set::<Value>(
            &key,
            &value,
            Some(std::time::Duration::from_secs(60)),
            &mut ctx,
        )
        .await
        .unwrap();

    // Read it back
    let result = composition.get::<Value>(&key, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.name, "arc_value");
}

#[tokio::test]
async fn test_composition_l1_l2_different_keys() {
    // Create L1 and L2 backends
    let l1_mem = MemBackend::new();
    let l2_mem = MemBackend::new();

    let mut ctx: BoxContext = CacheContext::default().boxed();

    // Populate L1 with key1
    let key1 = CacheKey::from_str("key1", "");
    let value1 = CacheValue::new(
        Value {
            name: "l1_only".to_owned(),
            index: 10,
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );
    l1_mem
        .set::<Value>(
            &key1,
            &value1,
            Some(std::time::Duration::from_secs(60)),
            &mut ctx,
        )
        .await
        .unwrap();

    // Populate L2 with key2
    let key2 = CacheKey::from_str("key2", "");
    let value2 = CacheValue::new(
        Value {
            name: "l2_only".to_owned(),
            index: 20,
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );
    l2_mem
        .set::<Value>(
            &key2,
            &value2,
            Some(std::time::Duration::from_secs(60)),
            &mut ctx,
        )
        .await
        .unwrap();

    // Create composition with trait objects
    let l1: Box<dyn Backend> = Box::new(l1_mem);
    let l2: Box<dyn Backend> = Box::new(l2_mem);
    let composition = CompositionBackend::new(l1, l2);

    // Test 1: Read key1 - should hit L1
    let result = composition.get::<Value>(&key1, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.name, "l1_only");

    // Test 2: Read key2 - should miss L1, hit L2
    let result = composition.get::<Value>(&key2, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.name, "l2_only");

    // Test 3: Read key3 - should miss both
    let key3 = CacheKey::from_str("key3", "");
    let result = composition.get::<Value>(&key3, &mut ctx).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_composition_backend_as_trait_object() {
    // Create L1 and L2 backends
    let l1_mem = MemBackend::new();
    let l2_mem = MemBackend::new();

    let mut ctx: BoxContext = CacheContext::default().boxed();

    // Populate L1 with one key
    let key_l1 = CacheKey::from_str("l1_key", "");
    let value_l1 = CacheValue::new(
        Value {
            name: "in_l1".to_owned(),
            index: 11,
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );
    l1_mem
        .set::<Value>(
            &key_l1,
            &value_l1,
            Some(std::time::Duration::from_secs(60)),
            &mut ctx,
        )
        .await
        .unwrap();

    // Populate L2 with another key
    let key_l2 = CacheKey::from_str("l2_key", "");
    let value_l2 = CacheValue::new(
        Value {
            name: "in_l2".to_owned(),
            index: 22,
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );
    l2_mem
        .set::<Value>(
            &key_l2,
            &value_l2,
            Some(std::time::Duration::from_secs(60)),
            &mut ctx,
        )
        .await
        .unwrap();

    // Create composition with trait objects
    let l1: Box<dyn Backend> = Box::new(l1_mem);
    let l2: Box<dyn Backend> = Box::new(l2_mem);
    let composition = CompositionBackend::new(l1, l2);

    // Use composition itself as a trait object
    let backend: Box<dyn Backend> = Box::new(composition);

    // Test 1: Read key from L1 through trait object
    let result = backend.get::<Value>(&key_l1, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.name, "in_l1");

    // Test 2: Read key from L2 through trait object
    let result = backend.get::<Value>(&key_l2, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.name, "in_l2");

    // Test 3: Write new key through trait object - should go to both layers
    let key_new = CacheKey::from_str("new_key", "");
    let value_new = CacheValue::new(
        Value {
            name: "nested_trait".to_owned(),
            index: 99,
        },
        Some(Utc::now() + chrono::Duration::seconds(60)),
        None,
    );
    backend
        .set::<Value>(
            &key_new,
            &value_new,
            Some(std::time::Duration::from_secs(60)),
            &mut ctx,
        )
        .await
        .unwrap();

    // Read back the new key
    let result = backend.get::<Value>(&key_new, &mut ctx).await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().data.name, "nested_trait");

    // Test 4: Missing key should return None
    let key_missing = CacheKey::from_str("not_there", "");
    let result = backend.get::<Value>(&key_missing, &mut ctx).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_composition_with_cache_wrapper() {
    // Test CompositionBackend with the Cache wrapper struct
    let l1: Box<dyn Backend> = Box::new(MemBackend::new());
    let l2: Box<dyn Backend> = Box::new(MemBackend::new());
    let composition = CompositionBackend::new(l1, l2);

    let cache = Cache::new(composition);

    // The Cache::test() method writes and reads a value
    cache.test().await;
}

// #[async_trait]
// impl CacheBackend for DummyBackend {
//     async fn get<T>(
//         &self,
//         key: &hitbox_core::CacheKey,
//     ) -> hitbox_backend::BackendResult<Option<hitbox_core::CachedValue<T::Cached>>>
//     where
//         T: hitbox_core::CacheableResponse,
//         <T as hitbox_core::CacheableResponse>::Cached: serde::de::DeserializeOwned,
//     {
//         todo!()
//     }
//
//     async fn set<T>(
//         &self,
//         key: &hitbox_core::CacheKey,
//         value: &hitbox_core::CachedValue<T::Cached>,
//         ttl: Option<u32>,
//     ) -> hitbox_backend::BackendResult<()>
//     where
//         T: hitbox_core::CacheableResponse + Send,
//         T::Cached: serde::Serialize + Send + Sync,
//     {
//         todo!()
//     }
//
//     async fn delete(
//         &self,
//         key: &hitbox_core::CacheKey,
//     ) -> hitbox_backend::BackendResult<hitbox_backend::DeleteStatus> {
//         todo!()
//     }
//
//     async fn start(&self) -> hitbox_backend::BackendResult<()> {
//         todo!()
//     }
// }
//
// #[derive(Clone, Debug)]
// struct A {}
//
// #[async_trait]
// impl CacheableResponse for A {
//     type Cached = u8;
//
//     type Subject = A;
//
//     async fn cache_policy<P>(self, predicates: P) -> hitbox_core::ResponseCachePolicy<Self>
//     where
//         P: hitbox_core::Predicate<Subject = Self::Subject> + Send + Sync,
//     {
//         todo!()
//     }
//
//     async fn into_cached(self) -> hitbox_core::CachePolicy<Self::Cached, Self> {
//         todo!()
//     }
//
//     async fn from_cached(cached: Self::Cached) -> Self {
//         todo!()
//     }
// }
//
// #[tokio::test]
// async fn test_dyn_backend() {
//     let backend: Box<dyn ErasedBackend> = Box::new(DummyBackend {});
//     let key = CacheKey::from_str("test", "key");
//     let value = CachedValue::new(42, Utc::now());
//     let result = backend.set::<A>(&key, &value, None).await;
//     dbg!(result);
//
//     let result = backend.get::<A>(&key).await;
//     dbg!(result);
// }
