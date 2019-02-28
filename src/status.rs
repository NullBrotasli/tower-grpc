use bytes::Bytes;
use h2;
use http::{self, HeaderMap};
use http::header::HeaderValue;
use std::{error::Error, fmt};
use percent_encoding::percent_decode;

#[derive(Clone)]
pub struct Status {
    code: Code,
    error_message: String,
    binary_error_details: Bytes,
}

const GRPC_STATUS_HEADER_CODE: &str = "grpc-status";
const GRPC_STATUS_MESSAGE_HEADER: &str = "grpc-message";
const GRPC_STATUS_DETAILS_HEADER: &str = "grpc-status-details-bin";

impl Status {
    pub fn with_code(code: Code) -> Status {
        Status {
            code,
            error_message: String::new(),
            binary_error_details: Bytes::new(),
        }
    }

    pub fn with_code_and_message(code: Code, message: String) -> Status {
        Status {
            code,
            error_message: message,
            binary_error_details: Bytes::new(),
        }
    }

    // TODO: This should probably be made public eventually. Need to decide on
    // the exact argument type.
    pub(crate) fn from_error(err: &(dyn Error + 'static)) -> Status {
        if let Some(status) = err.downcast_ref::<Status>() {
            Status {
                code: status.code,
                error_message: status.error_message.clone(),
                binary_error_details: status.binary_error_details.clone(),
            }
        } else {
            Status::with_code_and_message(
                Code::Unknown,
                err.to_string(),
            )
        }
    }

    pub fn from_header_map(header_map: &HeaderMap) -> Option<Status> {
        header_map.get(GRPC_STATUS_HEADER_CODE).map(|code| {
            let code = Code::from_bytes(code.as_ref());
            let error_message = header_map.get(GRPC_STATUS_MESSAGE_HEADER)
                .map(|header|
                    percent_decode(header.as_bytes())
                        .decode_utf8()
                        .map(|cow| cow.to_string()))
                .unwrap_or_else(|| Ok(String::new()));
            let binary_error_details = header_map.get(GRPC_STATUS_DETAILS_HEADER)
                .map(|h| Bytes::from(h.as_bytes())).unwrap_or_else(Bytes::new);
            match error_message {
                Ok(error_message) => Status {
                    code,
                    error_message,
                    binary_error_details,
                },
                Err(err) => {
                    warn!("Error deserializing status message header: {}", err);
                    Status {
                        code: Code::Unknown,
                        error_message: format!("Error deserializing status message header: {}", err),
                        binary_error_details,
                    }
                }
            }
        })
    }

    pub fn code(&self) -> Code {
        self.code
    }

    pub fn error_message(&self) -> &str {
        &self.error_message
    }

    pub fn binary_error_details(&self) -> &Bytes {
        &self.binary_error_details
    }

    // TODO: It would be nice for this not to be public
    pub fn to_header_map(&self) -> Result<HeaderMap, Self> {
        let mut header_map = HeaderMap::with_capacity(3);

        header_map.insert(GRPC_STATUS_HEADER_CODE, self.code.to_header_value());

        if !self.error_message.is_empty() {
            header_map.insert(GRPC_STATUS_MESSAGE_HEADER, HeaderValue::from_str(&self.error_message)
                .map_err(invalid_header_value_byte)?);
        }
        if !self.binary_error_details.is_empty() {
            header_map.insert(GRPC_STATUS_DETAILS_HEADER, HeaderValue::from_shared(self.binary_error_details.clone())
                .map_err(invalid_header_value_byte)?);
        }
        Ok(header_map)
    }
}

impl fmt::Debug for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // A manual impl to reduce the noise of frequently empty fields.
        let mut builder = f.debug_struct("Status");

        builder.field("code", &self.code);

        if !self.error_message.is_empty() {
            builder.field("message", &self.error_message);
        }

        if !self.binary_error_details.is_empty() {
            builder.field("details_bin", &self.binary_error_details);
        }

        builder.finish()
    }
}

fn invalid_header_value_byte<Error: fmt::Display>(err: Error) -> Status {
    debug!("Invalid header: {}", err);
    Status::with_code_and_message(
        Code::Internal,
        "Couldn't serialize non-text grpc status header".to_string(),
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    Ok = 0,
    Cancelled = 1,
    Unknown = 2,
    InvalidArgument = 3,
    DeadlineExceeded = 4,
    NotFound = 5,
    AlreadyExists = 6,
    PermissionDenied = 7,
    ResourceExhausted = 8,
    FailedPrecondition = 9,
    Aborted = 10,
    OutOfRange = 11,
    Unimplemented = 12,
    Internal = 13,
    Unavailable = 14,
    DataLoss = 15,
    Unauthenticated = 16,
}

impl Code {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Code {
        match bytes.len() {
            1 => {
                match bytes[0] {
                    b'0' => Code::Ok,
                    b'1' => Code::Cancelled,
                    b'2' => Code::Unknown,
                    b'3' => Code::InvalidArgument,
                    b'4' => Code::DeadlineExceeded,
                    b'5' => Code::NotFound,
                    b'6' => Code::AlreadyExists,
                    b'7' => Code::PermissionDenied,
                    b'8' => Code::ResourceExhausted,
                    b'9' => Code::FailedPrecondition,
                    _ => Code::parse_err(),
                }
            },
            2 => {
                match (bytes[0], bytes[1]) {
                    (b'1', b'0') => Code::Aborted,
                    (b'1', b'1') => Code::OutOfRange,
                    (b'1', b'2') => Code::Unimplemented,
                    (b'1', b'3') => Code::Internal,
                    (b'1', b'4') => Code::Unavailable,
                    (b'1', b'5') => Code::DataLoss,
                    (b'1', b'6') => Code::Unauthenticated,
                    _ => Code::parse_err(),
                }
            },
            _ => Code::parse_err(),
        }
    }

    // TODO: It would be nice for this not to be public
    pub fn to_header_value(&self) -> HeaderValue {
        match self {
            Code::Ok => HeaderValue::from_static("0"),
            Code::Cancelled => HeaderValue::from_static("1"),
            Code::Unknown => HeaderValue::from_static("2"),
            Code::InvalidArgument => HeaderValue::from_static("3"),
            Code::DeadlineExceeded => HeaderValue::from_static("4"),
            Code::NotFound => HeaderValue::from_static("5"),
            Code::AlreadyExists => HeaderValue::from_static("6"),
            Code::PermissionDenied => HeaderValue::from_static("7"),
            Code::ResourceExhausted => HeaderValue::from_static("8"),
            Code::FailedPrecondition => HeaderValue::from_static("9"),
            Code::Aborted => HeaderValue::from_static("10"),
            Code::OutOfRange => HeaderValue::from_static("11"),
            Code::Unimplemented => HeaderValue::from_static("12"),
            Code::Internal => HeaderValue::from_static("13"),
            Code::Unavailable => HeaderValue::from_static("14"),
            Code::DataLoss => HeaderValue::from_static("15"),
            Code::Unauthenticated => HeaderValue::from_static("16"),
        }
    }

    fn parse_err() -> Code {
        trace!("error parsing grpc-status");
        Code::Unknown
    }
}

impl From<h2::Error> for Status {
    fn from(err: h2::Error) -> Self {
        // See https://github.com/grpc/grpc/blob/3977c30/doc/PROTOCOL-HTTP2.md#errors
        let code = match err.reason() {
            Some(h2::Reason::NO_ERROR) |
            Some(h2::Reason::PROTOCOL_ERROR) |
            Some(h2::Reason::INTERNAL_ERROR) |
            Some(h2::Reason::FLOW_CONTROL_ERROR) |
            Some(h2::Reason::SETTINGS_TIMEOUT) |
            Some(h2::Reason::COMPRESSION_ERROR) |
            Some(h2::Reason::CONNECT_ERROR) => Code::Internal,
            Some(h2::Reason::REFUSED_STREAM) => Code::Unavailable,
            Some(h2::Reason::CANCEL) => Code::Cancelled,
            Some(h2::Reason::ENHANCE_YOUR_CALM) => Code::ResourceExhausted,
            Some(h2::Reason::INADEQUATE_SECURITY) => Code::PermissionDenied,

            _ => Code::Unknown,
        };


        Status::with_code_and_message(
            code,
            format!("h2 protocol error: {}", err),
        )
    }
}

impl From<Status> for h2::Error {
    fn from(_status: Status) -> Self {
        // TODO: implement
        h2::Reason::INTERNAL_ERROR.into()
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "grpc-status: {:?}, grpc-message: {:?}",
            self.code(),
            self.error_message()
        )
    }
}

impl Error for Status {}

///
/// Take the `Status` value from `trailers` if it is available, else from `status_code`.
///
pub(crate) fn infer_grpc_status(trailers: Option<HeaderMap>, status_code: http::StatusCode) -> Result<(), Status> {
    if let Some(trailers) = trailers {
        if let Some(status) = Status::from_header_map(&trailers) {
            if status.code() == Code::Ok {
                return Ok(());
            } else {
                return Err(status);
            }
        }
    }
    trace!("trailers missing grpc-status");
    let code = match status_code {
        // Borrowed from https://github.com/grpc/grpc/blob/master/doc/http-grpc-status-mapping.md
        http::StatusCode::BAD_REQUEST => Code::Internal,
        http::StatusCode::UNAUTHORIZED => Code::Unauthenticated,
        http::StatusCode::FORBIDDEN => Code::PermissionDenied,
        http::StatusCode::NOT_FOUND => Code::Unimplemented,
        http::StatusCode::TOO_MANY_REQUESTS |
        http::StatusCode::BAD_GATEWAY |
        http::StatusCode::SERVICE_UNAVAILABLE |
        http::StatusCode::GATEWAY_TIMEOUT => Code::Unavailable,
        _ => Code::Unknown,
    };

    let msg = format!(
        "grpc-status header missing, mapped from HTTP status code {}",
        status_code.as_u16(),
    );
    let status = Status::with_code_and_message(code, msg);
    Err(status)
}
