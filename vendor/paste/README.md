Macros for all your token pasting needs
=======================================

[![Build Status](https://api.travis-ci.org/dtolnay/paste.svg?branch=master)](https://travis-ci.org/dtolnay/paste)
[![Latest Version](https://img.shields.io/crates/v/paste.svg)](https://crates.io/crates/paste)
[![Rust Documentation](https://img.shields.io/badge/api-rustdoc-blue.svg)](https://docs.rs/paste)

The nightly-only [`concat_idents!`] macro in the Rust standard library is
notoriously underpowered in that its concatenated identifiers can only refer to
existing items, they can never be used to define something new.

[`concat_idents!`]: https://doc.rust-lang.org/std/macro.concat_idents.html

This crate provides a flexible way to paste together identifiers in a macro,
including using pasted identifiers to define new items.

```toml
[dependencies]
paste = "0.1"
```

This approach works with any stable or nightly Rust compiler 1.30+.

## Pasting identifiers

There are two entry points, `paste::expr!` for macros in expression position and
`paste::item!` for macros in item position.

Within either one, identifiers inside `[<`...`>]` are pasted together to form a
single identifier.

```rust
// Macro in item position: at module scope or inside of an impl block.
paste::item! {
    // Defines a const called `QRST`.
    const [<Q R S T>]: &str = "success!";
}

fn main() {
    // Macro in expression position: inside a function body.
    assert_eq!(
        paste::expr! { [<Q R S T>].len() },
        8,
    );
}
```

## More elaborate examples

This program demonstrates how you may want to bundle a paste invocation inside
of a more convenient user-facing macro of your own. Here the `routes!(A, B)`
macro expands to a vector containing `ROUTE_A` and `ROUTE_B`.

```rust
const ROUTE_A: &str = "/a";
const ROUTE_B: &str = "/b";

macro_rules! routes {
    ($($route:ident),*) => {{
        paste::expr! {
            vec![$( [<ROUTE_ $route>] ),*]
        }
    }}
}

fn main() {
    let routes = routes!(A, B);
    assert_eq!(routes, vec!["/a", "/b"]);
}
```

The next example shows a macro that generates accessor methods for some struct
fields.

```rust
macro_rules! make_a_struct_and_getters {
    ($name:ident { $($field:ident),* }) => {
        // Define a struct. This expands to:
        //
        //     pub struct S {
        //         a: String,
        //         b: String,
        //         c: String,
        //     }
        pub struct $name {
            $(
                $field: String,
            )*
        }

        // Build an impl block with getters. This expands to:
        //
        //     impl S {
        //         pub fn get_a(&self) -> &str { &self.a }
        //         pub fn get_b(&self) -> &str { &self.b }
        //         pub fn get_c(&self) -> &str { &self.c }
        //     }
        paste::item! {
            impl $name {
                $(
                    pub fn [<get_ $field>](&self) -> &str {
                        &self.$field
                    }
                )*
            }
        }
    }
}

make_a_struct_and_getters!(S { a, b, c });

fn call_some_getters(s: &S) -> bool {
    s.get_a() == s.get_b() && s.get_c().is_empty()
}
```

<br>

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>
