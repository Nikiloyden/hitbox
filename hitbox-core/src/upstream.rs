use std::future::Future;

/// Trait for calling upstream services with cacheable requests.
/// This trait is framework-agnostic and can be implemented for any async service.
///
/// # Examples
///
/// ```rust,ignore
/// use hitbox_core::Upstream;
/// use std::future::Ready;
///
/// struct MockUpstream {
///     response: MyResponse,
/// }
///
/// impl Upstream<MyRequest> for MockUpstream {
///     type Response = MyResponse;
///     type Future = Ready<Self::Response>;
///
///     fn call(&mut self, _req: MyRequest) -> Self::Future {
///         std::future::ready(self.response.clone())
///     }
/// }
/// ```
pub trait Upstream<Req> {
    /// The response type returned by the upstream service
    type Response;

    /// The future that resolves to the response
    type Future: Future<Output = Self::Response> + Send;

    /// Call the upstream service with the given request
    fn call(&mut self, req: Req) -> Self::Future;
}
