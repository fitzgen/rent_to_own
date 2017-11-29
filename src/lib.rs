/*!

[![](https://docs.rs/rent_to_own/badge.svg)](https://docs.rs/rent_to_own/) [![](https://img.shields.io/crates/v/rent_to_own.svg)](https://crates.io/crates/rent_to_own) [![](https://img.shields.io/crates/d/rent_to_own.png)](https://crates.io/crates/rent_to_own) [![Build Status](https://travis-ci.org/fitzgen/rent_to_own.png?branch=master)](https://travis-ci.org/fitzgen/rent_to_own)

`RentToOwn<T>`: A wrapper type for optionally giving up ownership of the
underlying value.

`RentToOwn<T>` is useful in situations where

1. a function might want to *conditionally take ownership* of some `T`
value, and

2. that function cannot take the `T` by value and return an `Option<T>` to maybe
give the `T` value back if it doesn't want ownership.

`RentToOwn<T>` dereferences (immutably and mutably) to its inner `T` value, and
additionally provides a `take` method that gives up ownership of the inner value
to the caller.

Under the covers, `RentToOwn<T>` is essentially an `Option<T>` that gets
unwrapped when dereferenced and calls `Option::take` if we need to take
ownership of the inner value. The key advantage over using `Option<T>` directly,
other than the `Deref` sugar, is some lifetime trickery to statically prevent
all unwrapping panics that would arise from using the `RentToOwn<T>` wrapper
again after the inner value has been taken. Once the inner value is taken, the
borrow checker will ensure that the original `RentToOwn<T>` cannot be used
anymore. See the `take` method's documentation for details.

## Example

In this example, if the `configure` function encounters any errors, we do not
wish to drop the `BigExpensiveResource`, but instead allow the caller to handle
the error and then reuse the resource. In effect, the `configure` function is
conditionally taking ownership of the `BigExpensiveResource` depending on if
there are IO errors or not.

```
use rent_to_own::RentToOwn;

use std::io::{self, Read};
use std::fs;

/// This is a big, expensive to create (or maybe even unique) resource, and we
/// want to reuse it even if `configure` returns an error.
struct BigExpensiveResource {
    // ...
}

#[derive(Default)]
struct Config {
    // ...
}

/// A big, expensive resource that has been properly configured.
struct ConfiguredResource {
    resource: BigExpensiveResource,
    config: Config,
}

fn read_and_parse_config_file() -> io::Result<Config> {
    // ...
#   Ok(Config {})
}

fn configure<'a>(
    resource: &'a mut RentToOwn<'a, BigExpensiveResource>
) -> io::Result<ConfiguredResource> {
    // We use normal error propagation with `?`. Because we haven't `take`n the
    // resource out of the `RentToOwn`, if we early return here the caller still
    // controls the `BigExpensiveResource` and it isn't dropped.
    let config = read_and_parse_config_file()?;

    // Now we `take` ownership of the resource and return the configured
    // resource.
    let resource = resource.take();
    Ok(ConfiguredResource { resource, config })
}
```

What does `configure`'s caller look like? It calls `RentToOwn::with` to
construct the `RentToOwn<BigExpensiveResource>` and invoke a closure with
it. Then it inspects the results of the closure and whether the
`BigExpensiveResource` was taken or not.

In this example, the caller can recover from any IO error when reading or
parsing the configuration file and use a default configuration with the
`BigExpensiveResource` instead.

```
# use rent_to_own::RentToOwn;
# struct BigExpensiveResource;
# impl BigExpensiveResource { fn reconstruct() -> Self { BigExpensiveResource } }
# #[derive(Default)]
# struct Config;
# struct ConfiguredResource {
#     resource: BigExpensiveResource,
#     config: Config,
# }
# fn configure<'a>(
#     resource: &'a mut RentToOwn<'a, BigExpensiveResource>
# ) -> ::std::io::Result<ConfiguredResource> {
#     unimplemented!()
# }
fn use_custom_configuration_or_default(resource: BigExpensiveResource) -> ConfiguredResource {
    // We pass the resource into `with` and it constructs the `RentToOwn`
    // wrapper around it and then gives the wrapper to the closure. Finally, it
    // returns a pair of an `Option<BigExpensiveResource>` which is `Some` if
    // the closure took ownership and `None` if it did not, and the closure's
    // return value.
    let (resource, result) = RentToOwn::with(resource, |resource| {
        configure(resource)
    });

    if let Ok(configured) = result {
        return configured;
    }

    // Reuse the resource if the closure did not take ownership or else
    // reconstruct it if the closure did take ownership. (In this particular
    // example, we know that `configure` took ownership if and only if the
    // result was `Ok`, but that doesn't hold for all possible examples.)
    // Finally, return the configured resource with the default configuration.
    let resource = resource.unwrap_or_else(|| BigExpensiveResource::reconstruct());
    let config = Config::default();
    ConfiguredResource { resource, config }
}
```

 */

#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

use std::ops::{Deref, DerefMut};

/// A wrapper around a `T` that allows users to conditionally take ownership of
/// the inner `T` value, or simply use it like a `&mut T` reference.
///
/// See the module documentation for details and examples.
#[derive(Debug, Hash)]
pub struct RentToOwn<'a, T: 'a> {
    inner: &'a mut Option<T>,
}

impl<'a, T> Deref for RentToOwn<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.inner.as_ref().unwrap()
    }
}

impl<'a, T> DerefMut for RentToOwn<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.inner.as_mut().unwrap()
    }
}

impl<'a, T: 'a> RentToOwn<'a, T> {
    /// Give the function `f` the option to take ownership of `inner`.
    ///
    /// That is, create a `RentToOwn` from the given `inner` value and then
    /// invoke the function `f` with it.
    ///
    /// The return value is a pair of:
    ///
    /// 1. If the closure took ownership of the inner value, `None`, otherwise
    /// `Some(inner)`.
    ///
    /// 2. The value returned by the closure.
    ///
    /// See the module level documentation for details and examples.
    pub fn with<F, U>(inner: T, f: F) -> (Option<T>, U)
    where
        F: for<'b> FnOnce(&'b mut RentToOwn<'b, T>) -> U,
    {
        let mut inner = Some(inner);
        let u = {
            let mut me = RentToOwn { inner: &mut inner };
            f(&mut me)
        };
        (inner, u)
    }
}

impl<'a, T> RentToOwn<'a, T> {
    /// Take ownership of the inner `T` value.
    ///
    /// Note that the lifetime on the `self` reference forces the mutable borrow
    /// to last for the rest of the `RentToOwn`'s existence. This "tricks" the
    /// borrow checker into statically disallowing use-after-take, which would
    /// otherwise result in a panic if you were using `Option<T>` instead of
    /// `RentToOwn<T>`.
    ///
    /// ```compile_fail
    /// use rent_to_own::RentToOwn;
    ///
    /// struct Thing(usize);
    ///
    /// fn use_after_take<'a>(outer: &'a mut RentToOwn<'a, Thing>) {
    ///     // Take ownership of the inner value, moving it out of the
    ///     // `RentToOwn`.
    ///     let inner = outer.take();
    ///
    ///     let inner_val = inner.0;
    ///     println!("inner's value is {}", inner_val);
    ///
    ///     // An attempt to use the `RentToOwn` again (via deref) after its
    ///     // value has already been taken!
    ///     let outer_val = outer.0;
    ///     println!("outer's value is {}", outer_val);
    /// }
    /// ```
    ///
    /// Attempting to compile that example results in a compilation error:
    ///
    /// ```text
    ///	error[E0502]: cannot borrow `*outer` as immutable because it is also borrowed as mutable
    ///    --> src/lib.rs:18:21
    ///    |
    /// 11 |     let inner = outer.take();
    ///    |                 ----- mutable borrow occurs here
    /// ...
    /// 18 |     let outer_val = outer.0;
    ///    |                     ^^^^^ immutable borrow occurs here
    /// 19 |     println!("outer's value is {}", outer_val);
    /// 20 | }
    ///    | - mutable borrow ends here
    /// ```
    pub fn take(&'a mut self) -> T {
        self.inner.take().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::RentToOwn;

    #[test]
    fn it_derefs() {
        RentToOwn::with(5, |x| {
            assert_eq!(**x, 5);
        });
    }

    #[test]
    fn it_derefs_mut() {
        RentToOwn::with(5, |x| {
            **x = 6;
            assert_eq!(**x, 6);
        });
    }

    #[test]
    fn it_takes() {
        RentToOwn::with(5, |x| {
            assert_eq!(x.take(), 5);
        });
    }

    #[test]
    fn with_returns_closures_result() {
        let (_, x) = RentToOwn::with(5, |_| 9);
        assert_eq!(x, 9);
    }

    #[test]
    fn with_gives_back_untaken_ownership() {
        let (orig, _) = RentToOwn::with(5, |_| {});
        assert_eq!(orig, Some(5));
    }

    #[test]
    fn with_does_not_give_back_taken_ownership() {
        let (orig, _) = RentToOwn::with(5, |x| x.take());
        assert!(orig.is_none());
    }
}
