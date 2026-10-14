use errset::{ErrorSet, errors, set, try_};
use std::{error::Error, io};

errors! { pub Expired = "expired"; }

fn inner(fail: bool) -> Result<u8, set!(io::Error, Expired)> {
    if fail {
        Err(ErrorSet::new(Expired))
    } else {
        Ok(try_!(Ok::<u8, io::Error>(1)))
    }
}

fn boxed(fail: bool) -> Result<u8, Box<dyn Error + Send + Sync>> {
    Ok(inner(fail)?)
}

fn assert_send_sync<T: Send + Sync + 'static>() {}

#[test]
fn plain_question_mark_into_boxed_errors() {
    assert_eq!(boxed(false).unwrap(), 1);
    assert_eq!(boxed(true).unwrap_err().to_string(), "expired");
    assert_send_sync::<set!(io::Error, Expired)>();
}
