use serde::Serialize;
use actix_multipart::MultipartError;

macro_rules! define_errors {
    (
        $(
            $name:ident => $status:ident : $msg:literal
        ),* $(,)?
    ) => {
        #[derive(thiserror::Error, Debug, Serialize)]
        pub enum Error {
            $(
                #[error($msg)]
                $name(String),
            )*
        }

        pub trait ErrorVariant {
            fn build(msg: String) -> Error;
        }

        $(
            #[derive(Debug)]
            pub struct $name;

            impl ErrorVariant for $name {
                fn build(msg: String) -> Error {
                    Error::$name(msg)
                }
            }
        )*

        impl Error {
            pub fn new<T: ErrorVariant>(msg: impl Into<String>) -> Self {
                T::build(msg.into())
            }
        }

        impl actix_web::ResponseError for Error {
            fn error_response(&self) -> actix_web::HttpResponse {
                match self {
                    $(
                        Error::$name(e) => actix_web::HttpResponse::$status().body(e.clone()),
                    )*
                }
            }
        }

        impl From<surrealdb::Error> for Error {
            fn from(err: surrealdb::Error) -> Self {
                Error::Db(err.to_string())
            }
        }

        impl From<std::io::Error> for Error {
            fn from(err: std::io::Error) -> Self {
                Error::Internal(err.to_string())
            }
        }

        impl From<serde_json::Error> for Error {
            fn from(err: serde_json::Error) -> Self {
                Error::BadRequest(err.to_string())
            }
        }
        
        impl From<MultipartError> for Error {
            fn from(err: MultipartError) -> Self {
                Error::BadRequest(err.to_string())
            }
        }

    };
}

define_errors! {
    Db           => InternalServerError : "Database error",
    Unauthorized => Unauthorized        : "Unauthorized",
    Forbidden    => Forbidden           : "Forbidden",
    NotFound     => NotFound            : "Not Found",
    BadRequest   => BadRequest          : "Bad Request",
    Conflict     => Conflict            : "Conflict",
    Timeout      => GatewayTimeout      : "Timeout",
    Internal     => InternalServerError : "Internal Server Error",
}
