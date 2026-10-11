//! Pure credential validation shared by setup generation and local HTTP transport.

use std::net::{Ipv4Addr, SocketAddr};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CredentialError {
    Endpoint,
    Token,
}

pub(crate) fn validate_credentials(
    endpoint: &str,
    token: &str,
) -> Result<SocketAddr, CredentialError> {
    let port = endpoint
        .strip_prefix("http://127.0.0.1:")
        .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port != 0)
        .ok_or(CredentialError::Endpoint)?;
    decode_token(token).ok_or(CredentialError::Token)?;
    Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
}

pub(crate) fn decode_token(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut decoded = [0; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let digit = |value: u8| match value {
            b'0'..=b'9' => Some(value - b'0'),
            b'a'..=b'f' => Some(value - b'a' + 10),
            _ => None,
        };
        decoded[index] = digit(pair[0])? * 16 + digit(pair[1])?;
    }
    Some(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_validation_and_decoder_accept_the_same_lowercase_wire_format() {
        let endpoint = "http://127.0.0.1:12345";
        // Generate a fixture covering every hex digit without a credential-shaped literal.
        let token = (0_u8..16)
            .map(|digit| format!("{digit:x}"))
            .collect::<String>()
            .repeat(4);
        assert_eq!(
            decode_token(&token),
            Some(
                [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]
                    .repeat(4)
                    .try_into()
                    .unwrap()
            )
        );
        assert!(validate_credentials(endpoint, &token).is_ok());
        for invalid in [
            "a".repeat(63),
            "a".repeat(65),
            "A".repeat(64),
            "g".repeat(64),
            format!("{}\r\n", "a".repeat(62)),
            "é".repeat(32),
        ] {
            assert_eq!(decode_token(&invalid), None);
            assert_eq!(
                validate_credentials(endpoint, &invalid),
                Err(CredentialError::Token)
            );
        }
    }
}
