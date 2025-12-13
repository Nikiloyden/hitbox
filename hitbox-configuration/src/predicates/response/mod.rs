pub mod header;
pub mod status;

use hitbox_http::predicates::NeutralResponsePredicate;
use hitbox_http::predicates::conditions::Or;
use hyper::body::Body as HttpBody;
use serde::{Deserialize, Serialize};

use crate::ResponsePredicate;
use crate::error::ConfigError;
use crate::predicates::body::BodyOperationConfig;
use crate::predicates::version::{self, VersionOperationConfig};

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum Predicate {
    Status(status::Operation),
    Body(BodyOperationConfig),
    Header(header::HeaderOperation),
    Version(VersionOperationConfig),
}

impl Predicate {
    pub fn into_predicates<ReqBody>(
        self,
        inner: ResponsePredicate<ReqBody>,
    ) -> Result<ResponsePredicate<ReqBody>, ConfigError>
    where
        ReqBody: HttpBody + Send + Unpin + 'static,
        ReqBody::Error: std::fmt::Debug + Send,
        ReqBody::Data: Send,
    {
        match self {
            Predicate::Status(status_op) => status_op.into_predicates(inner),
            Predicate::Body(body_op) => Ok(Box::new(body_op.into_predicates(inner)?)),
            Predicate::Header(header_op) => header::into_predicates(header_op, inner),
            Predicate::Version(version_op) => {
                Ok(Box::new(version::into_predicates(version_op, inner)?))
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum Operation {
    And(Vec<Expression>),
    Or(Vec<Expression>),
}

impl Operation {
    pub fn into_predicates<ReqBody>(
        self,
        inner: ResponsePredicate<ReqBody>,
    ) -> Result<ResponsePredicate<ReqBody>, ConfigError>
    where
        ReqBody: HttpBody + Send + Unpin + 'static,
        ReqBody::Error: std::fmt::Debug + Send,
        ReqBody::Data: Send,
    {
        match self {
            Operation::Or(predicates) => {
                let mut iter = predicates.into_iter();
                match iter.next() {
                    None => Ok(inner),
                    Some(first) => {
                        let first_predicate = first
                            .into_predicates(Box::new(NeutralResponsePredicate::new())
                                as ResponsePredicate<ReqBody>)?;
                        iter.try_fold(first_predicate, |acc, expression| {
                            let predicate = expression
                                .into_predicates(Box::new(NeutralResponsePredicate::new())
                                    as ResponsePredicate<ReqBody>)?;
                            Ok(Box::new(Or::new(predicate, acc)) as ResponsePredicate<ReqBody>)
                        })
                    }
                }
            }
            Operation::And(predicates) => predicates
                .into_iter()
                .try_fold(inner, |inner, predicate| predicate.into_predicates(inner)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(untagged)]
pub enum Expression {
    Predicate(Predicate),
    Operation(Operation),
}

impl Expression {
    pub fn into_predicates<ReqBody>(
        self,
        inner: ResponsePredicate<ReqBody>,
    ) -> Result<ResponsePredicate<ReqBody>, ConfigError>
    where
        ReqBody: HttpBody + Send + Unpin + 'static,
        ReqBody::Error: std::fmt::Debug + Send,
        ReqBody::Data: Send,
    {
        match self {
            Self::Predicate(predicate) => predicate.into_predicates(inner),
            Self::Operation(operation) => operation.into_predicates(inner),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(untagged)]
pub enum Response {
    Flat(Vec<Predicate>),
    Tree(Expression),
}

impl Default for Response {
    fn default() -> Self {
        Response::Flat(Vec::new())
    }
}

impl Response {
    pub fn into_predicates<Req>(self) -> Result<ResponsePredicate<Req>, ConfigError>
    where
        Req: HttpBody + Send + Unpin + 'static,
        Req::Error: std::fmt::Debug + Send,
        Req::Data: Send,
    {
        let neutral_predicate: ResponsePredicate<Req> =
            Box::new(NeutralResponsePredicate::<Req>::new());
        match self {
            Response::Flat(predicates) => predicates.into_iter().try_rfold(
                neutral_predicate,
                |inner, predicate| -> Result<ResponsePredicate<Req>, ConfigError> {
                    predicate.into_predicates(inner)
                },
            ),
            Response::Tree(expression) => expression.into_predicates(neutral_predicate),
        }
    }
}
