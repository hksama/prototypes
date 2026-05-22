// Package main — simulation.go
//
// Simulation engine for the Dynamo-style replication experiment.
//
// Workflow (per iteration):
//  1. Build a Zone→DC→Rack→Node topology via BuildTopology.
//     Node count is emergent from the topology parameters — there is no
//     fixed NumNodes.  Each node gets a capacity-proportional VnodeCount
//     and an individual failure probability.
//  2. Build a capacity-weighted consistent-hash ring (NewHashRing reads
//     node.VnodeCount directly).
//  3. Generate K deterministic keys ("key-0", "key-1", …).
//  4. Replicate each key to R nodes via the ring (PUT phase).
//  5. Compute storage overhead (total replica count across nodes).
//  6. Inject correlated failures via InjectFailures.
//     Higher-level failures cascade down (zone → DC → rack → node).
//  7. Read phase: for each key, attempt reads from its replicas.
//     A read succeeds if at least one replica is alive.
//  8. Collect and return metrics.
//
// RunMultiple executes Iterations rounds and returns averaged results.

package main

import (
	"fmt"
	"math/rand"
)

// ------------------- configuration -------------------

// Config holds all tunable simulation parameters.
type Config struct {
	// ── Topology sizing ─────────────────────────────────────────────────────
	// Total node count is emergent: NumZones × avg(DCsPerZone) ×
	// avg(RacksPerDC) × avg(NodesPerRack).
	NumZones        int // number of availability zones
	MinDCsPerZone   int // minimum data centres per zone  (Discrete Uniform)
	MaxDCsPerZone   int // maximum data centres per zone
	MinRacksPerDC   int // minimum racks per data centre  (Discrete Uniform)
	MaxRacksPerDC   int // maximum racks per data centre
	MinNodesPerRack int // minimum nodes per rack          (Discrete Uniform)
	MaxNodesPerRack int // maximum nodes per rack

	// ── Replication ──────────────────────────────────────────────────────────
	ReplicaFactor int // R – number of replicas per key
	NumKeys       int // K – number of random keys to generate

	// ── Failure model ────────────────────────────────────────────────────────
	// Each topology level draws its failure probability from a
	// Uniform(base·scale ± spread·scale) distribution.
	// Scale factors: node=1.00, rack=0.30, DC=0.08, zone=0.02.
	BaseFailProb float64 // base node-level failure probability (0.0–1.0)
	FailSpread   float64 // ± spread for the Uniform distribution

	// ── Capacity & ring weighting ─────────────────────────────────────────────
	// Node capacity drawn from Discrete Uniform(MinCapacity, MaxCapacity).
	// VnodeCount = round(BaseVnodes × capacity / meanCapacity).
	MinCapacity int // minimum capacity units per node
	MaxCapacity int // maximum capacity units per node
	BaseVnodes  int // vnodes for a node at exactly mean capacity

	// ── Simulation ───────────────────────────────────────────────────────────
	Iterations int // number of simulation rounds to average
}

// ------------------- per-iteration result -------------------

// Result captures metrics from a single simulation iteration.
type Result struct {
	TotalKeys       int     // always == Config.NumKeys
	SuccessReads    int     // keys readable (≥1 replica alive)
	LostKeys        int     // keys with 0 alive replicas
	StorageOverhead int     // sum of all node store sizes (total replicas written)
	Availability    float64 // SuccessReads / TotalKeys * 100

	// ── Topology snapshot for this iteration ─────────────────────────────────
	// Node count is emergent (varies per iteration due to Discrete Uniform draws).
	NumNodes int
	NumRacks int
	NumDCs   int
}

// ------------------- aggregated result -------------------

// AggregatedResult holds averaged metrics over multiple iterations.
type AggregatedResult struct {
	TotalKeys       float64
	SuccessReads    float64
	LostKeys        float64
	StorageOverhead float64
	Availability    float64

	// Averaged topology sizes.
	NumNodes float64
	NumRacks float64
	NumDCs   float64
}

// ------------------- single iteration -------------------

// RunSimulation executes one full simulation round:
// build topology → build ring → replicate → fail → read → measure.
func RunSimulation(cfg Config, rng *rand.Rand) Result {
	// ── step 1: build the zone→DC→rack→node hierarchy ────────────────────────
	nodes, zones, stats := BuildTopology(cfg, rng)

	// ── step 2: build capacity-weighted consistent-hash ring ─────────────────
	ring := NewHashRing(nodes)

	// ── step 3: generate keys and replicate ──────────────────────────────────
	keys := make([]string, cfg.NumKeys)
	// keyReplicas maps each key to its preference list so we can
	// query them in the read phase without re-hashing.
	keyReplicas := make(map[string][]*Node, cfg.NumKeys)

	for i := 0; i < cfg.NumKeys; i++ {
		k := fmt.Sprintf("key-%d", i)
		keys[i] = k

		replicas := ring.GetReplicaNodes(k, cfg.ReplicaFactor)
		keyReplicas[k] = replicas

		for _, n := range replicas {
			n.Put(k)
		}
	}

	// ── step 4: compute storage overhead ─────────────────────────────────────
	storageOverhead := 0
	for _, n := range nodes {
		storageOverhead += len(n.Store)
	}

	// ── step 5: inject hierarchical correlated failures ───────────────────────
	// Zone failures cascade to all DCs/racks/nodes beneath them.
	// DC failures cascade to all racks/nodes beneath them.
	// Rack failures cascade to all nodes on that rack.
	// Surviving nodes are each rolled individually against their FailProb.
	InjectFailures(zones, rng)

	// ── step 6: read phase ────────────────────────────────────────────────────
	successReads := 0
	lostKeys := 0

	for _, k := range keys {
		readable := false
		for _, n := range keyReplicas[k] {
			if n.Alive && n.Get(k) {
				readable = true
				break
			}
		}
		if readable {
			successReads++
		} else {
			lostKeys++
		}
	}

	// ── step 7: build result ──────────────────────────────────────────────────
	availability := 0.0
	if cfg.NumKeys > 0 {
		availability = float64(successReads) / float64(cfg.NumKeys) * 100.0
	}

	return Result{
		TotalKeys:       cfg.NumKeys,
		SuccessReads:    successReads,
		LostKeys:        lostKeys,
		StorageOverhead: storageOverhead,
		Availability:    availability,
		NumNodes:        stats.NumNodes,
		NumRacks:        stats.NumRacks,
		NumDCs:          stats.NumDCs,
	}
}

// ------------------- multi-iteration runner -------------------

// RunMultiple executes cfg.Iterations rounds and returns the averaged
// metrics.  Each iteration builds a fresh cluster with independent
// topology draws and failure rolls, using a shared RNG stream seeded
// at 42 for reproducibility.
func RunMultiple(cfg Config) AggregatedResult {
	rng := rand.New(rand.NewSource(42)) // fixed seed → reproducible results

	var agg AggregatedResult

	for i := 0; i < cfg.Iterations; i++ {
		r := RunSimulation(cfg, rng)
		agg.TotalKeys += float64(r.TotalKeys)
		agg.SuccessReads += float64(r.SuccessReads)
		agg.LostKeys += float64(r.LostKeys)
		agg.StorageOverhead += float64(r.StorageOverhead)
		agg.Availability += r.Availability
		agg.NumNodes += float64(r.NumNodes)
		agg.NumRacks += float64(r.NumRacks)
		agg.NumDCs += float64(r.NumDCs)
	}

	n := float64(cfg.Iterations)
	agg.TotalKeys /= n
	agg.SuccessReads /= n
	agg.LostKeys /= n
	agg.StorageOverhead /= n
	agg.Availability /= n
	agg.NumNodes /= n
	agg.NumRacks /= n
	agg.NumDCs /= n

	return agg
}
