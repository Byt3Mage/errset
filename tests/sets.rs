use errset::{ErrorSet, ResultExt, Set3, set};
use std::{error::Error, io};

#[derive(Debug, Clone, PartialEq, Eq, Hash, thiserror::Error)]
#[error("token is malformed")]
struct Malformed;

#[derive(Debug, Clone, PartialEq, Eq, Hash, thiserror::Error)]
#[error("token has expired")]
struct Expired;

#[derive(Debug, Clone, PartialEq, Eq, Hash, thiserror::Error)]
#[error("signature mismatch")]
struct BadSignature;

#[derive(Debug, Clone, PartialEq, Eq, Hash, thiserror::Error)]
#[error("connection refused")]
struct Refused;

#[derive(Debug, thiserror::Error)]
#[error("handshake failed")]
struct Handshake(#[source] io::Error);

#[derive(Debug, PartialEq)]
struct Token {
    expiry: u64,
}

fn verify(sig: &str) -> Result<(), BadSignature> {
    if sig == "ok" {
        Ok(())
    } else {
        Err(BadSignature)
    }
}

fn check_expiry(expiry: u64, now: u64) -> Result<(), set!(Expired)> {
    if expiry > now {
        Ok(())
    } else {
        Err(ErrorSet::new(Expired))
    }
}

fn parse_token(raw: &str, now: u64) -> Result<Token, set!(Malformed, Expired, BadSignature)> {
    let Some((body, sig)) = raw.split_once('.') else {
        return Err(ErrorSet::new(Malformed));
    };
    verify(sig).into_set()?;
    let expiry = body.parse::<u64>().map_err(|_| Malformed).into_set()?;
    check_expiry(expiry, now).into_set()?;
    Ok(Token { expiry })
}

#[allow(clippy::type_complexity)]
fn connect(
    raw: &str,
    now: u64,
    refuse: bool,
) -> Result<u64, set!(io::Error, Refused, BadSignature, Expired, Malformed)> {
    let token = parse_token(raw, now).into_set()?;
    if refuse {
        return Err(ErrorSet::new(Refused));
    }
    std::fs::metadata("/definitely/not/here").into_set()?;
    Ok(token.expiry)
}

fn connect_with_retry(
    raw: &str,
    now: u64,
) -> Result<u64, set!(io::Error, Refused, BadSignature, Malformed)> {
    match connect(raw, now, true) {
        Ok(v) => Ok(v),
        Err(e) => match e.narrow::<Expired, _>() {
            Ok(Expired) => Ok(0),
            Err(rest) => Err(rest.widen()),
        },
    }
}

#[test]
fn members_and_display() {
    assert_eq!(parse_token("9.ok", 1).unwrap(), Token { expiry: 9 });
    assert_eq!(
        parse_token("nodot", 1).unwrap_err().to_string(),
        "token is malformed"
    );
    assert_eq!(
        parse_token("x.ok", 1).unwrap_err().to_string(),
        "token is malformed"
    );
    assert_eq!(
        parse_token("9.bad", 1).unwrap_err().to_string(),
        "signature mismatch"
    );
    assert_eq!(
        parse_token("1.ok", 5).unwrap_err().to_string(),
        "token has expired"
    );
    assert_eq!(
        format!("{:?}", parse_token("1.ok", 5).unwrap_err()),
        "Expired"
    );
}

#[test]
fn widening_reorders() {
    assert_eq!(
        connect("9.ok", 1, true).unwrap_err().to_string(),
        "connection refused"
    );
    assert_eq!(
        connect("1.ok", 5, false).unwrap_err().to_string(),
        "token has expired"
    );
    assert!(
        connect("9.ok", 1, false)
            .unwrap_err()
            .narrow::<io::Error, _>()
            .is_ok()
    );
}

#[test]
fn narrowing() {
    assert_eq!(connect_with_retry("1.ok", 5).unwrap(), 0);
    assert_eq!(
        connect_with_retry("9.ok", 1).unwrap_err().to_string(),
        "connection refused"
    );

    let e = parse_token("9.bad", 1).unwrap_err();
    let rest = e.narrow::<Malformed, _>().unwrap_err();
    let rest = rest.narrow::<Expired, _>().unwrap_err();
    assert_eq!(rest.clone().into_single(), BadSignature);
    assert!(matches!(rest.narrow::<BadSignature, _>(), Ok(BadSignature)));
}

#[test]
fn inspecting_without_consuming() {
    let e = parse_token("1.ok", 5).unwrap_err();
    assert_eq!(e.get::<Expired, _>(), Some(&Expired));
    assert_eq!(e.get::<Malformed, _>(), None);
    assert!(e.is::<Expired, _>());
    assert!(!e.is::<BadSignature, _>());
    assert_eq!(e.to_string(), "token has expired");
}

#[test]
fn exhaustive_match() {
    match parse_token("1.ok", 5).unwrap_err().as_enum() {
        Set3::V0(Malformed) => panic!(),
        Set3::V1(Expired) => {}
        Set3::V2(BadSignature) => panic!(),
    }
    match parse_token("9.bad", 1).unwrap_err().as_enum() {
        Set3::V2(BadSignature) => {}
        _ => panic!(),
    }
}

#[test]
fn transparent_source() {
    fn fail() -> Result<(), set!(Handshake, Refused)> {
        Err(Handshake(io::Error::other("reset"))).into_set()
    }
    let e = fail().unwrap_err();
    assert_eq!(e.to_string(), "handshake failed");
    assert_eq!(e.source().unwrap().to_string(), "reset");
}

#[test]
fn stays_inline() {
    assert_eq!(size_of::<set!(Malformed, Expired, BadSignature)>(), 1);
    assert_eq!(
        size_of::<Result<(), set!(Malformed, Expired, BadSignature)>>(),
        1
    );
    assert_eq!(
        size_of::<set!(io::Error, Refused)>(),
        size_of::<Result<(), io::Error>>()
    );
}

#[test]
fn equality_and_hash() {
    use std::collections::HashSet;
    let a: set!(Malformed, Expired) = ErrorSet::new(Expired);
    let b: set!(Malformed, Expired) = ErrorSet::new(Expired);
    let c: set!(Malformed, Expired) = ErrorSet::new(Malformed);
    assert_eq!(a, b);
    assert_ne!(a, c);
    let mut seen = HashSet::new();
    seen.insert(a);
    assert!(seen.contains(&b));
    assert!(!seen.contains(&c));
}

#[test]
fn empty_set() {
    fn never() -> Result<u8, set!()> {
        Ok(3)
    }
    let v = match never() {
        Ok(v) => v,
        Err(e) => e.unreachable(),
    };
    assert_eq!(v, 3);
}
