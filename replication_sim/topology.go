// Package main — topology.go
//
// Defines the three-level physical hierarchy used by the simulator:
//
//	Zone  →  Datacenter  →  Rack  →  Node
//
// ═══════════════════════════════════════════════════════════
// Distributions used
// ═══════════════════════════════════════════════════════════
//
//  1. Node / Rack / DC counts per parent level
//     → Discrete Uniform(min, max)
//       Simple and transparent; models real provisioning variability
//       where each parent level is filled to somewhere between a
//       configured minimum and maximum.
//
//  2. Failure probabilities at every level
//     → Uniform(lo, hi)  where lo/hi = base·scale ± spread·scale
//       Each entity at a given level draws its own failure prob from
//       this range so individual racks / DCs / zones have heterogeneous
//       reliability.  Scale factors shrink the mean by level:
//
//         Node  : scale = 1.00  (the base probability itself)
//         Rack  : scale = 0.30  (~3× rarer than a single node)
//         DC    : scale = 0.08  (~12× rarer than a single node)
//         Zone  : scale = 0.02  (~50× rarer than a single node)
//
//       Hard-coded floors (1e-2 / 1e-3 / 1e-4 / 1e-5) guarantee no
//       probability is ever exactly 0 — outages, however rare, happen.
//
//  3. Node hardware capacity
//     → Discrete Uniform(minCapacity, maxCapacity)
//       Models a mixed fleet of different hardware generations.
//       Capacity linearly scales the number of virtual nodes the node
//       occupies on the consistent-hash ring:
//           vnodes = round(baseVnodes × capacity / meanCapacity)
//       so higher-capacity nodes own proportionally more key-space.
//
// ═══════════════════════════════════════════════════════════
// Failure cascade logic (InjectFailures)
// ═══════════════════════════════════════════════════════════
//
//  When a higher-level entity fails its roll, every node beneath it
//  is killed immediately and its sub-tree is skipped.  This models
//  real correlated outages (power strip, top-of-rack switch, AZ).
//
//   For each Zone:
//     roll zone.FailProb → if yes: kill all nodes → next zone
//     For each DC in zone:
//       roll dc.FailProb → if yes: kill all nodes → next DC
//       For each Rack in DC:
//         roll rack.FailProb → if yes: kill all nodes → next rack
//         For each Node in rack:
//           roll node.FailProb → kill individually

package main

import (
	"fmt"
	"math"
	"math/rand"
)

// ─────────────────────────────────────────────────────────────────────────────
// Failure-probability scale factors per topology level
// ─────────────────────────────────────────────────────────────────────────────

const (
	rackScale = 0.30 // racks fail ~3× less often than individual nodes
	dcScale   = 0.08 // DCs fail ~12× less often than individual nodes
	zoneScale = 0.02 // zones fail ~50× less often than individual nodes
)

// Minimum failure-probability floor per level (never exactly 0).
const (
	nodeFloor = 1e-2
	rackFloor = 1e-3
	dcFloor   = 1e-4
	zoneFloor = 1e-5
)

// ─────────────────────────────────────────────────────────────────────────────
// Topology types
// ─────────────────────────────────────────────────────────────────────────────

// Zone models an availability zone (e.g. us-east-1a).
type Zone struct {
	ID       int
	FailProb float64
	DCs      []*Datacenter
}

// Datacenter models a single data centre within a zone.
type Datacenter struct {
	ID       int
	FailProb float64
	Racks    []*Rack
	Zone     *Zone
}

// Rack models a physical rack of servers inside a data centre.
type Rack struct {
	ID       int
	FailProb float64
	Nodes    []*Node
	DC       *Datacenter
}

// TopologyStats summarises the cluster that was built in a single iteration.
type TopologyStats struct {
	NumZones int
	NumDCs   int
	NumRacks int
	NumNodes int
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

// uniformInRange draws a float64 uniformly from [lo, hi].
// If lo >= hi it returns lo unchanged.
func uniformInRange(rng *rand.Rand, lo, hi float64) float64 {
	if lo >= hi {
		return lo
	}
	return lo + rng.Float64()*(hi-lo)
}

// clampF clamps v to [lo, hi].
func clampF(v, lo, hi float64) float64 {
	return math.Max(lo, math.Min(hi, v))
}

// failProbRange computes the [lo, hi] interval for Uniform failure-prob
// sampling at a given topology level.
//
//	base  – node-level base failure probability (Config.BaseFailProb)
//	spread – ± spread parameter (Config.FailSpread)
//	scale  – level-specific scale factor (rackScale / dcScale / zoneScale / 1.0)
//	floor  – minimum value the probability can take
func failProbRange(base, spread, scale, floor float64) (float64, float64) {
	center := base * scale
	lo := clampF(center-spread*scale, floor, 1.0)
	hi := clampF(center+spread*scale, floor, 1.0)
	if lo > hi {
		lo = hi
	}
	return lo, hi
}

// randIntInRange returns a random integer in [min, max] (inclusive).
// Panics if min > max.
func randIntInRange(rng *rand.Rand, min, max int) int {
	if min == max {
		return min
	}
	return min + rng.Intn(max-min+1)
}

// ─────────────────────────────────────────────────────────────────────────────
// BuildTopology
// ─────────────────────────────────────────────────────────────────────────────

// BuildTopology constructs the full Zone→DC→Rack→Node hierarchy according to
// cfg.  It returns the flat list of all physical nodes (for the hash ring),
// the zone slice (for failure injection), and a summary stat struct.
//
// All randomness is sourced from rng so simulations are reproducible given the
// same seed.
func BuildTopology(cfg Config, rng *rand.Rand) ([]*Node, []*Zone, TopologyStats) {
	var allNodes []*Node
	var zones []*Zone

	nodeID, dcID, rackID := 0, 0, 0

	// Mean capacity — used to centre vnode-count scaling.
	meanCap := float64(cfg.MinCapacity+cfg.MaxCapacity) / 2.0

	for z := 0; z < cfg.NumZones; z++ {

		// ── Zone failure probability: Uniform around base*zoneScale ──────────
		zlo, zhi := failProbRange(cfg.BaseFailProb, cfg.FailSpread, zoneScale, zoneFloor)
		zone := &Zone{
			ID:       z,
			FailProb: uniformInRange(rng, zlo, zhi),
		}
		zones = append(zones, zone)

		// ── Number of DCs in this zone: Discrete Uniform ─────────────────────
		numDCs := randIntInRange(rng, cfg.MinDCsPerZone, cfg.MaxDCsPerZone)

		for d := 0; d < numDCs; d++ {

			// DC failure probability
			dclo, dchi := failProbRange(cfg.BaseFailProb, cfg.FailSpread, dcScale, dcFloor)
			dc := &Datacenter{
				ID:       dcID,
				FailProb: uniformInRange(rng, dclo, dchi),
				Zone:     zone,
			}
			dcID++
			zone.DCs = append(zone.DCs, dc)

			// ── Number of racks: Discrete Uniform ────────────────────────────
			numRacks := randIntInRange(rng, cfg.MinRacksPerDC, cfg.MaxRacksPerDC)

			for r := 0; r < numRacks; r++ {

				// Rack failure probability
				rlo, rhi := failProbRange(cfg.BaseFailProb, cfg.FailSpread, rackScale, rackFloor)
				rack := &Rack{
					ID:       rackID,
					FailProb: uniformInRange(rng, rlo, rhi),
					DC:       dc,
				}
				rackID++
				dc.Racks = append(dc.Racks, rack)

				// ── Number of nodes: Discrete Uniform ────────────────────────
				numNodes := randIntInRange(rng, cfg.MinNodesPerRack, cfg.MaxNodesPerRack)

				for n := 0; n < numNodes; n++ {

					// Node failure probability
					nlo, nhi := failProbRange(cfg.BaseFailProb, cfg.FailSpread, 1.0, nodeFloor)
					nodeFailProb := uniformInRange(rng, nlo, nhi)

					// Capacity: Discrete Uniform(minCap, maxCap)
					cap := randIntInRange(rng, cfg.MinCapacity, cfg.MaxCapacity)

					// Vnode count proportional to capacity.
					// A node at mean capacity → exactly baseVnodes.
					// A node at 2× mean capacity → 2× baseVnodes, etc.
					vnodeCount := int(math.Round(float64(cfg.BaseVnodes) * float64(cap) / meanCap))
					if vnodeCount < 1 {
						vnodeCount = 1
					}

					node := NewNode(nodeID, cap, vnodeCount, nodeFailProb)
					nodeID++

					rack.Nodes = append(rack.Nodes, node)
					allNodes = append(allNodes, node)
				}
			}
		}
	}

	stats := TopologyStats{
		NumZones: len(zones),
		NumDCs:   dcID,
		NumRacks: rackID,
		NumNodes: len(allNodes),
	}

	return allNodes, zones, stats
}

// ─────────────────────────────────────────────────────────────────────────────
// InjectFailures
// ─────────────────────────────────────────────────────────────────────────────

// InjectFailures applies hierarchical, cascading failures to the cluster.
//
// If a zone fails, every node under it is killed and its sub-tree is skipped.
// Within a surviving zone, DCs are evaluated independently; within a surviving
// DC, racks are evaluated; within a surviving rack, nodes are evaluated.
//
// This produces correlated failure bursts that are far more damaging than
// the i.i.d. per-node model: a single rack failure can down 2–5 nodes at once.
func InjectFailures(zones []*Zone, rng *rand.Rand) {
	for _, z := range zones {
		if rng.Float64() < z.FailProb {
			// Entire zone fails → kill every node beneath it.
			killZone(z)
			continue
		}

		for _, dc := range z.DCs {
			if rng.Float64() < dc.FailProb {
				killDC(dc)
				continue
			}

			for _, rack := range dc.Racks {
				if rng.Float64() < rack.FailProb {
					killRack(rack)
					continue
				}

				for _, n := range rack.Nodes {
					if rng.Float64() < n.FailProb {
						n.Kill()
					}
				}
			}
		}
	}
}

// killZone kills every node in a zone.
func killZone(z *Zone) {
	for _, dc := range z.DCs {
		killDC(dc)
	}
}

// killDC kills every node in a data centre.
func killDC(dc *Datacenter) {
	for _, rack := range dc.Racks {
		killRack(rack)
	}
}

// killRack kills every node on a rack.
func killRack(rack *Rack) {
	for _, n := range rack.Nodes {
		n.Kill()
	}
}

// ─────────────────────────────────────────────────────────────────────────────
// TopologySummary
// ─────────────────────────────────────────────────────────────────────────────

// TopologySummary returns a compact human-readable string for logging.
func TopologySummary(stats TopologyStats) string {
	return fmt.Sprintf("Zones=%-2d  DCs=%-3d  Racks=%-4d  Nodes=%-4d",
		stats.NumZones, stats.NumDCs, stats.NumRacks, stats.NumNodes)
}
