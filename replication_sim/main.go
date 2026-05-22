// Package main — main.go
//
// Entry point for the Dynamo replication simulator.
//
// Usage:
//
//	go run . [flags]
//
// Flags — Topology:
//
//	-numZones          int     number of availability zones               (default 2)
//	-minDCsPerZone     int     minimum data centres per zone              (default 2)
//	-maxDCsPerZone     int     maximum data centres per zone              (default 4)
//	-minRacksPerDC     int     minimum racks per data centre              (default 3)
//	-maxRacksPerDC     int     maximum racks per data centre              (default 6)
//	-minNodesPerRack   int     minimum physical nodes per rack            (default 2)
//	-maxNodesPerRack   int     maximum physical nodes per rack            (default 5)
//
// Flags — Replication:
//
//	-replicas          int     replication factor R                       (default 3)
//	-keys              int     number of keys to generate                 (default 1000)
//
// Flags — Failure model:
//
//	-baseFailProb      float   base node failure probability (0.0–1.0)   (default 0.10)
//	-failSpread        float   ± spread for Uniform failure distributions (default 0.05)
//
//	Failure probability per level:
//	  Node  = Uniform(baseFailProb ± failSpread)
//	  Rack  = Uniform(baseFailProb·0.30 ± failSpread·0.30)
//	  DC    = Uniform(baseFailProb·0.08 ± failSpread·0.08)
//	  Zone  = Uniform(baseFailProb·0.02 ± failSpread·0.02)
//
// Flags — Capacity & ring weighting:
//
//	-minCapacity       int     minimum capacity units per node            (default 1)
//	-maxCapacity       int     maximum capacity units per node            (default 8)
//	-baseVnodes        int     vnodes for a node at mean capacity         (default 100)
//
//	VnodeCount = round(baseVnodes × capacity / meanCapacity)
//
// Flags — Simulation:
//
//	-iterations        int     rounds to average over                     (default 10)
//
// Example:
//
//	go run . -numZones=3 -baseFailProb=0.2 -failSpread=0.08 -replicas=3 -iterations=50

package main

import (
	"flag"
	"fmt"
	"strings"
)

func main() {
	// ── topology flags ────────────────────────────────────────────────────────
	numZones        := flag.Int("numZones",        2,    "number of availability zones")
	minDCsPerZone   := flag.Int("minDCsPerZone",   2,    "min data centres per zone")
	maxDCsPerZone   := flag.Int("maxDCsPerZone",   4,    "max data centres per zone")
	minRacksPerDC   := flag.Int("minRacksPerDC",   3,    "min racks per data centre")
	maxRacksPerDC   := flag.Int("maxRacksPerDC",   6,    "max racks per data centre")
	minNodesPerRack := flag.Int("minNodesPerRack", 2,    "min physical nodes per rack")
	maxNodesPerRack := flag.Int("maxNodesPerRack", 5,    "max physical nodes per rack")

	// ── replication flags ─────────────────────────────────────────────────────
	replicas := flag.Int("replicas", 3,    "replication factor R")
	keys     := flag.Int("keys",     1000, "number of keys to generate")

	// ── failure-model flags ───────────────────────────────────────────────────
	baseFailProb := flag.Float64("baseFailProb", 0.10, "base node failure probability (0.0–1.0)")
	failSpread   := flag.Float64("failSpread",   0.05, "± spread for Uniform failure distributions")

	// ── capacity & ring flags ─────────────────────────────────────────────────
	minCapacity := flag.Int("minCapacity", 1,   "min capacity units per node")
	maxCapacity := flag.Int("maxCapacity", 8,   "max capacity units per node")
	baseVnodes  := flag.Int("baseVnodes",  100, "vnodes for a node at mean capacity")

	// ── simulation flag ───────────────────────────────────────────────────────
	iterations := flag.Int("iterations", 10, "number of simulation rounds to average")

	flag.Parse()

	cfg := Config{
		NumZones:        *numZones,
		MinDCsPerZone:   *minDCsPerZone,
		MaxDCsPerZone:   *maxDCsPerZone,
		MinRacksPerDC:   *minRacksPerDC,
		MaxRacksPerDC:   *maxRacksPerDC,
		MinNodesPerRack: *minNodesPerRack,
		MaxNodesPerRack: *maxNodesPerRack,
		ReplicaFactor:   *replicas,
		NumKeys:         *keys,
		BaseFailProb:    *baseFailProb,
		FailSpread:      *failSpread,
		MinCapacity:     *minCapacity,
		MaxCapacity:     *maxCapacity,
		BaseVnodes:      *baseVnodes,
		Iterations:      *iterations,
	}

	sep := strings.Repeat("═", 58)
	thin := strings.Repeat("─", 58)

	// ── header ────────────────────────────────────────────────────────────────
	fmt.Println(sep)
	fmt.Println("       Dynamo Replication Simulation (hierarchical)")
	fmt.Println(sep)

	// ── topology config summary ───────────────────────────────────────────────
	meanCap := float64(*minCapacity+*maxCapacity) / 2.0
	approxNodes := float64(*numZones) *
		(float64(*minDCsPerZone+*maxDCsPerZone) / 2.0) *
		(float64(*minRacksPerDC+*maxRacksPerDC) / 2.0) *
		(float64(*minNodesPerRack+*maxNodesPerRack) / 2.0)

	fmt.Printf("  Topology   : zones=%-2d  DCs/zone=%d–%-2d  racks/DC=%d–%-2d  nodes/rack=%d–%d\n",
		*numZones,
		*minDCsPerZone, *maxDCsPerZone,
		*minRacksPerDC, *maxRacksPerDC,
		*minNodesPerRack, *maxNodesPerRack)
	fmt.Printf("               approx %.0f total nodes per iteration\n", approxNodes)
	fmt.Printf("  Replication: R=%-2d   keys=%d\n", *replicas, *keys)
	fmt.Printf("  Failure    : baseProb=%.2f  spread=±%.2f\n", *baseFailProb, *failSpread)
	fmt.Printf("               rack=%.3f  DC=%.4f  zone=%.5f  (±scaled)\n",
		*baseFailProb*rackScale, *baseFailProb*dcScale, *baseFailProb*zoneScale)
	fmt.Printf("  Capacity   : %d–%d units  meanCap=%.1f  baseVnodes=%d\n",
		*minCapacity, *maxCapacity, meanCap, *baseVnodes)
	fmt.Printf("  Iterations : %d\n\n", *iterations)

	// ── run ───────────────────────────────────────────────────────────────────
	agg := RunMultiple(cfg)

	// ── results ───────────────────────────────────────────────────────────────
	fmt.Println(thin)
	fmt.Printf("  Averaged Results (%d iterations)\n", *iterations)
	fmt.Println(thin)
	fmt.Printf("  Avg Topology    :  nodes=%.1f   racks=%.1f   DCs=%.1f\n",
		agg.NumNodes, agg.NumRacks, agg.NumDCs)
	fmt.Println(thin)
	fmt.Printf("  Total Keys      :  %.0f\n", agg.TotalKeys)
	fmt.Printf("  Successful Reads:  %.2f\n", agg.SuccessReads)
	fmt.Printf("  Availability    :  %.2f%%\n", agg.Availability)
	fmt.Printf("  Keys Lost       :  %.2f\n", agg.LostKeys)
	fmt.Printf("  Storage Overhead:  %.2f\n", agg.StorageOverhead)
	fmt.Println(thin)
}
