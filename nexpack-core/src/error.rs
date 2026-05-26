use std::fmt;

#[derive(Debug)]
pub enum Error {
	Io {
		context: String,
		source: std::io::Error,
	},
	InvalidFormat(String),
	Cbor(String),
	DigestMismatch {
		expected: String,
		actual: String,
	},
	SignatureVerification(String),
	StorePath(String),
	Mount(String),
	IndexOutOfRange {
		what: String,
		index: usize,
		max: usize,
	},
	NotFound(String),
	Other(String),
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Error::Io { context, source } => {
				write!(f, "I/O error {}: {}", context, source)
			}
			Error::InvalidFormat(msg) => write!(f, "invalid format: {}", msg),
			Error::Cbor(e) => write!(f, "CBOR error: {}", e),
			Error::DigestMismatch { expected, actual } => {
				write!(f, "digest mismatch: expected {}, got {}", expected, actual)
			}
			Error::SignatureVerification(msg) => write!(f, "signature error: {}", msg),
			Error::StorePath(msg) => write!(f, "store error: {}", msg),
			Error::Mount(msg) => write!(f, "mount error: {}", msg),
			Error::IndexOutOfRange { what, index, max } => {
				write!(f, "{} index {} out of range (max {})", what, index, max)
			}
			Error::NotFound(msg) => write!(f, "not found: {}", msg),
			Error::Other(msg) => write!(f, "{}", msg),
		}
	}
}

impl std::error::Error for Error {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			Error::Io { source, .. } => Some(source),
			_ => None,
		}
	}
}
