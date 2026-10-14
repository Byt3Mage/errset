use errset::{ErrorSet, catch, err, errors, fail, set, switch, try_};
use std::error::Error;

const CODE: u32 = 404;

errors! {
    /// The token text could not be parsed.
    pub Malformed = "token is malformed";
    pub NotFound = "page not found ({})", CODE;
    pub Unauthorized = "status {CODE}";
    pub Expired;
    pub BadSignature;
    pub Refused = "connection refused";
}

fn verify(sig: &str) -> Result<(), BadSignature> {
    if sig == "ok" {
        Ok(())
    } else {
        Err(BadSignature)
    }
}

fn parse_token(raw: &str, now: u64) -> Result<u64, set!(Malformed, Expired, BadSignature)> {
    let Some((body, sig)) = raw.split_once('.') else {
        return err!(Malformed);
    };

    try_!(verify(sig));

    let expiry = try_!(body.parse::<u64>().map_err(|_| Malformed));

    if expiry <= now {
        fail!(Expired);
    }

    Ok(expiry)
}

fn connect(
    raw: &str,
    now: u64,
    refuse: bool,
) -> Result<u64, set!(Refused, BadSignature, Expired, Malformed)> {
    let expiry = try_!(parse_token(raw, now));

    if refuse {
        fail!(Refused);
    }

    Ok(expiry)
}

fn connect_or_zero(raw: &str, now: u64) -> Result<u64, set!(Refused, BadSignature, Malformed)> {
    let expiry = catch!(connect(raw, now, false), |err| switch!(err;
        Expired => 0,
        else as rest => fail!(rest),
    ));
    Ok(expiry)
}

fn connect_counting(raw: &str, now: u64, refuse: bool) -> Result<u64, String> {
    let expiry = catch!(connect(raw, now, refuse), |err| switch!(err;
        Expired => 0,
        Refused as refused => return Err(refused.to_string()),
        else as rest => return Err(format!("other: {rest}")),
    ));
    Ok(expiry)
}

fn connect_fully_handled(raw: &str, now: u64) -> Result<u64, set!()> {
    let expiry = catch!(connect(raw, now, false), |err| switch!(err;
        Expired => 0,
        Refused => 1,
        BadSignature => 2,
        Malformed => 3,
    ));
    Ok(expiry)
}

fn connect_logged(raw: &str, now: u64) -> u64 {
    let mut log = Vec::new();
    let expiry = catch!(connect(raw, now, false), |err| {
        log.push(err.to_string());
        u64::MAX
    });
    assert_eq!(log.is_empty(), expiry != u64::MAX);
    expiry
}

fn parse_or_default(raw: &str) -> u64 {
    catch!(raw.parse::<u64>(), |_err| 7)
}

#[test]
fn declared_messages() {
    assert_eq!(Malformed.to_string(), "token is malformed");
    assert_eq!(NotFound.to_string(), "page not found (404)");
    assert_eq!(Unauthorized.to_string(), "status 404");
    assert_eq!(Expired.to_string(), "Expired");
    assert!(Expired.source().is_none());
    assert_eq!(Expired, Expired.clone());
}

#[test]
fn set_type_macro() {
    let e: set!(Malformed, Expired) = ErrorSet::new(Expired);
    assert_eq!(e.to_string(), "Expired");
    let _empty: Option<set!()> = None;
}

#[test]
fn try_err_and_fail() {
    assert_eq!(parse_token("9.ok", 1).unwrap(), 9);
    assert_eq!(
        parse_token("nodot", 1).unwrap_err().to_string(),
        "token is malformed"
    );
    assert_eq!(parse_token("1.ok", 5).unwrap_err().to_string(), "Expired");
    assert_eq!(
        connect("9.bad", 1, false).unwrap_err().to_string(),
        "BadSignature"
    );
    assert_eq!(
        connect("9.ok", 1, true).unwrap_err().to_string(),
        "connection refused"
    );
}

#[test]
fn catching_and_switching() {
    assert_eq!(connect_or_zero("1.ok", 5).unwrap(), 0);
    assert_eq!(
        connect_or_zero("x.ok", 5).unwrap_err().to_string(),
        "token is malformed"
    );
    assert_eq!(connect_counting("1.ok", 5, false).unwrap(), 0);
    assert_eq!(
        connect_counting("9.ok", 1, true).unwrap_err(),
        "connection refused"
    );
    assert_eq!(
        connect_counting("9.bad", 1, false).unwrap_err(),
        "other: BadSignature"
    );
    assert_eq!(connect_fully_handled("1.ok", 5).unwrap(), 0);
    assert_eq!(connect_fully_handled("9.bad", 1).unwrap(), 2);
    assert_eq!(connect_fully_handled("x.ok", 1).unwrap(), 3);
    assert_eq!(connect_logged("9.ok", 1), 9);
    assert_eq!(connect_logged("1.ok", 5), u64::MAX);
    assert_eq!(parse_or_default("12"), 12);
    assert_eq!(parse_or_default("nope"), 7);
}
