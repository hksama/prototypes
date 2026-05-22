# Replication Sim — Detailed Code Analysis

## File Map

| File | Role |
|---|---|
| [node.go](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/node.go) | Data model for a single storage node |
| [hashring.go](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/hashring.go) | Consistent-hash ring construction & replica lookup |
| [simulation.go](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/simulation.go) | Simulation engine — one round + multi-round averaging |
| [main.go](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/main.go) | CLI entry point, flag parsing, pretty-print output |

---

## What Each File Implements

### [`node.go`](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/node.go)

A `Node` is the smallest unit of the cluster. It has:

- **`ID int`** — an integer identity used both for human readability and as the seed for its virtual-node labels on the ring.
- **`Alive bool`** — a binary alive/dead flag. No intermediate states.
- **`Store map[string]bool`** — a presence map. It only records *which* keys exist on this node, not any actual value. This is intentional for a simulation that cares about availability, not data correctness.

Operations: `Put(key)`, `Get(key)` (returns bool), `Kill()`, `Revive()`.

---

### [`hashring.go`](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/hashring.go)

Implements **consistent hashing** — the core routing primitive of systems like Amazon Dynamo and Apache Cassandra.

#### The Ring

Internally the ring is a **sorted slice of `vnode` structs**, each holding:
```
{ hash uint32, nodeID int }
```

The full ring has `NumNodes × vnodes` entries. With defaults (10 nodes, 100 vnodes), that's 1,000 entries sorted by their 32-bit hash.

#### Hash Function

`hashKey(s string) uint32` — SHA-256 of the string, then **big-endian first 4 bytes** → uint32. This gives a uniform distribution over a 2³² space.

#### Replica Lookup — `GetReplicaNodes(key, R)`

1. Hash the key → position `h` on the ring.
2. Binary-search for the first vnode with `hash ≥ h` (clockwise walk start).
3. Walk clockwise (with wraparound), collecting the *physical* node behind each vnode, skipping duplicates, until `R` distinct physical nodes are collected.

This is the canonical Dynamo preference list construction.

---

### [`simulation.go`](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/simulation.go)

#### `RunSimulation(cfg, rng)` — one round

The lifecycle of a single round:

```
Step 1  Create N fresh nodes (all alive, empty stores)
Step 2  Build the hash ring (100 vnodes per node — hardcoded)
Step 3  For each of K keys:
            - compute preference list (R nodes)
            - PUT the key on each replica node
Step 4  Count storage overhead = Σ len(node.Store) across all nodes
Step 5  Failure injection: each node independently fails with prob FailProb
Step 6  Read phase: for each key, check if ≥ 1 replica is alive → readable
Step 7  Build and return Result{availability, lost keys, overhead}
```

#### `RunMultiple(cfg)` — N iterations

Calls `RunSimulation` `cfg.Iterations` times using the **same shared `rand.Rand`** (seeded with 42), then **averages** all fields of `Result` and returns an `AggregatedResult`.

---

### [`main.go`](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/main.go)

Standard Go CLI flag parsing. Defaults:

| Flag | Default | Meaning |
|---|---|---|
| `-nodes` | 10 | N physical nodes |
| `-replicas` | 3 | R = replication factor |
| `-keys` | 1000 | K keys to write |
| `-failProb` | 0.1 | 10% chance each node dies |
| `-iterations` | 10 | rounds to average over |

Calls `RunMultiple` and prints averaged results.

---

## The Virtual Nodes Parameter Explained

### What it is

Virtual nodes (vnodes) control how many **positions on the ring** each physical node occupies. Currently **hardcoded to 100** inside `RunSimulation` at line 69:

```go
ring := NewHashRing(nodes, 100)
```

This value is not exposed as a CLI flag.

### Why it matters

Without vnodes, each physical node would have exactly **one** random position on the ring. With 10 nodes and 1000 keys, each node would ideally hold ~100 keys, but because hash positions are random, the actual distribution would be extremely uneven — some nodes might hold 300 keys, others 20.

With **100 vnodes per node**, each physical node has 100 random positions spread across the ring. The keys are spread more evenly because the "arcs" between consecutive positions are much shorter and more uniformly distributed.

### Is 100 a good number?

| Vnodes | Effect |
|---|---|
| 1 | Severe imbalance; large key-space "hot spots" |
| 10–20 | Moderate imbalance, still noticeable |
| **100–150** | Good balance; standard starting point |
| 500+ | Near-perfect balance but memory overhead grows |

**Real-world reference**: Cassandra defaults to **256 vnodes per node**. Amazon Dynamo's original paper used **O(100–200)**. So 100 is reasonable but slightly conservative.

---

## The `Iterations` Parameter Explained

### What it does

Each iteration is a **completely independent simulation round**: fresh nodes, fresh stores, fresh failure dice rolls (but using the same shared RNG stream). After all iterations, every metric is **arithmetically averaged**.

### Why you need it

The failure injection step (`FailProb`) is stochastic. In a single run:
- With `failProb=0.1` and 10 nodes, you might get lucky and kill 0 nodes, or unlucky and kill 3.
- A single availability reading would be meaningless as a representative number.

With `Iterations=10`, you're computing **E[availability]** (expected value) by Monte Carlo averaging. More iterations → lower variance → more stable, trustworthy numbers.

### Rule of thumb

| Iterations | Use case |
|---|---|
| 5–10 | Quick exploration |
| 50–100 | Reasonable confidence in results |
| 500–1000 | Publishing / paper-quality statistics |

The fixed seed (`42`) makes results **reproducible** but means the same `cfg` always produces the same `agg` output — useful for debugging, less useful if you want to validate across different random seeds.

---

## Real-World Gaps & Inaccuracies

These are ordered roughly by severity / impact on realism.

### 1. ❌ Instantaneous, Simultaneous Failures (Most Impactful)
**Current**: All failures are injected in one atomic batch at a single instant, after all writes complete.  
**Reality**: Nodes fail and recover at random, independent times during the write and read phases. A node can fail mid-write, causing a partial replica to exist. Failures have **duration** (MTBF/MTTR). The simulation has no time dimension at all.

### 2. ❌ No Hinted Handoff / Sloppy Quorum
**Current**: If a replica node is dead during a PUT, the key simply isn't written there. There is no attempt to write to a "next available" node.  
**Reality**: Dynamo uses **hinted handoff** — writes that can't reach their target node are stored temporarily on another node with a "hint" to forward once the target recovers. This is central to Dynamo's write availability guarantee.

### 3. ❌ Binary Node State (Alive/Dead vs. Real Partial Failure)
**Current**: `Alive bool` — a node is either perfectly operational or completely dead.  
**Reality**: Nodes can be:
- Slow (high latency, not dead)
- Experiencing network partition (reachable from some nodes, not others)
- Out of disk space (accepting reads but rejecting writes)
- Experiencing GC pauses
- Returning stale data (lagging replica)

### 4. ❌ No Quorum Reads/Writes (W, R parameters)
**Current**: A write succeeds trivially to all healthy replicas. A read succeeds if *any one* replica is alive.  
**Reality**: Dynamo and Cassandra use configurable quorum: write must succeed on **W** nodes, read must contact **R** nodes, with the constraint `R + W > N` guaranteeing at least one overlap (strong consistency). The simulation has no concept of W or R quorum thresholds.

### 5. ❌ No Anti-Entropy / Read Repair
**Current**: Replicas are written once. There's no mechanism for a lagging replica to catch up.  
**Reality**: Systems use **Merkle tree-based anti-entropy** (background sync between replicas) and **read repair** (when a read detects divergent replicas, the coordinator repairs them on the fly).

### 6. ❌ No Vector Clocks / Versioning
**Current**: `Store map[string]bool` — a key either exists or it doesn't. There's no value, no version.  
**Reality**: Each write produces a new version. Concurrent writes to different replicas produce conflicting versions. Dynamo uses **vector clocks** to track causality and detect/resolve conflicts. Without this, "availability" in the simulation means nothing about correctness.

### 7. ❌ Uniform Failure Probability (No Correlated Failures)
**Current**: Each node fails independently with the same `FailProb`.  
**Reality**: Failures are heavily correlated:
- A rack failure takes down all nodes in that rack simultaneously.
- A network partition splits nodes into groups, not random individuals.
- A bad OS update deployed to a fleet kills a cluster of nodes at once.
Correlated failures make the real availability *much worse* than the i.i.d. model predicts.

### 8. ❌ No Node Heterogeneity
**Current**: All nodes are identical — same capacity, same failure probability, same performance.  
**Reality**: In production clusters, nodes may have different hardware generations (different CPU speeds, disk sizes). Cassandra and Dynamo weight vnodes proportionally to node capacity. A more powerful node should own more of the ring.

### 9. ❌ Keys Are Sequential, Not Real-Workload Distributed
**Current**: Keys are `"key-0"`, `"key-1"`, … `"key-999"` — deterministic sequential strings.  
**Reality**: Real workloads have **hot keys** (Zipf/Pareto distribution). Some keys receive 1000x more reads. This matters because hot-key replicas become bottlenecks. Sequential keys also happen to hash fairly uniformly, which flatters the ring's distribution quality.

### 10. ❌ No Network Topology Awareness
**Current**: The ring places vnodes randomly without awareness of rack or datacenter.  
**Reality**: Production consistent-hash rings use **rack-aware** placement: the R replicas for a key must span at least R different racks (or datacenters) so a single rack failure can't take down all replicas. Cassandra calls this `NetworkTopologyStrategy`.

### 11. ❌ Hardcoded Vnodes (100) Not Exposed as a Parameter
The vnode count is hardcoded at `simulation.go:69`. It should be a `Config` field and a CLI flag so you can study its effect on distribution uniformity.

### 12. ❌ Storage Overhead Metric Is Misleading
**Current**: `StorageOverhead = Σ len(node.Store)` — this is just the raw replica count, always equal to `K × R` (assuming enough nodes exist). It doesn't account for the actual data size, hot spots, or imbalance between nodes.

### 13. ❌ Fixed RNG Seed Removes Statistical Independence
**Current**: All iterations share a single `rand.New(rand.NewSource(42))`. This means the same config always produces exactly the same numbers. You can't run the sim twice with different seeds to get confidence intervals.

---

## Summary Table

| Gap | Severity | Fix Complexity |
|---|---|---|
| No time dimension / concurrent failures | 🔴 High | High |
| No hinted handoff | 🔴 High | Medium |
| Binary alive/dead state | 🔴 High | Medium |
| No W/R quorum | 🟠 Medium | Low |
| No anti-entropy/read repair | 🟠 Medium | High |
| No vector clocks | 🟠 Medium | High |
| Uniform i.i.d. failures (no correlation) | 🟠 Medium | Medium |
| No node heterogeneity | 🟡 Low | Low |
| Sequential keys, no hot spots | 🟡 Low | Low |
| No rack/DC awareness | 🟡 Low | Medium |
| Hardcoded vnodes | 🟡 Low | Trivial |
| Fixed RNG seed | 🟡 Low | Trivial |
