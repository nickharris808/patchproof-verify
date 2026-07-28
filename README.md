# patchproof-verify

**Re-check a [patchproof](https://github.com/nickharris808/patchproof) certificate without running patchproof.**

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.74%2B-orange.svg)](Cargo.toml)
[![CI](https://github.com/nickharris808/patchproof-verify/actions/workflows/ci.yml/badge.svg)](https://github.com/nickharris808/patchproof-verify/actions/workflows/ci.yml)

> **▶ [Try the toolkit in your browser](https://huggingface.co/spaces/nickh007/hw-verify)** — no install.

## Why this exists

patchproof emits a certificate so a sceptic does not have to trust it. That claim was
only worth so much while the only checker was the same Python package that produced
the certificate: a proof you can check exclusively with the prover's own code is not
much of a proof.

This is a second implementation, in a different language, written against the format
rather than derived from the original. No solver, no Python, no shared code. It is
deliberately small enough to audit in one sitting — the arithmetic is multiply, add,
and compare.

**Both implementations are checked against the same 30 test vectors**, and a
disagreement fails the build in both repositories. Two verifiers that quietly
disagree would mean one is wrong and nobody knows which.

## Install

```bash
cargo install --git https://github.com/nickharris808/patchproof-verify
```

Or build from a checkout:

```bash
git clone https://github.com/nickharris808/patchproof-verify && cd patchproof-verify
cargo build --release        # ./target/release/patchproof-verify
```

## 30-second quickstart

```bash
# produce a certificate with the Python tool...
pip install git+https://github.com/nickharris808/patchproof@main
patchproof cert A -o a.json

# ...and check it with this one, which shares no code with it
patchproof-verify a.json
```

## Worked example

```console
$ patchproof-verify a.json
VERIFIED   replayed: combination is 1 <= 0, a contradiction; constraints match the
           canonical corrected-AND-NOT-safety forms of class A
$ echo $?
0
```

Now tamper with it — flip one multiplier from `1` to `2`:

```console
$ patchproof-verify tampered.json
REJECTED   variables did not cancel: index(1), size(-1)
$ echo $?
1
```

And a certificate whose arithmetic is perfect but which claims nothing:

```console
$ patchproof-verify unbound.json
UNVERIFIED arithmetic replays: the listed inequalities combine to 1 <= 0, a
           contradiction. But the certificate names no defect class, so there is
           nothing to check those inequalities against: this does NOT establish that
           any patch is complete.
$ echo $?
1
```

## The three outcomes, and why the middle one is not a pass

| Status | Meaning | Exit |
|---|---|---|
| `VERIFIED` | the arithmetic replays **and** the constraints are the claimed class's | 0 |
| `UNVERIFIED` | the arithmetic replays, but nothing binds it to a claim | 1 |
| `REJECTED` | the arithmetic fails, or the constraints are the wrong ones | 1 |

There are two questions here and only one of them is arithmetic:

1. *Do these inequalities, with these multipliers, combine to a contradiction?*
2. *Are these the inequalities of the patch being claimed?*

A certificate holding the single form `5 <= 0` answers (1) perfectly — `5 <= 0` is
false, so any system containing it is trivially infeasible — while saying nothing
whatever about anybody's patch. Checking (1) and printing `VERIFIED` would certify a
claim nobody established.

So the canonical `corrected AND NOT safety` forms for each defect class are **held by
this verifier**, not taken from the certificate, and the constraints are compared
against them. A class A certificate relabelled as class C is `REJECTED`.

## What it proves, and what it does not

**Proves:** the listed inequalities are exactly the constraints of the named defect
class, and they are jointly infeasible over the integers. By the affine form of
Farkas' lemma, that means no input satisfies `corrected_guard AND NOT safety` — every
violating input in that class is eliminated.

**Does not prove:** anything about your codebase. A defect class models a *guard*.
Getting from real C to a faithful class model is on you, and it is the step where
mistakes actually happen. See patchproof's
[SCOPE.md](https://github.com/nickharris808/patchproof/blob/main/SCOPE.md).

The parser is strict on purpose: it must consume the whole form, so `@@@ <= 0` is
rejected rather than read as the well-formed `0 <= 0` — a real bug the Python parser
once had, which let a certificate of pure nonsense verify. Integer overflow is an
error rather than a wrap, because a wrapped coefficient could make variables appear
to cancel when they do not.

## Library use

```rust
use patchproof_verify::{replay_bound, Status};

let cert: serde_json::Value = serde_json::from_str(&std::fs::read_to_string("a.json")?)?;
let (status, message) = replay_bound(&cert);
assert_eq!(status, Status::Verified);
```

## Development

```bash
cargo test        # 11 unit tests + the cross-implementation differential
cargo clippy -- -D warnings
```

<!-- portfolio:start -->
## Part of the hw-verify toolkit

Open tools for proving security properties of hardware and bounds checks.
They share one boundary: **everything open analyses a design you disclose in full.**

| Project | What it does |
|---|---|
| **▶ [Live demo](https://huggingface.co/spaces/nickh007/hw-verify)** | Constant-time checker in your browser — runs the real analyzer via Pyodide |
| [`hw-verify`](https://github.com/nickharris808/hw-verify) | One install, one command, all three checkers |
| [`ctbench`](https://github.com/nickharris808/ctbench) | Matched-pair constant-time RTL benchmark + leaderboard |
| [`patchproof`](https://github.com/nickharris808/patchproof) | Prove a bounds-check fix eliminates *every* violating input |
| **`patchproof-verify`** (you are here) | Re-check its certificates in Rust, with no shared code |
| [`ct-mask`](https://github.com/nickharris808/ct-mask) | First-order masking verification by two certificates |
| [`hw-verify-mcp`](https://github.com/nickharris808/hw-verify-mcp) | MCP server — the checkers, callable by AI agents |
| [`ct-audit-action`](https://github.com/nickharris808/ct-audit-action) | GitHub Action — fail a PR on a leaky completion signal |
| [verdicts](https://huggingface.co/datasets/nickh007/hw-verify) · [witness paths](https://huggingface.co/datasets/nickh007/hw-verify-paths) | Two datasets: what each design is, and why |

**The commercial boundary.** Proving a property to a third party who never receives
the design — a verdict bound to a commitment of a design that stays hidden — is a
different problem and a commercial one. It is not in any of these packages.
<!-- portfolio:end -->

## License

Apache-2.0. See [LICENSE](LICENSE).
