//! # arg_enum_proc_macro
//!
//! This crate consists in a procedural macro derive that provides the
//! same implementations that clap the [`clap::arg_enum`][1] macro provides:
//! [`std::fmt::Display`], [`std::str::FromStr`] and a `variants()` function.
//!
//! By using a procedural macro it allows documenting the enum fields
//! correctly and avoids the requirement of expanding the macro to use
//! the structure with [cbindgen](https://crates.io/crates/cbindgen).
//!
//! [1]: https://docs.rs/clap/2.32.0/clap/macro.arg_enum.html
//!

#![recursion_limit = "128"]

extern crate proc_macro;

use proc_macro2::{Literal, Punct, Span, TokenStream, TokenTree};
use quote::{quote, quote_spanned};
use std::iter::FromIterator;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parenthesized, parse2, parse_macro_input, Data, DeriveInput, Ident, LitStr, Token};

#[derive(Debug)]
enum ArgEnumAttr {
    Alias(Literal),
}

impl Parse for ArgEnumAttr {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        use self::ArgEnumAttr::*;
        let name: Ident = input.parse()?;
        let name_str = name.to_string();

        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;

            match name_str.as_ref() {
                "alias" => {
                    let alias: LitStr = input.parse()?;
                    Ok(Alias(Literal::string(&alias.value())))
                }
                _ => panic!("unexpected attribute {}", name_str),
            }
        } else {
            panic!("unexpected attribute: {}", name_str)
        }
    }
}

#[derive(Debug)]
struct ArgEnumAttrs {
    paren_token: syn::token::Paren,
    attrs: Punctuated<ArgEnumAttr, Token![,]>,
}

impl Parse for ArgEnumAttrs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let content;

        Ok(ArgEnumAttrs {
            paren_token: parenthesized!(content in input),
            attrs: content.parse_terminated(ArgEnumAttr::parse)?,
        })
    }
}

/// Implement [`std::fmt::Display`], [`std::str::FromStr`] and `variants()`.
///
/// The invocation:
/// ``` no_run
/// use arg_enum_proc_macro::ArgEnum;
///
/// #[derive(ArgEnum)]
/// enum Foo {
///     A,
///     #[arg_enum(alias = "Bar")]
///     B
/// }
/// ```
///
/// produces:
/// ``` no_run
/// enum Foo {
///     A,
///     B
/// }
/// impl ::std::str::FromStr for Foo {
///     type Err = String;
///
///     fn from_str(s: &str) -> ::std::result::Result<Self,Self::Err> {
///         match s {
///             "A" | _ if s.eq_ignore_ascii_case("A") => Ok(Foo::A),
///             "B" | _ if s.eq_ignore_ascii_case("B") => Ok(Foo::B),
///             "Bar" | _ if s.eq_ignore_ascii_case("Bar") => Ok(Foo::B),
///             _ => Err({
///                 let v = vec![ "A", "B", "Bar" ];
///                 format!("valid values: {}", v.join(" ,"))
///             }),
///         }
///     }
/// }
/// impl ::std::fmt::Display for Foo {
///     fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
///         match *self {
///             Foo::A => write!(f, "A"),
///             Foo::B => write!(f, "B"),
///         }
///     }
/// }
///
/// impl Foo {
///     /// Returns an array of valid values which can be converted into this enum.
///     #[allow(dead_code)]
///     pub fn variants() -> [&'static str; 3] {
///         [ "A", "B", "Bar" ]
///     }
/// }
/// ```
#[proc_macro_derive(ArgEnum, attributes(arg_enum))]
pub fn arg_enum(items: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(items as DeriveInput);

    let name = input.ident;
    let variants = if let Data::Enum(data) = input.data {
        data.variants
    } else {
        panic!("Only enum supported");
    };

    let all_variants: Vec<(TokenTree, &Ident)> = variants
        .iter()
        .flat_map(|item| {
            let id = &item.ident;
            let lit: TokenTree = Literal::string(&id.to_string()).into();
            let mut all_lits = vec![(lit, id)];

            item.attrs
                .iter()
                .filter(|attr| attr.path.is_ident("arg_enum"))
                // .flat_map(|attr| {
                .for_each(|attr| {
                    let attrs: ArgEnumAttrs = parse2(attr.tokens.clone()).unwrap();

                    for attr in attrs.attrs {
                        match attr {
                            ArgEnumAttr::Alias(alias) => all_lits.push((alias.into(), id)),
                        }
                    }
                });
            all_lits.into_iter()
        })
        .collect();

    let len = all_variants.len();

    let from_str_match = all_variants.iter().flat_map(|(lit, id)| {
        let pat: TokenStream = quote! {
            #lit | _ if s.eq_ignore_ascii_case(#lit) => Ok(#name::#id),
        }
        .into();

        pat.into_iter()
    });

    let from_str_match = TokenStream::from_iter(from_str_match);

    let display_match = variants.iter().flat_map(|item| {
        let id = &item.ident;
        let lit: TokenTree = Literal::string(&id.to_string()).into();

        let pat: TokenStream = quote! {
            #name::#id => write!(f, #lit),
        }
        .into();

        pat.into_iter()
    });

    let display_match = TokenStream::from_iter(display_match);

    let comma: TokenTree = Punct::new(',', proc_macro2::Spacing::Alone).into();
    let array_items = all_variants
        .iter()
        .flat_map(|(tok, _id)| vec![tok.clone(), comma.clone()].into_iter());

    let array_items = TokenStream::from_iter(array_items);

    let ret: TokenStream = quote_spanned! {
        Span::call_site() =>
        impl ::std::str::FromStr for #name {
            type Err = String;

            fn from_str(s: &str) -> ::std::result::Result<Self,Self::Err> {
                match s {
                    #from_str_match
                    _ => {
                        let values = vec![ #array_items ];

                        Err(format!("valid values: {}", values.join(" ,")))
                    }
                }
            }
        }
        impl ::std::fmt::Display for #name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                match *self {
                    #display_match
                }
            }
        }
        impl #name {
            /// Returns an array of valid values which can be converted into this enum.
            pub fn variants() -> [&'static str; #len] {
                [ #array_items ]
            }
        }
    }
    .into();

    ret.into()
}
