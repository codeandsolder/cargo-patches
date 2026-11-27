// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! JWT encoding helpers.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use btls::{
    hash::MessageDigest,
    pkey::{Id, PKey, PKeyRef, Private},
    rsa::{Padding, Rsa},
    sign::Signer,
};
use serde::Serialize;

use crate::{Error, Result};

/// Encode a JWS compact JWT using the RS256 algorithm.
///
/// RS256 is RSA PKCS#1 v1.5 with SHA-256. The caller owns the JWT header and
/// claims shape so service-specific fields such as `x5t` stay service-local.
pub fn encode_rs256<H, C>(header: &H, claims: &C, private_key: &PKeyRef<Private>) -> Result<String>
where
    H: Serialize,
    C: Serialize,
{
    let encoded_header = encode_json(header)?;
    let encoded_claims = encode_json(claims)?;
    let signing_input = format!("{encoded_header}.{encoded_claims}");

    let signature = sign_rs256(private_key, signing_input.as_bytes())?;
    let encoded_signature = URL_SAFE_NO_PAD.encode(signature);

    Ok(format!("{signing_input}.{encoded_signature}"))
}

/// Encode a JWS compact JWT using an unencrypted RSA PKCS#8 or PKCS#1 PEM key.
pub fn encode_rs256_pem<H, C>(header: &H, claims: &C, private_key_pem: &[u8]) -> Result<String>
where
    H: Serialize,
    C: Serialize,
{
    let private_key = parse_rsa_private_key(private_key_pem)?;
    encode_rs256(header, claims, &private_key)
}

/// Parse an unencrypted RSA PKCS#8 or PKCS#1 PEM key. Decode the explicit
/// PEM type before native parsing, so encrypted keys never prompt for input.
pub fn parse_rsa_private_key(private_key_pem: &[u8]) -> Result<PKey<Private>> {
    let block = pem::parse(private_key_pem).map_err(|e| {
        Error::credential_invalid("failed to parse RSA private key PEM").with_source(e)
    })?;
    let key = match block.tag() {
        "PRIVATE KEY" => PKey::private_key_from_pkcs8(block.contents()),
        "RSA PRIVATE KEY" => Rsa::private_key_from_der(block.contents()).and_then(PKey::from_rsa),
        _ => {
            return Err(Error::credential_invalid(
                "expected an unencrypted RSA private key PEM",
            ));
        }
    }
    .map_err(|e| Error::credential_invalid("failed to decode RSA private key").with_source(e))?;
    if key.id() != Id::RSA {
        return Err(Error::credential_invalid("private key is not RSA"));
    }
    let rsa = key
        .rsa()
        .map_err(|e| Error::credential_invalid("invalid RSA private key").with_source(e))?;
    rsa.check_key()
        .map_err(|e| Error::credential_invalid("invalid RSA private key").with_source(e))?;
    Ok(key)
}

/// Sign arbitrary bytes with RSASSA-PKCS1-v1_5 and SHA-256.
///
/// Shared by JWT assertions and Google V4 signed URLs.
pub fn sign_rs256(private_key: &PKeyRef<Private>, message: &[u8]) -> Result<Vec<u8>> {
    if private_key.id() != Id::RSA {
        return Err(Error::credential_invalid("private key is not RSA"));
    }
    let sign = || {
        let mut signer = Signer::new_oneshot(MessageDigest::sha256(), private_key)?;
        signer.set_rsa_padding(Padding::PKCS1)?;
        signer.sign_oneshot_to_vec(message)
    };
    sign().map_err(|e| Error::unexpected("failed to sign RS256").with_source(e))
}

fn encode_json<T>(value: &T) -> Result<String>
where
    T: Serialize,
{
    let json = serde_json::to_vec(value)
        .map_err(|e| Error::unexpected("failed to serialize JWT JSON").with_source(e))?;
    Ok(URL_SAFE_NO_PAD.encode(json))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use btls::sign::Verifier;
    use serde_json::json;

    #[derive(Serialize)]
    struct Header<'a> {
        alg: &'a str,
        typ: &'a str,
        x5t: &'a str,
    }

    #[derive(Serialize)]
    struct Claims<'a> {
        iss: &'a str,
        sub: &'a str,
        aud: &'a str,
        exp: u64,
    }

    #[test]
    fn encode_rs256_keeps_jws_shape_and_verifiable_signature() -> Result<()> {
        let private_key = Rsa::generate(2048)
            .and_then(PKey::from_rsa)
            .map_err(|e| Error::unexpected("failed to generate test RSA key").with_source(e))?;

        let jwt = encode_rs256(
            &Header {
                alg: "RS256",
                typ: "JWT",
                x5t: "thumbprint",
            },
            &Claims {
                iss: "client",
                sub: "client",
                aud: "https://example.com/token",
                exp: 1,
            },
            &private_key,
        )?;

        let parts = jwt.split('.').collect::<Vec<_>>();
        assert_eq!(parts.len(), 3);

        let header: serde_json::Value = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(parts[0])
                .map_err(|e| Error::unexpected("failed to decode JWT header").with_source(e))?,
        )
        .map_err(|e| Error::unexpected("failed to parse JWT header").with_source(e))?;
        assert_eq!(
            header,
            json!({
                "alg": "RS256",
                "typ": "JWT",
                "x5t": "thumbprint"
            })
        );

        let signature = URL_SAFE_NO_PAD
            .decode(parts[2])
            .map_err(|e| Error::unexpected("failed to decode JWT signature").with_source(e))?;
        let mut verifier = Verifier::new_oneshot(MessageDigest::sha256(), &private_key)
            .map_err(|e| Error::unexpected("failed to initialize verifier").with_source(e))?;
        verifier
            .set_rsa_padding(Padding::PKCS1)
            .map_err(|e| Error::unexpected("failed to set RSA padding").with_source(e))?;
        assert!(
            verifier
                .verify_oneshot(&signature, format!("{}.{}", parts[0], parts[1]).as_bytes())
                .map_err(|e| Error::unexpected("failed to verify JWT signature").with_source(e))?
        );

        Ok(())
    }
    #[test]
    fn rs256_matches_independent_openssl_reference() {
        // Public test-only key from btls/test/rsa.pem. Expected bytes generated
        // with openssl dgst -sha256 -sign, not with the provider under test.
        let key =
            parse_rsa_private_key(include_bytes!("../tests/fixtures/rs256-test-key.pem")).unwrap();
        let expected =
            hex::decode(include_str!("../tests/fixtures/rs256-openssl-signature.hex").trim())
                .unwrap();
        assert_eq!(
            sign_rs256(&key, b"reqsign RS256 reference vector").unwrap(),
            expected
        );
        let mut verifier = Verifier::new_oneshot(MessageDigest::sha256(), &key).unwrap();
        verifier.set_rsa_padding(Padding::PKCS1).unwrap();
        assert!(
            !verifier
                .verify_oneshot(&expected, b"altered reference vector")
                .unwrap()
        );
    }

    #[test]
    fn pem_key_formats_and_wrong_key_rejection() {
        let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
        let pkcs8 = key.private_key_to_pem_pkcs8().unwrap();
        let pkcs1 = key.rsa().unwrap().private_key_to_pem().unwrap();
        let message = b"reqsign RS256 key-format regression";
        let signature = sign_rs256(&key, message).unwrap();
        for pem in [pkcs8, pkcs1] {
            let parsed = parse_rsa_private_key(&pem).unwrap();
            assert!(parsed.public_eq(&key));
            assert_eq!(sign_rs256(&parsed, message).unwrap(), signature);
        }
        let wrong = PKey::generate(Id::ED25519).unwrap();
        assert!(parse_rsa_private_key(&wrong.private_key_to_pem_pkcs8().unwrap()).is_err());
        assert!(sign_rs256(&wrong, message).is_err());
        assert!(parse_rsa_private_key(b"invalid key").is_err());
        let mut der = key.private_key_to_der_pkcs8().unwrap();
        der.push(0);
        let trailing = pem::encode(&pem::Pem::new("PRIVATE KEY", der));
        assert!(parse_rsa_private_key(trailing.as_bytes()).is_err());
        // A syntactically valid PKCS#1 key with an inconsistent CRT
        // coefficient must fail native key validation, not reach signing.
        let mut inconsistent = key.rsa().unwrap().private_key_to_der().unwrap();
        *inconsistent.last_mut().unwrap() ^= 1;
        let inconsistent = pem::encode(&pem::Pem::new("RSA PRIVATE KEY", inconsistent));
        assert!(parse_rsa_private_key(inconsistent.as_bytes()).is_err());
    }
}
