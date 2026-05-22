# Hierarchical Failure Model + Capacity-Weighted Vnodes

Replaces the flat i.i.d. failure model with a correlated, three-level hierarchy (Zone → Datacenter → Rack → Node). Node counts at each level are drawn from configurable distributions. Each node gets a random hardware capacity, which drives its vnode weight on the hash ring.

---

## Distributions Used (Per Feature)

| Feature | Distribution | Why |
|---|---|---|
| Node count per rack | **Discrete Uniform(min, max)** | Simple, models real hardware provisioning variability |
| Rack count per DC | **Discrete Uniform(min, max)** | Same rationale |
| DC count per Zone | **Discrete Uniform(min, max)** | Same rationale |
| Node failure prob | **Uniform(base − spread, base + spread)** | Each machine has slightly different reliability; uniform is transparent |
| Rack failure prob | **Uniform(base·0.30 − spread·0.30, base·0.30 + spread·0.30)** | Racks fail ~3× less often than individual nodes |
| DC failure prob | **Uniform(base·0.08 − spread·0.08, base·0.08 + spread·0.08)** | DCs fail ~12× less often |
| Zone failure prob | **Uniform(base·0.02 − spread·0.02, base·0.02 + spread·0.02)** | AZ outages are rare; ~50× less likely than individual nodes |
| Node capacity | **Discrete Uniform(minCap, maxCap)** | Hardware tiers; uniform between min/max is realistic for a mixed fleet |
| Vnode count | **Proportional**: `round(baseVnodes × cap / meanCap)` | Capacity-weighted — more powerful node → larger ring slice |

> [!IMPORTANT]
> All failure probability floors are hardcoded non-zero minimums (`1e-5` for zones, `1e-4` for DCs, `1e-3` for racks, `1e-2` for nodes) so the probability is never exactly 0.

---

## Failure Cascade Logic

When a higher-level entity fails, **all** nodes beneath it are immediately killed, regardless of their individual probabilities. This is the key realism improvement — a rack failure takes down every node on it.

```
For each Zone:
  Roll zone.FailProb → if fail: kill all nodes under zone, skip to next zone
  For each DC in zone:
    Roll dc.FailProb → if fail: kill all nodes under DC, skip to next DC
    For each Rack in DC:
      Roll rack.FailProb → if fail: kill all nodes in rack, skip to next rack
      For each Node in rack:
        Roll node.FailProb → kill individually
```

---

## Proposed Changes

### `topology.go` — [NEW]

New file owning the entire hierarchy and failure injection.

**Types**:
```go
type Zone       { ID, FailProb, DCs []*Datacenter }
type Datacenter { ID, FailProb, Racks []*Rack, Zone *Zone }
type Rack       { ID, FailProb, Nodes []*Node, DC *Datacenter }
type TopologyStats { NumZones, NumDCs, NumRacks, NumNodes int }
```

**Functions**:
- `BuildTopology(cfg, rng) ([]*Node, []*Zone, TopologyStats)` — constructs the full hierarchy using the distributions above.
- `InjectFailures(zones, rng)` — hierarchical cascading failure injection.
- Internal helpers: `uniformInRange`, `clamp`, `failProbRange`.

---

### [`node.go`](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/node.go) — MODIFY

Add three new fields:

```diff
type Node struct {
    ID    int
    Alive bool
    Store map[string]bool
+   Capacity   int     // relative capacity units (from discrete uniform)
+   VnodeCount int     // pre-computed vnode count for this node's capacity
+   FailProb   float64 // individual failure probability (from uniform dist)
}
```

`NewNode` signature changes to: `NewNode(id, capacity, vnodeCount int, failProb float64)`.

---

### [`hashring.go`](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/hashring.go) — MODIFY

Drop the uniform `vnodes int` parameter from `NewHashRing`. Instead, read `node.VnodeCount` per node:

```diff
-func NewHashRing(nodes []*Node, vnodes int) *HashRing {
+func NewHashRing(nodes []*Node) *HashRing {
     ...
-    for v := 0; v < vnodes; v++ {
+    for v := 0; v < n.VnodeCount; v++ {
```

Remove the `vnodes int` field from the `HashRing` struct.

---

### [`simulation.go`](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/simulation.go) — MODIFY

**Config** — replace `NumNodes` and `FailProb` with the full topology + failure config:

```go
type Config struct {
    // Topology sizing (node count is emergent)
    NumZones        int
    MinDCsPerZone   int
    MaxDCsPerZone   int
    MinRacksPerDC   int
    MaxRacksPerDC   int
    MinNodesPerRack int
    MaxNodesPerRack int

    // Replication
    ReplicaFactor   int
    NumKeys         int

    // Failure model
    BaseFailProb    float64  // base node-level probability (0.0–1.0)
    FailSpread      float64  // ± spread for the uniform distributions

    // Capacity & ring weighting
    MinCapacity     int      // min capacity units per node
    MaxCapacity     int      // max capacity units per node
    BaseVnodes      int      // vnodes for a node at mean capacity

    // Simulation
    Iterations      int
}
```

**Result** — add topology stats fields:
```go
type Result struct {
    ...existing...
    NumNodes int
    NumRacks int
    NumDCs   int
}
```

**RunSimulation** — replace steps 1, 2, 5:
- Step 1: `BuildTopology(cfg, rng)` instead of manual node creation.
- Step 2: `NewHashRing(nodes)` (no vnode arg).
- Step 5: `InjectFailures(zones, rng)` instead of the flat loop.

**RunMultiple** — aggregate `NumNodes`, `NumRacks`, `NumDCs`.

---

### [`main.go`](file:///Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim/main.go) — MODIFY

Replace `-nodes` / `-failProb` flags with the new set. New CLI defaults:

| Flag | Default | Produces |
|---|---|---|
| `-numZones` | 2 | 2 AZs |
| `-minDCsPerZone` / `-maxDCsPerZone` | 2 / 4 | 2–4 DCs per AZ |
| `-minRacksPerDC` / `-maxRacksPerDC` | 3 / 6 | 3–6 racks per DC |
| `-minNodesPerRack` / `-maxNodesPerRack` | 2 / 5 | 2–5 nodes per rack |
| `-replicas` | 3 | R=3 |
| `-keys` | 1000 | 1000 keys |
| `-baseFailProb` | 0.10 | 10% base node failure |
| `-failSpread` | 0.05 | ±5% spread |
| `-minCapacity` / `-maxCapacity` | 1 / 8 | capacity units |
| `-baseVnodes` | 100 | vnodes at mean cap |
| `-iterations` | 10 | rounds to average |

Default topology produces roughly **2 × 3 × 4.5 × 3.5 ≈ 94 nodes**, replacing the old `-nodes=10` default with something more realistic.

Output will show an averaged topology summary line in addition to the existing metrics.

---

## Verification Plan

### Build test
```
cd /Users/harshavardhankolhatkar04/Desktop/Projects/Distributed_Systems/prototypes/replication_sim
go build ./...
```

### Smoke run (defaults)
```
go run . 
```
Expected: topology summary shows Zones/DCs/Racks/Nodes, availability prints cleanly.

### Cascade test (high DC fail prob)
```
go run . -baseFailProb=0.0 -failSpread=0.0 -numZones=1 -minDCsPerZone=1 -maxDCsPerZone=1 -minRacksPerDC=1 -maxRacksPerDC=1 -minNodesPerRack=5 -maxNodesPerRack=5 -keys=100
```
With `baseFailProb=0` and `failSpread=0`, all probs floor to their minimum constants → availability should be near 100%.

### High failure test
```
go run . -baseFailProb=0.9 -failSpread=0.05 -replicas=3 -numZones=3 -iterations=50
```
Expected: low availability, demonstrating correlated cascade is working.
