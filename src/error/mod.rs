use serde::{Serialize, Deserialize};
use actix_multipart::MultipartError;
use tokio::task::JoinError;

macro_rules! define_errors {
    (
        $(
            $name:ident => $status:ident : $msg:literal
        ),* $(,)?
    ) => {
        #[derive(thiserror::Error, Debug, Serialize, Deserialize)]
        pub enum ErrorIO {
            $(
                #[error($msg)]
                $name(String),
            )*
        }

        pub trait ErrorVariant {
            fn build(msg: String) -> ErrorIO;
        }

        $(
            #[derive(Debug)]
            pub struct $name;

            impl ErrorVariant for $name {
                fn build(msg: String) -> ErrorIO {
                    ErrorIO::$name(msg)
                }
            }
        )*

        impl ErrorIO {
            pub fn new<T: ErrorVariant>(msg: impl Into<String>) -> Self {
                T::build(msg.into())
            }
        }

        impl actix_web::ResponseError for ErrorIO {
            fn error_response(&self) -> actix_web::HttpResponse {
                match self {
                    $(
                        ErrorIO::$name(e) => actix_web::HttpResponse::$status().body(e.clone()),
                    )*
                }
            }
        }

        impl From<surrealdb::Error> for ErrorIO {
            fn from(err: surrealdb::Error) -> Self {
                ErrorIO::Db(err.to_string())
            }
        }

        impl From<std::io::Error> for ErrorIO {
            fn from(err: std::io::Error) -> Self {
                ErrorIO::Internal(err.to_string())
            }
        }

        impl From<serde_json::Error> for ErrorIO {
            fn from(err: serde_json::Error) -> Self {
                ErrorIO::BadRequest(err.to_string())
            }
        }

        impl From<MultipartError> for ErrorIO {
            fn from(err: MultipartError) -> Self {
                ErrorIO::BadRequest(err.to_string())
            }
        }

        impl From<image::ImageError> for ErrorIO {
            fn from(err: image::ImageError) -> Self {
                ErrorIO::Internal(err.to_string())
            }
        }

        impl From<actix_web::Error> for ErrorIO {
            fn from(err: actix_web::Error) -> Self {
                ErrorIO::Internal(err.to_string())
            }
        }

        impl From<JoinError> for ErrorIO {
            fn from(err: JoinError) -> Self {
                if err.is_panic() {
                    ErrorIO::Internal(format!("Background task panicked: {:?}", err))
                } else {
                    ErrorIO::Internal(format!("Background task cancelled: {:?}", err))
                }
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