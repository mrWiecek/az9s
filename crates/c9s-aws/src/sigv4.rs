//! AWS Signature Version 4 for the few JSON APIs c9s calls.

use ring::{digest, hmac};

/// What a request is signed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    /// Temporary credentials (SSO, assumed roles) carry a session token.
    pub session_token: Option<String>,
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(data: &[u8]) -> String {
    hex(digest::digest(&digest::SHA256, data).as_ref())
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), data.as_bytes())
        .as_ref()
        .to_vec()
}

/// The key for one day, region and service.
pub fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac(format!("AWS4{secret}").as_bytes(), date);
    let k_region = hmac(&k_date, region);
    let k_service = hmac(&k_region, service);
    hmac(&k_service, "aws4_request")
}

/// A request to sign. `headers` are lowercase names with their values and
/// must include `host`; `x-amz-date` (and the session token) are added.
pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    /// Already canonical: sorted `key=value` pairs, URI-encoded.
    pub query: &'a str,
    pub headers: Vec<(String, String)>,
    pub body: &'a [u8],
}

/// The headers to send: the request's own plus `x-amz-date`, the session
/// token and `authorization`. `amz_date` is `YYYYMMDDTHHMMSSZ`.
pub fn sign(
    request: &Request,
    credentials: &Credentials,
    region: &str,
    service: &str,
    amz_date: &str,
) -> Vec<(String, String)> {
    let mut headers = request.headers.clone();
    headers.push(("x-amz-date".into(), amz_date.into()));
    if let Some(token) = &credentials.session_token {
        headers.push(("x-amz-security-token".into(), token.clone()));
    }
    headers.sort();
    let canonical_headers: String = headers
        .iter()
        .map(|(k, v)| format!("{k}:{}\n", v.trim()))
        .collect();
    let signed_headers = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{}",
        request.method,
        request.path,
        request.query,
        sha256_hex(request.body)
    );
    let date = &amz_date[..8];
    let scope = format!("{date}/{region}/{service}/aws4_request");
    let to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical.as_bytes())
    );
    let key = signing_key(&credentials.secret_access_key, date, region, service);
    let signature = hex(&hmac(&key, &to_sign));
    headers.push((
        "authorization".into(),
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
            credentials.access_key_id
        ),
    ));
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    // The example from AWS's Signature Version 4 documentation (IAM
    // ListUsers, 2015-08-30).
    const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";

    #[test]
    fn signing_key_matches_the_documented_example() {
        assert_eq!(
            hex(&signing_key(SECRET, "20150830", "us-east-1", "iam")),
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
    }

    #[test]
    fn signature_matches_the_documented_example() {
        let request = Request {
            method: "GET",
            path: "/",
            query: "Action=ListUsers&Version=2010-05-08",
            headers: vec![
                (
                    "content-type".into(),
                    "application/x-www-form-urlencoded; charset=utf-8".into(),
                ),
                ("host".into(), "iam.amazonaws.com".into()),
            ],
            body: b"",
        };
        let creds = Credentials {
            access_key_id: "AKIDEXAMPLE".into(),
            secret_access_key: SECRET.into(),
            session_token: None,
        };
        let headers = sign(&request, &creds, "us-east-1", "iam", "20150830T123600Z");
        let auth = &headers
            .iter()
            .find(|(k, _)| k == "authorization")
            .unwrap()
            .1;
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request, \
             SignedHeaders=content-type;host;x-amz-date, \
             Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
    }

    #[test]
    fn a_session_token_is_signed_and_sent() {
        let request = Request {
            method: "POST",
            path: "/",
            query: "",
            headers: vec![("host".into(), "example.amazonaws.com".into())],
            body: b"{}",
        };
        let creds = Credentials {
            access_key_id: "A".into(),
            secret_access_key: "S".into(),
            session_token: Some("T".into()),
        };
        let headers = sign(&request, &creds, "eu-west-1", "tagging", "20260101T000000Z");
        assert!(headers.contains(&("x-amz-security-token".into(), "T".into())));
        let auth = &headers
            .iter()
            .find(|(k, _)| k == "authorization")
            .unwrap()
            .1;
        assert!(
            auth.contains("SignedHeaders=host;x-amz-date;x-amz-security-token"),
            "{auth}"
        );
    }
}
