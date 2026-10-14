# errset

Unboxed, type-keyed error sets for Rust: list the errors a function can return right in its signature, with no per-function error enum, no boxing and no allocation.

```rust
use errset::{catch, errors, fail, set, switch, try_};
use std::num::ParseIntError;

errors! {
    pub Missing = "no port given";
    pub Reserved = "port is reserved";
}

fn parse_port(raw: &str) -> Result<u16, set!(Missing, Reserved, ParseIntError)> {
    if raw.is_empty() {
        fail!(Missing);
    }
    let port = try_!(raw.parse::<u16>());
    if port < 1024 {
        fail!(Reserved);
    }
    Ok(port)
}

let port = catch!(parse_port("80"), |err| switch!(err;
    Missing => 8080,
    Reserved => 8443,
    ParseIntError => 8080,
));
assert_eq!(port, 8443);
```

`set!(Missing, Reserved, ParseIntError)` is an `ErrorSet<(Missing, Reserved, ParseIntError)>`: one value out of those three types, stored inline as a generated enum.

## Motivation

Rust's precise error handling has a naming problem. A `Token`, an `Endpoint`, a `Connection` and a `Cookie` each fail in their own ways, so precise signatures mean declaring `TokenCreateError`, `EndpointCreateError`, `ConnectionCreateError` and so on, each with its own `From` impls and `Display`, and another wrapper enum every time two of them meet. Most crates give up and use one crate-wide error enum, and every function then claims it can return errors it never does.

`errset` lets the signature say exactly what can fail, and nothing else has to be declared:

```rust
use errset::{errors, fail, set, try_};
use std::io;

errors! {
    pub Malformed = "token is malformed";
    pub Expired = "token has expired";
}

#[derive(Debug)]
struct Token(String);

fn parse_token(raw: &str) -> Result<Token, set!(Malformed, Expired)> {
    match raw.trim() {
        "" => fail!(Malformed),
        "expired" => fail!(Expired),
        token => Ok(Token(token.to_string())),
    }
}

fn load_token(path: &str) -> Result<Token, set!(io::Error, Malformed, Expired)> {
    let raw = try_!(std::fs::read_to_string(path));
    Ok(try_!(parse_token(&raw)))
}

assert_eq!(parse_token(" abc ").unwrap().0, "abc");
assert!(load_token("/does/not/exist").unwrap_err().is::<io::Error, _>());
```

## Features

- **Error sets as types.** `set!(A, B, C)` names a set of up to 16 error types. Order and membership are checked at compile time.
- **Inline storage.** The set is a plain enum the size of its largest member. No `Box`, no allocator; the crate is `#![no_std]` without `alloc`.
- **Zig-style propagation.** `try_!` propagates any member error or smaller set into the function's set, and `fail!` returns one.
- **Zig-style handling.** `catch!` handles an error inline and `switch!` matches on members by type, exhaustively.
- **Payload-free error declarations.** `errors!` declares unit error types with an optional message in one line each.
- **Named enums for the API boundary.** `named!` turns an ordinary enum declaration into one that any matching set converts into, so a library can use sets internally and expose a plain enum.
- **Interops well with others.** `ErrorSet` implements `Error` with transparent `Display` and `source()`, so plain `?` into `Box<dyn Error>`, `anyhow` or `eyre` works unchanged.

## Usage

```toml
[dependencies]
errset = "0.1"
```

### Declaring errors

`errors!` declares payload-free error types. Each derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq` and `Hash`, and implements `Error`. Without a message, `Display` prints the type name.

```rust
use errset::errors;

const CODE: u32 = 404;

errors! {
    /// The token text could not be parsed.
    pub Malformed = "token is malformed";
    pub NotFound = "page not found ({})", CODE;
    pub Expired;
}

assert_eq!(NotFound.to_string(), "page not found (404)");
assert_eq!(Expired.to_string(), "Expired");
```

Errors that carry data are ordinary types, written with `thiserror` or by hand, and mix freely into sets.

### Propagating

`try_!(expr)` evaluates to the `Ok` value, or converts the error into the enclosing function's set and returns it. It accepts a single member error or a whole smaller set, so sets widen automatically as errors move up the stack, in any order.

```rust
use errset::{errors, fail, set, try_};
use std::{io, num::ParseIntError};

errors! { pub Reserved = "port is reserved"; }

fn parse_port(raw: &str) -> Result<u16, set!(ParseIntError, Reserved)> {
    let port = try_!(raw.trim().parse::<u16>());
    if port < 1024 {
        fail!(Reserved);
    }
    Ok(port)
}

fn load_port(path: &str) -> Result<u16, set!(io::Error, Reserved, ParseIntError)> {
    let raw = try_!(std::fs::read_to_string(path));
    Ok(try_!(parse_port(&raw)))
}

assert!(load_port("/does/not/exist").unwrap_err().is::<io::Error, _>());
```

`fail!(error)` returns an error from the function, like Zig's `return error.Name`. `err!(error)` builds the `Err` value without returning, and `.into_set()` is the method form for use with `?` when the function returns a set.

### Handling

`catch!(expr, |err| body)` evaluates to the `Ok` value, or runs `body` with the error. The body is inlined in your function rather than being a closure, so it can `return`, use `?` or `fail!`.

`switch!` matches a set by member type. Without an `else` arm, every member needs an arm; `else as rest` receives the remaining members as a smaller set.

```rust
use errset::{catch, errors, fail, set, switch};

errors! {
    pub Malformed;
    pub Expired;
    pub Revoked;
}

fn parse(raw: &str) -> Result<u64, set!(Malformed, Expired, Revoked)> {
    match raw {
        "expired" => fail!(Expired),
        "revoked" => fail!(Revoked),
        _ => raw.parse().or_else(|_| fail!(Malformed)),
    }
}

fn expiry(raw: &str) -> Result<u64, set!(Malformed, Revoked)> {
    let expiry = catch!(parse(raw), |err| switch!(err;
        Expired => 0,
        else as rest => fail!(rest),
    ));
    Ok(expiry)
}

assert_eq!(expiry("expired").unwrap(), 0);
assert_eq!(expiry("revoked").unwrap_err().to_string(), "Revoked");
```

A missing arm is a compile error that names what remains:

```text
expected `ErrorSet<()>`, found `ErrorSet<(Revoked,)>`
```

### Inspecting and reshaping

```rust
use errset::{set, ErrorSet, Set2};
use std::{io, num::ParseIntError};

let err: set!(io::Error, ParseIntError) = ErrorSet::new(io::Error::other("disk full"));

assert!(err.is::<io::Error, _>());
assert_eq!(err.get::<io::Error, _>().unwrap().to_string(), "disk full");

match err.as_enum() {
    Set2::V0(io) => assert_eq!(io.kind(), io::ErrorKind::Other),
    Set2::V1(_) => unreachable!(),
}

let rest = err.narrow::<ParseIntError, _>().unwrap_err();
let wider: set!(ParseIntError, io::Error) = rest.widen();
assert!(wider.is::<io::Error, _>());
```

| Method | Does |
| --- | --- |
| `ErrorSet::new(e)` | Wraps a member error. |
| `get::<E, _>()` / `is::<E, _>()` | Inspects without consuming. |
| `narrow::<E, _>()` | Splits off `E`, returning `Ok(E)` or the remaining members as a smaller set. |
| `widen()` | Converts into any superset, in any order. |
| `into_enum()` / `as_enum()` | The underlying `SetN` enum, for positional matching. |
| `into_single()` | Unwraps a one-member set. |

### Named enums at the API boundary

Users of a library usually want a plain enum they can `match` on instead of an error set. `named!` takes an ordinary enum declaration whose variants each hold one error type, emits it exactly as written, docs and derives included, and connects it to error sets:

```rust
use errset::{errors, fail, named, set, try_};
use std::{io, num::ParseIntError};

errors! {
    pub Malformed = "token is malformed";
    pub Refused = "connection refused";
}

named! {
    /// Every error the library can return.
    #[derive(Debug, thiserror::Error)]
    pub enum LibraryError {
        /// The connection failed.
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

fn parse(raw: &str) -> Result<u16, set!(Malformed, ParseIntError)> {
    if raw.is_empty() {
        fail!(Malformed);
    }
    Ok(try_!(raw.parse::<u16>()))
}

pub fn connect(raw: &str) -> Result<u16, LibraryError> {
    let port = try_!(parse(raw));           // any subset, in any order
    if port == 0 {
        fail!(Refused);
    }
    std::fs::metadata("/srv/socket")?;      // plain ? for a single member type
    Ok(port)
}

assert!(matches!(connect(""), Err(LibraryError::Malformed(Malformed))));
assert!(matches!(connect("0"), Err(LibraryError::Refused(Refused))));
```

`named!` adds `From<T>` for each variant's type, an inherent `from_set` that accepts any set whose members are all variant types, and an inherent `into_set` back to the full set. `try_!`, `fail!` and `err!` convert sets and other `named!` enums into it. A set with a member the enum lacks is a compile error at the conversion:

```text
`ErrorSet<(Malformed, Expired)>` cannot be converted into `ApiError`
```

Several `named!` enums can coexist, such as a `ConnectError` and a broader `LibraryError`. Their relationship is never declared explicitly. Instead, each lists its variants, and one converts into another whenever its variant types are a subset, compile-time checked at every conversion.

### Interop

`ErrorSet` implements `Error`, and is `Send + Sync + 'static` whenever its members are, so it converts with plain `?` wherever a boxed or dynamic error is expected:

```rust
use errset::{errors, set, ErrorSet};
use std::error::Error;

errors! { pub Expired = "token has expired"; }

fn check() -> Result<(), set!(Expired)> {
    Err(ErrorSet::new(Expired))
}

fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    check()?;
    Ok(())
}

assert_eq!(run().unwrap_err().to_string(), "token has expired");
```

### Zero cost

```rust
use errset::{errors, set};
use std::io;

errors! { pub Malformed; pub Expired; }

assert_eq!(size_of::<Result<(), set!(Malformed, Expired)>>(), 1);
assert_eq!(size_of::<set!(io::Error, Malformed)>(), size_of::<Result<(), io::Error>>());
```

## Use cases

- **Libraries** that want precise per-function errors internally and a plain, documented enum at the API boundary, with `named!` doing the conversion.
- **Embedded and `no_std` code.** No allocator is required, and errors cost no more than the enum you would have written by hand.
- **Hot paths** where errors are frequent, such as parsers, protocol decoders and retry loops, where allocating per error adds up.
- **Code that wants exhaustiveness.** `switch!` makes the compiler check that every possible failure is handled.

## Limitations

- **Sets hold up to 16 members.**
- **Members should be distinct types.** A set naming the same type twice fails to infer at its first use; see the `Set` trait docs.
- **No context or backtraces.** Errors are exactly the values you put in.
- **Bare `?` cannot convert into a set.** `?` only calls `From`, and a generic `From` impl would need an inferred index that impls cannot carry. `try_!` is the minimal shim; plain `?` still works out of a set into boxed errors, and from a single member type into a `named!` enum.
- **`named!` variants each hold exactly one unnamed field of a distinct type,** and the enum cannot be generic.

## Background

`errset` is the result of a long-running effort to make precise errors in Rust ergonomic. It started with an anonymous struct library, moved on to anonymous enums keyed by variant name, with set types encoded at the type level and conversions solved by inference, and then explored Zig-style error set composition. Restricting the idea to error handling only, and keying members by type rather than by name, resulted in a small, predictable design implemented in stable Rust.

## Inspiration and prior art

`errset`'s design is inspired by three crates:

- [terrors](https://github.com/komora-io/terrors) introduced `OneOf<(A, B, C)>`: an open sum type keyed by a tuple of error types, with membership proven through inferred position indices, `narrow` to split one type off and `broaden` to move to a superset.
- [eros](https://github.com/mcmah309/eros) builds on that idea with `ErrorUnion`. It adds a catch-all `AnyError`, attached context and backtraces, and the method-before-`?` pattern (`.union()?`) that makes conversions work on stable Rust.
- [error_set](https://github.com/mcmah309/error_set), brought Zig's error sets to Rust as flattened named enums with generated conversions between subsets.

`errset` takes the tuple-keyed sets and inferred indices from terrors and eros, and the Zig-style vocabulary from error_set. However, where it's different is that terrors and eros store the error type-erased in a `Box`, while `errset` stores it inline as a plain enum. Boxing makes reshaping a set free and enables eros's catch-all type and diagnostics. Storing inline instead means no allocation, no `alloc` requirement, narrowing by `match` rather than downcast, and members that don't need to be `'static`.

| | errset | eros | terrors |
| --- | --- | --- | --- |
| Storage | Inline enum | `Box`ed, type-erased | `Box<dyn Any>` |
| Allocation per error | None | One | One |
| `no_std` | Without `alloc` | Requires `alloc` | No |
| Members must be `'static` | No | Yes | Yes |
| Narrowing | `match` | Downcast | Downcast |
| Catch-all error type | No | `AnyError` | No |
| Context and backtraces | No | Yes | No |
| Propagation | `try_!(expr)` | `.union()?`, `.widen()?` | `.map_err(OneOf::broaden)?` |
| Handling | `catch!`, `switch!`, `narrow` | `.recover()`, `narrow` | `narrow`, `subset` |
| Choose it for | Precise errors with zero overhead, embedded targets, hot paths | Applications and services that want rich diagnostics and untyped errors alongside typed ones | Minimal boxed open sum types |

## Minimum supported Rust version

Rust 1.81, for `core::error::Error`.
