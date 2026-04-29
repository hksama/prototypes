// Package main — main.go
//
// Entry point for the Dynamo replication simulator.
//
// Usage:
//   go run . [flags]
//
// Flags:
//   -nodes       int     number of nodes in the cluster     (default 10)
//   -replicas    int     replication factor R                (default 3)
//   -keys        int     number of keys to generate         (default 1000)
//   -failProb    float   node failure probability (0.0–1.0) (default 0.1)
//   -iterations  int     simulation rounds to average       (default 10)
//
// Example:
//   go run . -nodes=20 -replicas=5 -keys=5000 -failProb=0.3 -iterations=5

package main

import (
	"flag"
	"fmt"
	"strings"
)

func main() {
	// ---- CLI flags ----
	nodes := flag.Int("nodes", 10, "number of nodes in the cluster (N)")
	replicas := flag.Int("replicas", 3, "replication factor (R)")
	keys := flag.Int("keys", 1000, "number of keys to generate (K)")
	failProb := flag.Float64("failProb", 0.1, "node failure probability (0.0–1.0)")
	iterations := flag.Int("iterations", 10, "number of simulation rounds to average")
	flag.Parse()

	cfg := Config{
		NumNodes:      *nodes,
		ReplicaFactor: *replicas,
		NumKeys:       *keys,
		FailProb:      *failProb,
		Iterations:    *iterations,
	}

	// ---- header ----
	fmt.Println(strings.Repeat("=", 50))
	fmt.Println("   Dynamo Replication Simulation")
	fmt.Println(strings.Repeat("=", 50))
	fmt.Printf("Config: nodes=%d, replicas=%d, keys=%d, failProb=%.2f, iterations=%d\n\n",
		cfg.NumNodes, cfg.ReplicaFactor, cfg.NumKeys, cfg.FailProb, cfg.Iterations)

	// ---- run ----
	agg := RunMultiple(cfg)

	// ---- print results ----
	fmt.Println(strings.Repeat("-", 50))
	fmt.Printf("  Averaged Results (%d iterations)\n", cfg.Iterations)
	fmt.Println(strings.Repeat("-", 50))
	fmt.Printf("  Total Keys:        %.0f\n", agg.TotalKeys)
	fmt.Printf("  Successful Reads:  %.2f\n", agg.SuccessReads)
	fmt.Printf("  Availability:      %.2f%%\n", agg.Availability)
	fmt.Printf("  Keys Lost:         %.2f\n", agg.LostKeys)
	fmt.Printf("  Storage Overhead:  %.2f\n", agg.StorageOverhead)
	fmt.Println(strings.Repeat("-", 50))
}
