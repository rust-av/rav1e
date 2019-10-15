# Procedural macro derive that mimics `arg_enum!` from [clap](https://clap.rs)

## Usage

In `Cargo.toml`:
``` toml
[dependencies]
arg_enum_proc_macro = "0.1"
```

In the rust code:
``` rust
use arg_enum_proc_macro::ArgEnum;

/// All the possible states of Foo
#[derive(ArgEnum)]
pub enum Foo {
    /// Initial state
    Unk,
    /// Foo is on
    On,
    /// Foo is off
    Off,
}
```

### Aliases

It is possible to express an alias using the attribute `arg_enum(alias = "AliasVariant")`.
The `FromStr` will map the "AliasVariant" string to the decorated enum variant:

``` rust
/// All the possible states of Foo
#[derive(ArgEnum)]
pub enum Foo {
    /// Initial state
    Unk,
    /// Foo is on
    #[arg_enum(alias = "Up")]
    On,
    /// Foo is off
    #[arg_enum(alias = "Down")]
    Off,
}
```
