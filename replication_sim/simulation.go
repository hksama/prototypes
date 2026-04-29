// Package main — simulation.go
//
// Simulation engine for the Dynamo-style replication experiment.
//
// Workflow (per iteration):
//  1. Create N fresh nodes and build a consistent-hash ring.
//  2. Generate K deterministic keys ("key-0", "key-1", …).
//  3. Replicate each key to R nodes via the ring (PUT phase).
//  4. Compute storage overhead (total replica count across nodes).
//  5. Inject failures: each node independently fails with probability FailProb.
//  6. Read phase: for each key, attempt reads from its replicas.
//     A read succeeds if at least one replica is alive.
//  7. Collect and return metrics.
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
	NumNodes      int     // N – number of nodes in the cluster
	ReplicaFactor int     // R – number of replicas per key
	NumKeys       int     // K – number of random keys to generate
	FailProb      float64 // probability that each node fails (0.0–1.0)
	Iterations    int     // number of simulation rounds to average
}

// ------------------- per-iteration result -------------------

// Result captures metrics from a single simulation iteration.
type Result struct {
	TotalKeys       int     // always == Config.NumKeys
	SuccessReads    int     // keys readable (≥1 replica alive)
	LostKeys        int     // keys with 0 alive replicas
	StorageOverhead int     // sum of all node store sizes (total replicas)
	Availability    float64 // SuccessReads / TotalKeys * 100
}

// ------------------- aggregated result -------------------

// AggregatedResult holds averaged metrics over multiple iterations.
type AggregatedResult struct {
	TotalKeys       float64
	SuccessReads    float64
	LostKeys        float64
	StorageOverhead float64
	Availability    float64
}

// ------------------- single iteration -------------------

// RunSimulation executes one full simulation round:
// build → replicate → fail → read → measure.
func RunSimulation(cfg Config, rng *rand.Rand) Result {
	// ---- step 1: create nodes ----
	nodes := make([]*Node, cfg.NumNodes)
	for i := 0; i < cfg.NumNodes; i++ {
		nodes[i] = NewNode(i)
	}

	// ---- step 2: build hash ring (100 vnodes per physical node) ----
	ring := NewHashRing(nodes, 100)

	// ---- step 3: generate keys and replicate ----
	keys := make([]string, cfg.NumKeys)
	// keyReplicas maps each key to its set of replica nodes so we can
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

	// ---- step 4: compute storage overhead ----
	storageOverhead := 0
	for _, n := range nodes {
		storageOverhead += len(n.Store)
	}

	// ---- step 5: inject failures ----
	for _, n := range nodes {
		if rng.Float64() < cfg.FailProb {
			n.Kill()
		}
	}

	// ---- step 6: read phase ----
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

	// ---- step 7: build result ----
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
	}
}

// ------------------- multi-iteration runner -------------------

// RunMultiple executes cfg.Iterations rounds and returns the averaged
// metrics. Each iteration uses a fresh cluster and independent failures.
func RunMultiple(cfg Config) AggregatedResult {
	rng := rand.New(rand.NewSource(42)) // reproducible seed

	var agg AggregatedResult

	for i := 0; i < cfg.Iterations; i++ {
		r := RunSimulation(cfg, rng)
		agg.TotalKeys += float64(r.TotalKeys)
		agg.SuccessReads += float64(r.SuccessReads)
		agg.LostKeys += float64(r.LostKeys)
		agg.StorageOverhead += float64(r.StorageOverhead)
		agg.Availability += r.Availability
	}

	n := float64(cfg.Iterations)
	agg.TotalKeys /= n
	agg.SuccessReads /= n
	agg.LostKeys /= n
	agg.StorageOverhead /= n
	agg.Availability /= n

	return agg
}
