# feral-scavenger

A small demo agent that exercises the Vitalis **irreducible core** — sense,
metabolize, remember, replicate — plus the Feral profile's barter path
(negotiate). It runs entirely on a **simulated mesh**, so no network or special
hardware is required.

## What it shows

1. **Scavenge scenario** (`sense` + `metabolism` + `replicate`)
   - A "rich" peer advertises spare energy on the simulated mesh.
   - The scavenger discovers it, sips a little power, and — when its own battery
     drops below a safe floor — checkpoints itself and authorizes a redundant
     copy (proves goals **G1–G3**).
2. **Negotiate scenario** (`negotiate`, Feral profile §6 weighting)
   - The scavenger offers compute in exchange for the rich peer's energy.
   - The rich peer accepts and delivers; the settlement is credited on both
     ledgers. Demonstrates the signed barter protocol with replay/spoof
     protection.

## Run it

```sh
cargo run -p feral-scavenger
```

You should see the scavenger discover a peer, sip power, checkpoint under
pressure, and then run a two-agent barter that settles correctly on both sides.

## The simulated mesh

`SimulatedMesh` is a stand-in for a real libp2p transport (the documented
swap-in point for production). The peer-discovery and barter logic are identical
regardless of the backing transport, so the demo's behavior carries over to a
real mesh unchanged.
