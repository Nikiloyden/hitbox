use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use hitbox::{
    CachePolicy, CacheValue, CacheableResponse, EntityPolicyConfig, predicate::PredicateResult,
};
use http::{HeaderMap, Response, response::Parts};
use hyper::body::Body as HttpBody;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

use crate::CacheableSubject;
use crate::body::BufferedBody;
use crate::predicates::header::HasHeaders;

#[derive(Debug)]
pub struct CacheableHttpResponse<ResBody>
where
    ResBody: HttpBody,
{
    pub parts: Parts,
    pub body: BufferedBody<ResBody>,
}

impl<ResBody> CacheableHttpResponse<ResBody>
where
    ResBody: HttpBody,
{
    pub fn from_response(response: Response<BufferedBody<ResBody>>) -> Self {
        let (parts, body) = response.into_parts();
        CacheableHttpResponse { parts, body }
    }

    pub fn into_response(self) -> Response<BufferedBody<ResBody>> {
        Response::from_parts(self.parts, self.body)
    }
}

impl<ResBody> CacheableSubject for CacheableHttpResponse<ResBody>
where
    ResBody: HttpBody,
{
    type Body = ResBody;
    type Parts = Parts;

    fn into_parts(self) -> (Self::Parts, BufferedBody<Self::Body>) {
        (self.parts, self.body)
    }

    fn from_parts(parts: Self::Parts, body: BufferedBody<Self::Body>) -> Self {
        Self { parts, body }
    }
}

impl<ResBody> HasHeaders for CacheableHttpResponse<ResBody>
where
    ResBody: HttpBody,
{
    fn headers(&self) -> &http::HeaderMap {
        &self.parts.headers
    }
}

#[cfg(feature = "rkyv_format")]
mod rkyv_status_code {
    use hitbox::RkyvDeserializeError;
    use http::StatusCode;
    use rkyv::{
        Fallible,
        with::{ArchiveWith, DeserializeWith, SerializeWith},
    };

    pub struct StatusCodeAsU16;

    impl ArchiveWith<StatusCode> for StatusCodeAsU16 {
        type Archived = rkyv::Archived<u16>;
        type Resolver = rkyv::Resolver<u16>;

        unsafe fn resolve_with(
            field: &StatusCode,
            pos: usize,
            resolver: Self::Resolver,
            out: *mut Self::Archived,
        ) {
            let value = field.as_u16();
            // Safety: The caller guarantees that `out` is aligned and points to enough bytes
            unsafe { rkyv::Archive::resolve(&value, pos, resolver, out) };
        }
    }

    impl<S: Fallible + ?Sized> SerializeWith<StatusCode, S> for StatusCodeAsU16
    where
        S: rkyv::ser::ScratchSpace + rkyv::ser::Serializer,
    {
        fn serialize_with(
            field: &StatusCode,
            serializer: &mut S,
        ) -> Result<Self::Resolver, S::Error> {
            rkyv::Serialize::serialize(&field.as_u16(), serializer)
        }
    }

    impl<D: Fallible + ?Sized> DeserializeWith<rkyv::Archived<u16>, StatusCode, D> for StatusCodeAsU16
    where
        D::Error: From<RkyvDeserializeError>,
    {
        fn deserialize_with(
            field: &rkyv::Archived<u16>,
            deserializer: &mut D,
        ) -> Result<StatusCode, D::Error> {
            let value: u16 = rkyv::Deserialize::deserialize(field, deserializer)?;
            StatusCode::from_u16(value).map_err(|e| {
                RkyvDeserializeError::new(format!("invalid status code: {}", e)).into()
            })
        }
    }
}

#[cfg(feature = "rkyv_format")]
mod rkyv_header_map {
    use http::HeaderMap;
    use rkyv::{
        Fallible,
        with::{ArchiveWith, DeserializeWith, SerializeWith},
    };

    pub struct AsHeaderVec;

    impl ArchiveWith<HeaderMap> for AsHeaderVec {
        type Archived = rkyv::Archived<Vec<(String, Vec<u8>)>>;
        type Resolver = rkyv::Resolver<Vec<(String, Vec<u8>)>>;

        unsafe fn resolve_with(
            field: &HeaderMap,
            pos: usize,
            resolver: Self::Resolver,
            out: *mut Self::Archived,
        ) {
            let vec: Vec<(String, Vec<u8>)> = field
                .iter()
                .map(|(name, value)| (name.as_str().to_string(), value.as_bytes().to_vec()))
                .collect();
            unsafe {
                rkyv::Archive::resolve(&vec, pos, resolver, out);
            }
        }
    }

    impl<S: Fallible + ?Sized> SerializeWith<HeaderMap, S> for AsHeaderVec
    where
        S: rkyv::ser::ScratchSpace + rkyv::ser::Serializer,
    {
        fn serialize_with(
            field: &HeaderMap,
            serializer: &mut S,
        ) -> Result<Self::Resolver, S::Error> {
            let vec: Vec<(String, Vec<u8>)> = field
                .iter()
                .map(|(name, value)| (name.as_str().to_string(), value.as_bytes().to_vec()))
                .collect();
            rkyv::Serialize::serialize(&vec, serializer)
        }
    }

    impl<D: Fallible + ?Sized> DeserializeWith<rkyv::Archived<Vec<(String, Vec<u8>)>>, HeaderMap, D>
        for AsHeaderVec
    {
        fn deserialize_with(
            field: &rkyv::Archived<Vec<(String, Vec<u8>)>>,
            deserializer: &mut D,
        ) -> Result<HeaderMap, D::Error> {
            let vec: Vec<(String, Vec<u8>)> = rkyv::Deserialize::deserialize(field, deserializer)?;
            let mut map = HeaderMap::new();
            for (name, value) in vec {
                if let (Ok(header_name), Ok(header_value)) = (
                    http::header::HeaderName::from_bytes(name.as_bytes()),
                    http::header::HeaderValue::from_bytes(&value),
                ) {
                    map.append(header_name, header_value);
                }
            }
            Ok(map)
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(
    feature = "rkyv_format",
    derive(
        rkyv::Archive,
        rkyv::Serialize,
        rkyv::Deserialize,
        rkyv_typename::TypeName
    )
)]
#[cfg_attr(feature = "rkyv_format", archive(check_bytes))]
#[cfg_attr(feature = "rkyv_format", archive_attr(derive(rkyv_typename::TypeName)))]
pub struct SerializableHttpResponse {
    #[serde(with = "http_serde::status_code")]
    #[cfg_attr(feature = "rkyv_format", with(rkyv_status_code::StatusCodeAsU16))]
    status: http::StatusCode,
    version: String,
    body: Bytes,
    #[serde(with = "http_serde::header_map")]
    #[cfg_attr(feature = "rkyv_format", with(rkyv_header_map::AsHeaderVec))]
    headers: HeaderMap,
}

#[async_trait]
impl<ResBody> CacheableResponse for CacheableHttpResponse<ResBody>
where
    ResBody: HttpBody + Send + 'static,
    // debug bounds
    ResBody::Error: Debug + Send,
    ResBody::Data: Send,
{
    type Cached = SerializableHttpResponse;
    type Subject = Self;

    async fn cache_policy<P>(
        self,
        predicates: P,
        config: &EntityPolicyConfig,
    ) -> hitbox::ResponseCachePolicy<Self>
    where
        P: hitbox::Predicate<Subject = Self::Subject> + Send + Sync,
    {
        match predicates.check(self).await {
            PredicateResult::Cacheable(cacheable) => match cacheable.into_cached().await {
                CachePolicy::Cacheable(res) => CachePolicy::Cacheable(CacheValue::new(
                    res,
                    config.ttl.map(|duration| Utc::now() + duration),
                    config.stale_ttl.map(|duration| Utc::now() + duration),
                )),
                CachePolicy::NonCacheable(res) => CachePolicy::NonCacheable(res),
            },
            PredicateResult::NonCacheable(res) => CachePolicy::NonCacheable(res),
        }
    }

    async fn into_cached(self) -> CachePolicy<Self::Cached, Self> {
        let body_bytes = match self.body.collect().await {
            Ok(bytes) => bytes,
            Err(error_body) => {
                // If collection fails, return NonCacheable with error body
                return CachePolicy::NonCacheable(CacheableHttpResponse {
                    parts: self.parts,
                    body: error_body,
                });
            }
        };

        // We can store the HeaderMap directly, including pseudo-headers
        // HeaderMap is designed to handle pseudo-headers and http-serde will serialize them correctly
        CachePolicy::Cacheable(SerializableHttpResponse {
            status: self.parts.status,
            version: format!("{:?}", self.parts.version),
            body: body_bytes,
            headers: self.parts.headers,
        })
    }

    async fn from_cached(cached: Self::Cached) -> Self {
        let body = BufferedBody::Complete(Some(cached.body));
        let mut response = Response::new(body);
        *response.status_mut() = cached.status;
        *response.headers_mut() = cached.headers;

        CacheableHttpResponse::from_response(response)
    }
}
