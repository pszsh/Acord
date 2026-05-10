# Acord

Here's the 'sales' pitch:

"Hi there, do you enjoy casually solving project euler problems and are tired of using the spotlight bar as your primary calculator?" - Then this might be your kinda thing.

First, but least importantly, Acord is a native markdown editor... with a 'small' expression language baked in. 

- Lines starting with `/=` evaluate — the result is printed inline below it. 
- Tables are real, editable, and can carry formulas. 
- Runs on Rust + iced + wgpu in a Swift shell on macOS.
- Sports my wicked cool Rust DSL, Cordial. Trust me, you'll love it. It's like Rust but you never have to remember any semi-colons. Don't panic - indentation, case, these things don't matter much either. It's a sensible, salad days sort of syntax. 

The war - against the evil, as always.
Here's an analogy. Against the evil that is unsavory programming languages and — things... those things I don't like. Just generally, "things," sucky things. Evil.
Against this evil, let's say Rust is the best weapon made by humankind. It's what you need, it's the truth, it is the goodness within us all.
Python sure wanted to help, and golly, in its own ways it has. It was something like... the hand grenade. Effective at the start of the war, devastating it both its sweeping victories and its massive shortcomings. But I am grateful nonetheless, they did the trick when they were most needed. Not evil. Just bad. Normal bad. If the enemy is evil and the other side is good, that makes warfare `just` bad. This is just an analogy. I'm no expert on wars, or analogies for that matter. So bear with me.
Well, assuming everything I have said is true and just, then this, 
- Cordial -
...would be like a garden hoe. Sure, you'll find plenty of them in a war. But that's just cuz lots of the country is farmland. Sure, you could do some damage with it. But mostly, its a garden hoe. It just does its job. Its no swiss army knife, nor scalpel, or even X-acto. It's the pocket knife you have, the one in your hand cuz there's an apple to cut and there's no need to get out the good knife, both the tool and the tool-be'd are already in hand, they'll handle it... man.

Happy? Me too.

![Screenie](https://git.else-if.org/jess/Acord/raw/branch/main/Home.png)

## Quickstart

- Download a pre-compiled binary from [Releases](../../../releases)

- Or build from source — one command, picks the right script for your platform:

```bash
cargo xtask install
```

On macOS this drops a release `.app` into `/Applications`. On Linux it installs the binary into `~/.local/bin`, drops a `.desktop` entry into `~/.local/share/applications`, and registers the icon. On Windows it builds the release exe.

Other commands:

```bash
cargo xtask build              # release build only
cargo xtask debug              # debug build, foreground launch
cargo xtask package --all      # cross-compile + zip all six distributables
```

Append `-macos`, `-windows`, or `-linux` to force a platform (e.g. `cargo xtask build-universal-windows`).

On Linux, both x11 and wayland backends are linked into the binary by default.
Force one with `ACORD_FEATURES=wayland cargo xtask build`

Cross-compile uses [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild) (zig as the cross-linker) for the windows/linux targets — no Docker, no VMs. One-time setup:

```bash
brew install zig librsvg
cargo install cargo-zigbuild
```

Or build a single target: `cargo xtask package --target windows-aarch64`.

Or (on linux only) you can use the nix shell:

```bash
cd Acord
nix-build && ./result
```

---


I built Acord because I needed something between vim and the IDE — not a webapp (VS Code), not a stack of plugins held together with hope. A real, focused, native tool for thinking in. Notes I write turn into running calculators turn into working code, all in the same file.

## What you can do here

Math

```cordial
let m = 100
let v = 9.8
/= 0.5 * m * v^2
    → 4802
```

With `use spice` enabled, write with units and SPICE conventions. 

```cordial
use spice

fn L(f: Hz, c: F) -> H {
    return 1 / (((2pi * f)^2) * c)
}

/= L(1000, 22n)
    → 1.15H
```

Take any function and invert it on a chosen variable in one line:

```cordial
let f0 = solve!(f, L)

/= f0(1.15H, 22n)
    → 1000
```

Tables work the way you'd expect from Numbers or Excel: cells, formulas, ranges, references between tables, mutation from text blocks. Headings carry scope, so a table under `### Budget` becomes `@Budget` from anywhere in the document.

## Where to start

- **Install** [Install](../../../wiki/Install) → [First Note](../../../wiki/First-Note)
- **Cordial Language** [Cordial Quickstart](../../../wiki/Cordial-Quickstart) → [Cordial Reference](../../../wiki/Cordial-Reference)
  - [Tables and formulas](../../../wiki/Cordial-Tables)
  - [SPICE notation for engineering values](../../../wiki/Cordial-SPICE)
  - [solve! for inverting functions](../../../wiki/Cordial-solve)
  - [Architecture](../../../wiki/Architecture) → [Contributing](../../../wiki/Contributing)

## Status
    0.2.0 All platforms are fully supported. There's also been quite a bit of polish since 0.1.0.
    0.1.0 First Beta release. Primarily MacOS supported.

Now, I'll be honest. I'm writing this first and foremost for myself, and I am a MacOS user. But I have been a Linux user, and I have been a Windows user, and I think it's important to aim to make things accessible for everyone. So with that said, if you come across a platform specific bug, please let me know. I probably just don't realize it exists, and I will fix it. MacOS is the best supported by default but not by design or intent. 
