//! Unboxed, type-keyed error sets.
//!
//! `ErrorSet<(io::Error, ParseIntError)>` holds exactly one of the listed error types,
//! stored inline as a generated enum. Membership, narrowing, and widening are resolved at
//! compile time through position indices that inference solves.
//!
//! ```
//! use errset::{catch, errors, fail, set, switch, try_};
//!
//! errors! {
//!     pub Malformed = "token is malformed";
//!     pub Expired = "token has expired";
//! }
//!
//! fn parse(raw: &str) -> Result<u64, set!(Malformed, Expired)> {
//!     let expiry = try_!(raw.parse::<u64>().map_err(|_| Malformed));
//!     if expiry == 0 {
//!         fail!(Expired);
//!     }
//!     Ok(expiry)
//! }
//!
//! fn expiry_or_default(raw: &str) -> u64 {
//!     catch!(parse(raw), |err| switch!(err;
//!         Malformed => 1,
//!         Expired => 2,
//!     ))
//! }
//!
//! assert_eq!(expiry_or_default("30"), 30);
//! assert_eq!(expiry_or_default("x"), 1);
//! assert_eq!(expiry_or_default("0"), 2);
//! assert_eq!(parse("0").unwrap_err().to_string(), "token has expired");
//! ```

#![no_std]

use core::error::Error;
use core::fmt;
use core::hash::{Hash, Hasher};

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

mod sealed {
    pub trait Sealed {}

    /// Unnameable outside the crate. Required as a method argument by the conversion traits,
    /// so only this crate can implement them.
    pub struct Token;
}

/// A tuple of error types usable as the parameter of [`ErrorSet`].
///
/// Implemented for `()` and tuples of up to 16 types. Sealed.
///
/// Members should be distinct types. A set that names the same type twice compiles, but
/// converting into it, narrowing it, or switching on it fails with "type annotations
/// needed" (E0283) listing the colliding positions; an explicit index such as
/// `ErrorSet::new::<_, idx::I1>(error)` selects one. In generic code the positions are
/// resolved before substitution, so `ErrorSet<(T, io::Error)>` with `T = io::Error` keeps two
/// separate slots.
pub trait Set: sealed::Sealed {
    /// The inline enum holding one member of the set.
    type Enum;
}

/// `T` is a member of `Self` at position `Idx`. Sealed.
#[diagnostic::on_unimplemented(message = "`{T}` is not a member of the error set `{Self}`")]
pub trait Contains<T, Idx>: Set {
    fn inject(value: T) -> Self::Enum;

    /// The member at position `Idx`, if `value` holds it.
    fn get(value: &Self::Enum) -> Option<&T>;

    #[doc(hidden)]
    fn __sealed(_: sealed::Token);
}

/// `T` can be split off `Self` at position `Idx`, leaving `Rest`. Sealed.
#[diagnostic::on_unimplemented(message = "`{T}` is not a member of the error set `{Self}`")]
pub trait Narrow<T, Idx>: Set {
    /// `Self` with `T` removed, other members in their original order.
    type Rest: Set;
    fn narrow(value: Self::Enum) -> Result<T, <Self::Rest as Set>::Enum>;

    #[doc(hidden)]
    fn __sealed(_: sealed::Token);
}

/// Every member of `Self` is a member of `Target`. Sealed.
#[diagnostic::on_unimplemented(
    message = "`{Self}` has a member that is not in the error set `{Target}`"
)]
pub trait SubsetOf<Target: Set, Idx>: Set {
    fn widen(value: Self::Enum) -> Target::Enum;

    #[doc(hidden)]
    fn __sealed(_: sealed::Token);
}

/// Type-level indices. They only appear in trait bounds and are always inferred.
pub mod idx {
    use core::marker::PhantomData;

    macro_rules! markers {
        ($($I:ident)*) => { $( #[doc(hidden)] pub struct $I; )* };
    }
    markers!(I0 I1 I2 I3 I4 I5 I6 I7 I8 I9 I10 I11 I12 I13 I14 I15);

    /// Selects the conversion of a single member error into a set.
    pub struct Member<I>(PhantomData<I>);
    /// Selects the conversion of a smaller set into a larger one.
    pub struct Subset<I>(PhantomData<I>);
}

/// One error out of the set `S`, stored inline without allocation.
///
/// `Display`, `Debug`, and `Error::source` forward to the held member.
pub struct ErrorSet<S: Set>(S::Enum);

impl<S: Set> ErrorSet<S> {
    /// Wraps a member error.
    pub fn new<E, I>(error: E) -> Self
    where
        S: Contains<E, I>,
    {
        ErrorSet(S::inject(error))
    }

    /// Splits off member `E`: `Ok(error)` if `self` holds an `E`, otherwise `Err` with the
    /// remaining members as a smaller set.
    pub fn narrow<E, I>(self) -> Result<E, ErrorSet<<S as Narrow<E, I>>::Rest>>
    where
        S: Narrow<E, I>,
    {
        S::narrow(self.0).map_err(ErrorSet)
    }

    /// The member `E`, if `self` holds one, without consuming the set.
    ///
    /// ```
    /// use errset::{set, ErrorSet};
    /// use std::{io, num::ParseIntError};
    ///
    /// let err: set!(io::Error, ParseIntError) = ErrorSet::new(io::Error::other("disk full"));
    /// assert_eq!(err.get::<io::Error, _>().unwrap().to_string(), "disk full");
    /// assert!(err.get::<ParseIntError, _>().is_none());
    /// assert!(err.is::<io::Error, _>());
    /// ```
    pub fn get<E, I>(&self) -> Option<&E>
    where
        S: Contains<E, I>,
    {
        S::get(&self.0)
    }

    /// Whether `self` holds the member `E`.
    pub fn is<E, I>(&self) -> bool
    where
        S: Contains<E, I>,
    {
        S::get(&self.0).is_some()
    }

    /// Converts into any set containing every member of `S`, in any order.
    pub fn widen<T: Set, I>(self) -> ErrorSet<T>
    where
        S: SubsetOf<T, I>,
    {
        ErrorSet(S::widen(self.0))
    }

    /// The inline enum, for exhaustive matching. Variant `Vk` holds the member at position
    /// `k` of `S`.
    pub fn into_enum(self) -> S::Enum {
        self.0
    }

    /// A reference to the inline enum, for matching without consuming the set.
    pub fn as_enum(&self) -> &S::Enum {
        &self.0
    }
}

impl<E> ErrorSet<(E,)> {
    /// The only possible member.
    pub fn into_single(self) -> E {
        match self.0 {
            Set1::V0(error) => error,
        }
    }
}

impl ErrorSet<()> {
    /// An empty set has no values, so this can never be called at runtime.
    pub fn unreachable(self) -> ! {
        match self.0 {}
    }
}

impl<S: Set> fmt::Debug for ErrorSet<S>
where
    S::Enum: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<S: Set> fmt::Display for ErrorSet<S>
where
    S::Enum: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<S: Set> Error for ErrorSet<S>
where
    S::Enum: Error,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0.source()
    }
}

impl<S: Set> Clone for ErrorSet<S>
where
    S::Enum: Clone,
{
    fn clone(&self) -> Self {
        ErrorSet(self.0.clone())
    }
}

impl<S: Set> PartialEq for ErrorSet<S>
where
    S::Enum: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<S: Set> Eq for ErrorSet<S> where S::Enum: Eq {}

impl<S: Set> Hash for ErrorSet<S>
where
    S::Enum: Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// Conversion into `ErrorSet<S>` from either a member error or a smaller set. Sealed.
/// The two sources use distinct index wrappers, so the blanket impls never overlap.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be converted into `ErrorSet<{S}>`",
    note = "the source must be a member of `{S}`, or an `ErrorSet` whose members are all in `{S}`"
)]
pub trait IntoSet<S: Set, Idx> {
    fn lift(self) -> ErrorSet<S>;

    #[doc(hidden)]
    fn __sealed(_: sealed::Token);
}

impl<E, S, I> IntoSet<S, idx::Member<I>> for E
where
    S: Contains<E, I>,
{
    fn lift(self) -> ErrorSet<S> {
        ErrorSet(S::inject(self))
    }

    fn __sealed(_: sealed::Token) {}
}

impl<Sub, S, I> IntoSet<S, idx::Subset<I>> for ErrorSet<Sub>
where
    Sub: SubsetOf<S, I>,
    S: Set,
{
    fn lift(self) -> ErrorSet<S> {
        ErrorSet(Sub::widen(self.0))
    }

    fn __sealed(_: sealed::Token) {}
}

/// Adds `.into_set()` to `Result`, converting its error into an `ErrorSet`.
///
/// Followed by `?`, the target set is inferred from the enclosing function's return type and
/// the member position from the error type; `?` itself only performs the identity
/// conversion.
pub trait ResultExt<T, E> {
    fn into_set<S: Set, I>(self) -> Result<T, ErrorSet<S>>
    where
        E: IntoSet<S, I>;
}

impl<T, E> ResultExt<T, E> for Result<T, E> {
    fn into_set<S: Set, I>(self) -> Result<T, ErrorSet<S>>
    where
        E: IntoSet<S, I>,
    {
        self.map_err(IntoSet::lift)
    }
}

/// Inline storage of the empty error set. It has no variants, so no value of it exists.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Set0 {}

impl sealed::Sealed for () {}

impl Set for () {
    type Enum = Set0;
}

impl fmt::Debug for Set0 {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

impl fmt::Display for Set0 {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

impl Error for Set0 {}

impl<Target: Set> SubsetOf<Target, ()> for () {
    fn widen(value: Set0) -> Target::Enum {
        match value {}
    }

    fn __sealed(_: sealed::Token) {}
}

/// Emits `Contains` and `Narrow` for each position of one arity. Walks the member list,
/// carrying the members already passed (`$DT $DV`) so the remainder can be formed from
/// those plus the members still ahead (`$RT $RV`).
macro_rules! positions {
    (@walk $Enum:ident [$($All:ident)*] [$($DT:ident $DV:ident)*]) => {};
    (@walk $Enum:ident [$($All:ident)*] [$($DT:ident $DV:ident)*]
        [$T:ident $V:ident $I:ident $J:ident] $([$RT:ident $RV:ident $RI:ident $RJ:ident])*
    ) => {
        impl<$($All),*> Contains<$T, $crate::idx::$I> for ($($All,)*) {
            fn inject(value: $T) -> Self::Enum {
                $Enum::$V(value)
            }

            #[allow(unreachable_patterns)]
            fn get(value: &Self::Enum) -> Option<&$T> {
                match value {
                    $Enum::$V(error) => Some(error),
                    _ => None,
                }
            }

            fn __sealed(_: sealed::Token) {}
        }

        impl<$($All),*> Narrow<$T, $crate::idx::$I> for ($($All,)*) {
            type Rest = ($($DT,)* $($RT,)*);

            fn narrow(value: Self::Enum) -> Result<$T, <Self::Rest as Set>::Enum> {
                match value {
                    $Enum::$V(error) => Ok(error),
                    $( $Enum::$DV(other) => Err(<Self::Rest as Contains<$DT, _>>::inject(other)), )*
                    $( $Enum::$RV(other) => Err(<Self::Rest as Contains<$RT, _>>::inject(other)), )*
                }
            }

            fn __sealed(_: sealed::Token) {}
        }

        positions!(@walk $Enum [$($All)*] [$($DT $DV)* $T $V] $([$RT $RV $RI $RJ])*);
    };
}

/// Emits the storage enum, the `Set` impl, the transparent formatting and error impls, the
/// `SubsetOf` impl, and every per-position impl for one tuple arity.
macro_rules! set_arity {
    ($Enum:ident $n:literal; $([$T:ident $V:ident $I:ident $J:ident])+) => {
        #[doc = concat!("Inline storage of an error set with ", $n, " members. Variant `Vk` holds the member at position `k` of the set's tuple.")]
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $Enum<$($T),+> {
            $( $V($T), )+
        }

        impl<$($T),+> sealed::Sealed for ($($T,)+) {}

        impl<$($T),+> Set for ($($T,)+) {
            type Enum = $Enum<$($T),+>;
        }

        impl<$($T: fmt::Debug),+> fmt::Debug for $Enum<$($T),+> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $( $Enum::$V(error) => fmt::Debug::fmt(error, f), )+
                }
            }
        }

        impl<$($T: fmt::Display),+> fmt::Display for $Enum<$($T),+> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    $( $Enum::$V(error) => fmt::Display::fmt(error, f), )+
                }
            }
        }

        impl<$($T: Error),+> Error for $Enum<$($T),+> {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                match self {
                    $( $Enum::$V(error) => error.source(), )+
                }
            }
        }

        impl<$($T,)+ Target: Set, $($J),+> SubsetOf<Target, ($($J,)+)> for ($($T,)+)
        where
            $( Target: Contains<$T, $J>, )+
        {
            fn widen(value: Self::Enum) -> Target::Enum {
                match value {
                    $( $Enum::$V(error) => <Target as Contains<$T, $J>>::inject(error), )+
                }
            }

            fn __sealed(_: sealed::Token) {}
        }

        positions!(@walk $Enum [$($T)+] [] $([$T $V $I $J])+);
    };
}

/// Calls `set_arity!` once per prefix of the member list, so one invocation covers every
/// arity from 1 up to the list length.
macro_rules! sets {
    (@prefix [$($done:tt)*]) => {};
    (@prefix [$($done:tt)*] $Enum:ident $n:literal $T:ident $V:ident $I:ident $J:ident, $($rest:tt)*) => {
        set_arity!($Enum $n; $($done)* [$T $V $I $J]);
        sets!(@prefix [$($done)* [$T $V $I $J]] $($rest)*);
    };
}

sets!(@prefix []
    Set1 "1" A V0 I0 J0,
    Set2 "2" B V1 I1 J1,
    Set3 "3" C V2 I2 J2,
    Set4 "4" D V3 I3 J3,
    Set5 "5" E V4 I4 J4,
    Set6 "6" F V5 I5 J5,
    Set7 "7" G V6 I6 J6,
    Set8 "8" H V7 I7 J7,
    Set9 "9" K V8 I8 J8,
    Set10 "10" L V9 I9 J9,
    Set11 "11" M V10 I10 J10,
    Set12 "12" N V11 I11 J11,
    Set13 "13" O V12 I12 J12,
    Set14 "14" P V13 I13 J13,
    Set15 "15" Q V14 I14 J14,
    Set16 "16" R V15 I15 J15,
);

/// An `Err` holding the given error converted into the `ErrorSet` the context expects.
/// Accepts a member error or a smaller set.
///
/// ```
/// use errset::{err, errors, set};
///
/// errors! {
///     pub Malformed;
///     pub Expired;
/// }
///
/// fn check(raw: &str) -> Result<(), set!(Malformed, Expired)> {
///     if raw.is_empty() {
///         return err!(Malformed);
///     }
///     if raw == "old" {
///         return err!(Expired);
///     }
///     Ok(())
/// }
///
/// assert!(check("fresh").is_ok());
/// assert_eq!(check("").unwrap_err().to_string(), "Malformed");
/// assert_eq!(check("old").unwrap_err().to_string(), "Expired");
/// ```
#[macro_export]
macro_rules! err {
    ($error:expr) => {
        ::core::result::Result::Err($crate::IntoSet::lift($error))
    };
}

/// Returns an error from the enclosing function, converting it into the function's
/// `ErrorSet`. Accepts a member error or a smaller set, like Zig's `return error.Name`.
///
/// ```
/// use errset::{catch, errors, fail, set};
///
/// errors! {
///     pub Malformed;
///     pub Expired;
/// }
///
/// fn check(raw: &str) -> Result<(), set!(Malformed)> {
///     if raw.is_empty() {
///         fail!(Malformed);
///     }
///     Ok(())
/// }
///
/// fn load(raw: &str) -> Result<(), set!(Malformed, Expired)> {
///     catch!(check(raw), |err| fail!(err));
///     if raw == "old" {
///         fail!(Expired);
///     }
///     Ok(())
/// }
///
/// assert_eq!(load("").unwrap_err().to_string(), "Malformed");
/// assert_eq!(load("old").unwrap_err().to_string(), "Expired");
/// ```
#[macro_export]
macro_rules! fail {
    ($error:expr) => {
        return $crate::err!($error)
    };
}

/// The `ErrorSet` type for a list of member error types.
///
/// `set!(Malformed, Expired)` is `ErrorSet<(Malformed, Expired)>`; `set!()` is the empty
/// set.
///
/// ```
/// use errset::{set, ErrorSet};
/// use std::{io, num::ParseIntError};
///
/// type LoadError = set!(io::Error, ParseIntError);
///
/// let err: ErrorSet<(io::Error, ParseIntError)> = LoadError::new("x".parse::<u8>().unwrap_err());
/// assert_eq!(err.to_string(), "invalid digit found in string");
///
/// let none: Option<set!()> = None;
/// assert!(none.is_none());
/// ```
#[macro_export]
macro_rules! set {
    ($($member:ty),* $(,)?) => {
        $crate::ErrorSet<($($member,)*)>
    };
}

/// Zig-style `try`: evaluates to the `Ok` value, or converts the error into the enclosing
/// function's `ErrorSet` and returns it. Accepts a member error or a smaller set.
/// Equivalent to `catch!(expr, |err| fail!(err))`.
///
/// ```
/// use errset::{errors, fail, set, try_};
/// use std::num::ParseIntError;
///
/// errors! {
///     pub Missing = "no port given";
/// }
///
/// fn port(raw: &str) -> Result<u16, set!(Missing, ParseIntError)> {
///     if raw.is_empty() {
///         fail!(Missing);
///     }
///     let port = try_!(raw.parse::<u16>());
///     Ok(port)
/// }
///
/// assert_eq!(port("8080").unwrap(), 8080);
/// assert_eq!(port("").unwrap_err().to_string(), "no port given");
/// assert!(port("http").is_err());
/// ```
#[macro_export]
macro_rules! try_ {
    ($result:expr) => {
        $crate::ResultExt::into_set($result)?
    };
}

/// Zig-style `catch`: evaluates to the `Ok` value, or binds the error and evaluates the
/// handler body in the enclosing function. The body must produce the `Ok` type or diverge,
/// so it can `return`, use `?`, `fail!`, or `switch!` on the error.
///
/// ```
/// use errset::{catch, errors, fail, set, switch};
///
/// errors! {
///     pub Malformed;
///     pub Expired;
///     pub Revoked;
/// }
///
/// fn parse(raw: &str) -> Result<u64, set!(Malformed, Expired, Revoked)> {
///     match raw {
///         "expired" => fail!(Expired),
///         "revoked" => fail!(Revoked),
///         _ => raw.parse().or_else(|_| fail!(Malformed)),
///     }
/// }
///
/// fn expiry(raw: &str) -> Result<u64, set!(Malformed, Revoked)> {
///     let expiry = catch!(parse(raw), |err| switch!(err;
///         Expired => 0,
///         else as rest => fail!(rest),
///     ));
///     Ok(expiry)
/// }
///
/// let port = catch!("http".parse::<u16>(), |_err| 8080);
/// assert_eq!(port, 8080);
///
/// assert_eq!(expiry("60").unwrap(), 60);
/// assert_eq!(expiry("expired").unwrap(), 0);
/// assert_eq!(expiry("revoked").unwrap_err().to_string(), "Revoked");
/// ```
#[macro_export]
macro_rules! catch {
    ($result:expr, |$err:pat_param| $body:expr $(,)?) => {
        match $result {
            ::core::result::Result::Ok(value) => value,
            ::core::result::Result::Err($err) => $body,
        }
    };
}

/// Zig-style `switch` over an `ErrorSet`, keyed by member type.
///
/// Each arm names a member type, optionally binds it with `as`, and evaluates to the
/// switch's result type or diverges. An `else` arm receives the remaining members as a
/// smaller set. Without `else`, every member must have an arm; a missing member is a compile
/// error naming the members left over.
///
/// ```
/// use errset::{errors, set, switch, ErrorSet};
/// use std::io;
///
/// errors! {
///     pub Expired;
///     pub Refused = "connection refused";
/// }
///
/// fn describe(err: set!(io::Error, Expired, Refused)) -> String {
///     switch!(err;
///         io::Error as e => format!("io: {:?}", e.kind()),
///         Expired => "expired".to_string(),
///         else as rest => rest.to_string(),
///     )
/// }
///
/// assert_eq!(describe(ErrorSet::new(Expired)), "expired");
/// assert_eq!(describe(ErrorSet::new(Refused)), "connection refused");
/// assert_eq!(describe(ErrorSet::new(io::Error::from(io::ErrorKind::NotFound))), "io: NotFound");
/// ```
///
/// Leaving out a member without an `else` arm is a compile error:
///
/// ```compile_fail
/// use errset::{errors, set, switch, ErrorSet};
///
/// errors! {
///     pub Expired;
///     pub Refused;
/// }
///
/// let err: set!(Expired, Refused) = ErrorSet::new(Expired);
/// let code = switch!(err;
///     Expired => 1,
/// );
/// ```
#[macro_export]
macro_rules! switch {
    ($set:expr; $($arms:tt)*) => {{
        let set = $set;
        $crate::__switch_arms!(set; $($arms)*)
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __switch_arms {
    ($set:ident; $(,)?) => {
        $crate::ErrorSet::<()>::unreachable($set)
    };
    ($set:ident; else $(as $bind:pat)? => $body:expr $(,)?) => {{
        let _rest = $set;
        $( let $bind = _rest; )?
        $body
    }};
    ($set:ident; $member:ty $(as $bind:pat)? => $body:expr $(, $($rest:tt)*)?) => {
        match $set.narrow::<$member, _>() {
            ::core::result::Result::Ok(_caught) => {
                $( let $bind = _caught; )?
                $body
            }
            ::core::result::Result::Err(rest) => $crate::__switch_arms!(rest; $($($rest)*)?),
        }
    };
}

/// Declares payload-free error types, one per entry, each optionally followed by
/// `= "message"` and extra format arguments. Without a message, `Display` prints the type
/// name. Each type derives `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq` and `Hash`, and
/// implements `Error`.
///
/// ```
/// use errset::errors;
///
/// const CODE: u32 = 404;
///
/// errors! {
///     /// The token text could not be parsed.
///     pub Malformed = "token is malformed";
///     pub NotFound = "page not found ({})", CODE;
///     pub Gone = "status {CODE}";
///     pub Expired;
/// }
///
/// assert_eq!(Malformed.to_string(), "token is malformed");
/// assert_eq!(NotFound.to_string(), "page not found (404)");
/// assert_eq!(Gone.to_string(), "status 404");
/// assert_eq!(Expired.to_string(), "Expired");
/// ```
#[macro_export]
macro_rules! errors {
    () => {};
    ($(#[$attr:meta])* $vis:vis $name:ident $(= $msg:literal $(, $arg:expr)*)? ; $($rest:tt)*) => {
        $(#[$attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis struct $name;

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                $crate::__error_message!(f, $name $(, $msg $(, $arg)*)?)
            }
        }

        impl ::core::error::Error for $name {}

        $crate::errors!($($rest)*);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __error_message {
    ($f:ident, $name:ident) => {
        $f.write_str(::core::stringify!($name))
    };
    ($f:ident, $name:ident, $msg:literal $(, $arg:expr)*) => {
        $f.write_fmt(::core::format_args!($msg $(, $arg)*))
    };
}
