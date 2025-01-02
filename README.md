# rustybug

I've purchased the early access release of [Building a Debugger](https://nostarch.com/building-a-debugger).
And as a result, I'm implementing it in Rust. The original blog series was very
helpful in wrangling ptrace and making [cargo-tarpaulin](https://github.com/xd009642/tarpaulin)
more reliable and I'm hoping this even more comprehensive book will help me
nail down and solve even more issues!

I will try and crib from the tarpaulin implementation as much as possible, so
don't expect an accurate recreation of the book. I'm also too lazy to write a
DWARF parser so I'll be using object and gimli for parsing of ELF files and
DWARF tables.

## Tests

Haven't done anything except make some C programs, these are built with meson
because I'm a modern man with modern sensibilities.

```
cd tests/data/apps
meson setup build
cd build
meson compile
```

And you should see all of the programs. Then `cargo test` will run the debugger
a bit on some of these programs.

## Thoughts

So there's a few things I might want to do and because this is a less serious
project I won't be using issues yet to do them but just jot down some ideas
here.

* Generate tests and random interactions in C or rust using parsers like syn or [lang\_c](https://docs.rs/lang-c/0.15.1/lang_c/) to pick random breakpoint locations
* Deterministic Simulation Testing
* Maybe play around with things like time-travel debugging

## TODO

I have a diary of what I'm doing and what I'm planning it lives [here](TODO.md)
