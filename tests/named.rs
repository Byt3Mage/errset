use errset::{err, errors, fail, named, set, switch, try_, ErrorSet, ResultExt};
use std::{error::Error, io, num::ParseIntError};

errors! {
    pub Malformed = "token is malformed";
    pub Refused = "connection refused";
    pub Expired = "token has expired";
}

named! {
    /// Errors returned by connecting.
    #[derive(Debug, thiserror::Error)]
    pub enum ConnectError {
        /// The socket failed.
        #[error("i/o failure")]
        Io(#[source] io::Error),
        #[error(transparent)]
        Refused(Refused),
    }
}

named! {
    /// Every error the library can return.
    #[derive(Debug, thiserror::Error)]
    pub enum LibraryError {
        #[error("i/o failure")]
        Io(#[source] io::Error),
        #[error(transparent)]
        Parse(ParseIntError),
        #[error(transparent)]
        Malformed(Malformed),
        #[error(transparent)]
        Refused(Refused),
    }
}

fn parse(raw: &str) -> Result<u16, set!(ParseIntError, Malformed)> {
    if raw.is_empty() {
        fail!(Malformed);
    }
    Ok(try_!(raw.parse::<u16>()))
}

fn dial(port: u16) -> Result<(), ConnectError> {
    if port == 0 {
        fail!(Refused);
    }
    std::fs::metadata("/does/not/exist")?;
    Ok(())
}

fn connect(raw: &str) -> Result<u16, LibraryError> {
    let port = try_!(parse(raw));
    try_!(dial(port));
    Ok(port)
}

fn refuse_reordered() -> Result<(), LibraryError> {
    let set: set!(Refused, Malformed) = ErrorSet::new(Refused);
    fail!(set)
}

fn full_coverage() -> Result<(), set!(io::Error, ParseIntError, Malformed, Refused)> {
    Err(ErrorSet::new(Malformed))
}

fn from_full_coverage() -> Result<(), LibraryError> {
    try_!(full_coverage());
    Ok(())
}

fn named_into_set() -> Result<(), set!(Expired, Refused, io::Error)> {
    try_!(dial(0));
    Ok(())
}

fn explicit_into_set() -> Result<(), LibraryError> {
    parse("").into_set::<LibraryError, _>()?;
    Ok(())
}

fn via_err() -> Result<(), LibraryError> {
    err!(Refused)
}

#[test]
fn members_convert_with_from_and_plain_question_mark() {
    assert!(matches!(
        LibraryError::from(Malformed),
        LibraryError::Malformed(Malformed)
    ));
    let io: LibraryError = io::Error::other("reset").into();
    assert!(matches!(io, LibraryError::Io(_)));
    assert!(matches!(dial(80), Err(ConnectError::Io(_))));
}

#[test]
fn subsets_convert_in_any_order() {
    assert!(matches!(
        connect(""),
        Err(LibraryError::Malformed(Malformed))
    ));
    assert!(matches!(connect("x"), Err(LibraryError::Parse(_))));
    assert!(matches!(
        refuse_reordered(),
        Err(LibraryError::Refused(Refused))
    ));
    assert!(matches!(
        from_full_coverage(),
        Err(LibraryError::Malformed(Malformed))
    ));
    assert!(matches!(
        explicit_into_set(),
        Err(LibraryError::Malformed(Malformed))
    ));
    assert!(matches!(via_err(), Err(LibraryError::Refused(Refused))));

    let set: set!(Refused, io::Error) = ErrorSet::new(io::Error::other("reset"));
    assert!(matches!(LibraryError::from_set(set), LibraryError::Io(_)));
}

#[test]
fn named_enums_convert_into_larger_enums_and_sets() {
    assert!(matches!(connect("0"), Err(LibraryError::Refused(Refused))));
    assert!(matches!(connect("80"), Err(LibraryError::Io(_))));
    assert!(named_into_set().unwrap_err().is::<Refused, _>());
}

#[test]
fn round_trips_through_its_set() {
    let set = LibraryError::from(Refused).into_set();
    assert!(set.is::<Refused, _>());
    let code = switch!(set;
        io::Error => 0,
        ParseIntError => 1,
        Malformed => 2,
        Refused => 3,
    );
    assert_eq!(code, 3);
}

#[test]
fn derives_and_attributes_pass_through() {
    let e = connect("80").unwrap_err();
    assert_eq!(e.to_string(), "i/o failure");
    assert!(e.source().is_some());
    assert_eq!(
        LibraryError::from(Refused).to_string(),
        "connection refused"
    );
}
