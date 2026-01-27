use std::sync::Arc;

use crate::Extractor;
use crate::policy::PolicyConfig;
use crate::predicate::Predicate;

pub type BoxPredicate<R> = Box<dyn Predicate<Subject = R> + Send + Sync>;
pub type BoxExtractor<Req> = Box<dyn Extractor<Subject = Req> + Send + Sync>;

/// Trait for cache configuration.
///
/// Provides predicates for determining cacheability, extractors for generating
/// cache keys, and policy for TTL/staleness behavior.
pub trait CacheConfig<Req, Res> {
    type RequestPredicate: Predicate<Subject = Req> + Send + Sync + 'static;
    type ResponsePredicate: Predicate<Subject = Res> + Send + Sync + 'static;
    type Extractor: Extractor<Subject = Req> + Send + Sync + 'static;

    fn request_predicates(&self) -> Self::RequestPredicate;
    fn response_predicates(&self) -> Self::ResponsePredicate;
    fn extractors(&self) -> Self::Extractor;
    fn policy(&self) -> &PolicyConfig;
}

/// Generic cache configuration.
///
/// A protocol-agnostic configuration that holds predicates, extractors, and policy.
/// Use this with any protocol (HTTP, gRPC, etc.) by providing appropriate
/// predicates and extractors.
///
/// # Example
///
/// ```ignore
/// use hitbox::Config;
/// use hitbox::policy::PolicyConfig;
/// use hitbox_http::predicates::request::Method;
/// use hitbox_http::extractors::Method as MethodExtractor;
/// use std::time::Duration;
///
/// let config = Config::builder()
///     .request_predicate(Method::get())
///     .extractor(MethodExtractor::new().path("/api/*"))
///     .policy(PolicyConfig::builder().ttl(Duration::from_secs(60)).build())
///     .build();
/// ```
pub struct Config<ReqPred, ResPred, Ext> {
    request_predicate: Arc<ReqPred>,
    response_predicate: Arc<ResPred>,
    extractor: Arc<Ext>,
    policy: PolicyConfig,
}

impl<ReqPred, ResPred, Ext> Clone for Config<ReqPred, ResPred, Ext> {
    fn clone(&self) -> Self {
        Self {
            request_predicate: Arc::clone(&self.request_predicate),
            response_predicate: Arc::clone(&self.response_predicate),
            extractor: Arc::clone(&self.extractor),
            policy: self.policy.clone(),
        }
    }
}

impl<ReqPred, ResPred, Ext> std::fmt::Debug for Config<ReqPred, ResPred, Ext> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("request_predicate", &"...")
            .field("response_predicate", &"...")
            .field("extractor", &"...")
            .field("policy", &self.policy)
            .finish()
    }
}

impl<Req, Res, ReqPred, ResPred, Ext> CacheConfig<Req, Res> for Config<ReqPred, ResPred, Ext>
where
    Req: Send,
    Res: Send,
    ReqPred: Predicate<Subject = Req> + Send + Sync + 'static,
    ResPred: Predicate<Subject = Res> + Send + Sync + 'static,
    Ext: Extractor<Subject = Req> + Send + Sync + 'static,
{
    type RequestPredicate = Arc<ReqPred>;
    type ResponsePredicate = Arc<ResPred>;
    type Extractor = Arc<Ext>;

    fn request_predicates(&self) -> Self::RequestPredicate {
        Arc::clone(&self.request_predicate)
    }

    fn response_predicates(&self) -> Self::ResponsePredicate {
        Arc::clone(&self.response_predicate)
    }

    fn extractors(&self) -> Self::Extractor {
        Arc::clone(&self.extractor)
    }

    fn policy(&self) -> &PolicyConfig {
        &self.policy
    }
}

/// Builder for [`Config`].
///
/// Use [`Config::builder()`] to create a new builder.
pub struct ConfigBuilder<ReqPred, ResPred, Ext> {
    request_predicate: ReqPred,
    response_predicate: ResPred,
    extractor: Ext,
    policy: PolicyConfig,
}

/// Marker type for unset builder fields.
///
/// This type is used in the typestate pattern for `ConfigBuilder`.
/// When you see `NotSet` in a compiler error, it means you haven't called
/// the corresponding builder method yet.
pub struct NotSet;

impl Config<NotSet, NotSet, NotSet> {
    /// Creates a new [`ConfigBuilder`].
    pub fn builder() -> ConfigBuilder<NotSet, NotSet, NotSet> {
        ConfigBuilder::new()
    }
}

impl ConfigBuilder<NotSet, NotSet, NotSet> {
    /// Creates a new builder with no fields set.
    pub fn new() -> Self {
        Self {
            request_predicate: NotSet,
            response_predicate: NotSet,
            extractor: NotSet,
            policy: PolicyConfig::default(),
        }
    }
}

impl Default for ConfigBuilder<NotSet, NotSet, NotSet> {
    fn default() -> Self {
        Self::new()
    }
}

impl<ReqPred, ResPred, Ext> ConfigBuilder<ReqPred, ResPred, Ext> {
    /// Sets the request predicate.
    pub fn request_predicate<NewReqPred>(
        self,
        predicate: NewReqPred,
    ) -> ConfigBuilder<NewReqPred, ResPred, Ext> {
        ConfigBuilder {
            request_predicate: predicate,
            response_predicate: self.response_predicate,
            extractor: self.extractor,
            policy: self.policy,
        }
    }

    /// Sets the response predicate.
    pub fn response_predicate<NewResPred>(
        self,
        predicate: NewResPred,
    ) -> ConfigBuilder<ReqPred, NewResPred, Ext> {
        ConfigBuilder {
            request_predicate: self.request_predicate,
            response_predicate: predicate,
            extractor: self.extractor,
            policy: self.policy,
        }
    }

    /// Sets the cache key extractor.
    pub fn extractor<NewExt>(self, extractor: NewExt) -> ConfigBuilder<ReqPred, ResPred, NewExt> {
        ConfigBuilder {
            request_predicate: self.request_predicate,
            response_predicate: self.response_predicate,
            extractor,
            policy: self.policy,
        }
    }

    /// Sets the cache policy.
    pub fn policy(self, policy: PolicyConfig) -> Self {
        Self { policy, ..self }
    }
}

impl<ReqPred, ResPred, Ext> ConfigBuilder<ReqPred, ResPred, Ext>
where
    ReqPred: Predicate + Send + Sync + 'static,
    ResPred: Predicate + Send + Sync + 'static,
    Ext: Extractor + Send + Sync + 'static,
{
    /// Builds the [`Config`].
    ///
    /// All fields (request_predicate, response_predicate, extractor) must be set
    /// before calling this method.
    pub fn build(self) -> Config<ReqPred, ResPred, Ext> {
        Config {
            request_predicate: Arc::new(self.request_predicate),
            response_predicate: Arc::new(self.response_predicate),
            extractor: Arc::new(self.extractor),
            policy: self.policy,
        }
    }
}

impl<T, Req, Res> CacheConfig<Req, Res> for Arc<T>
where
    T: CacheConfig<Req, Res>,
{
    type RequestPredicate = T::RequestPredicate;
    type ResponsePredicate = T::ResponsePredicate;
    type Extractor = T::Extractor;

    fn request_predicates(&self) -> Self::RequestPredicate {
        self.as_ref().request_predicates()
    }

    fn response_predicates(&self) -> Self::ResponsePredicate {
        self.as_ref().response_predicates()
    }

    fn extractors(&self) -> Self::Extractor {
        self.as_ref().extractors()
    }

    fn policy(&self) -> &PolicyConfig {
        self.as_ref().policy()
    }
}
